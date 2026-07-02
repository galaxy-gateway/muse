//! Audio spine: decode (symphonia, ffmpeg fallback) -> rtrb ring -> cpal output,
//! with a triple_buffer tap in the cpal callback feeding the live oscilloscope.
//!
//! The cpal callback is the single source of truth for "what is audible now": it
//! pulls PCM from the ring and copies the same block into the scope buffer, so the
//! visualization is sample-synced to the DAC. No locks/allocations on that path.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};

/// Frames in the live-scope window. 2048 ~ 46ms at 44.1k. The published scope
/// buffer is interleaved **stereo**, so it holds `SCOPE_LEN * 2` samples (L,R…)
/// — mono presets fold to `0.5*(l+r)`, the XY preset uses the pair directly.
pub const SCOPE_LEN: usize = 2048;

/// Commands sent from the UI thread to the decode thread.
pub enum TransportCmd {
    Open(PathBuf),
    Toggle,
    #[allow(dead_code)] // used by later phases (auto-advance, compare A/B)
    Pause,
    #[allow(dead_code)]
    Play,
    SeekRel(f64),
    /// Seek to an absolute position in seconds.
    SeekTo(f64),
    VolRel(f32),
    /// Set absolute volume (0.0..1.0) — used to restore the saved session volume.
    SetVol(f32),
}

/// Fully decoded track: interleaved **stereo** f32 at `sample_rate`.
pub struct DecodedAudio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl DecodedAudio {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}

/// Seconds of decoded audio kept ahead of / behind the playhead. The decoder
/// throttles to stay within `AHEAD` and trims played frames older than `BEHIND`,
/// so memory is bounded to ~(AHEAD+BEHIND) seconds no matter how long the track
/// is — a 3-minute song and a 3-hour podcast use the same ~45 s window.
const WINDOW_AHEAD_SECS: u64 = 30;
const WINDOW_BEHIND_SECS: u64 = 15;

/// Decode lookahead for a stream that hasn't really been listened to yet.
/// Expanded to `WINDOW_AHEAD_SECS` only once ~0.5s has actually played, so a
/// track that is immediately switched away from again (held `n`/`p`, rapid
/// clicking) allocates ~1MB instead of a full 30s window per press — the churn
/// of full windows is what ballooned RSS during rapid cycling (freed, but
/// cached dirty by the allocator for a long while). Preloads idle at this small
/// window too; decode outruns playback by ~50-100x, so the post-adoption
/// expansion never risks an underrun.
const WARMUP_AHEAD_SECS: u64 = 3;

/// A track decoded *incrementally* into a bounded sliding window. The decoder
/// keeps only [`playhead - BEHIND` .. `playhead + AHEAD`] resident; playback
/// reads from it by absolute frame. A fully-decoded (gapless-preloaded) track is
/// just a `StreamBuf` created `done` with the whole thing resident and no trim.
struct StreamBuf {
    /// Device-rate interleaved-stereo samples for the current window.
    samples: Mutex<Vec<f32>>,
    /// Absolute device frame of `samples[0]`. Only mutated while holding the
    /// `samples` lock (during append/trim), so readers that hold the lock see a
    /// consistent `(base, samples)` pair.
    base: AtomicU64,
    /// Absolute device frame decoded up to (`base + samples.len()/2`).
    ready_abs: AtomicU64,
    /// Absolute device frame the consumer has reached — drives throttle + trim.
    play_head: AtomicU64,
    /// Decoding finished (EOF or error) — no more frames will be appended.
    done: AtomicBool,
    /// Estimated total device frames from the container, or 0 if unknown.
    total: AtomicU64,
    /// Set to stop the background decoder early when the track is superseded.
    cancel: AtomicBool,
}

impl StreamBuf {
    fn empty(base: u64) -> Arc<Self> {
        Arc::new(Self {
            samples: Mutex::new(Vec::new()),
            base: AtomicU64::new(base),
            ready_abs: AtomicU64::new(base),
            play_head: AtomicU64::new(base),
            done: AtomicBool::new(false),
            total: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
        })
    }

    /// Shared trim logic used by both `push` and `trim_behind`: drop frames from
    /// the front of `s` older than `play_head - lookbehind`, advancing `base` to
    /// match. `keep_from` never trims below `base` since it's saturating, so
    /// `u64::MAX` lookbehind is effectively "never trim". After a large drop
    /// (bigger than the window itself — i.e. a one-off catch-up rather than the
    /// usual per-chunk trickle) `Vec::drain` alone wouldn't shrink capacity, so
    /// reclaim it with `shrink_to_fit` — that's how the full-decode fallback's
    /// initial gigabytes actually get returned to the allocator.
    fn trim_locked(s: &mut Vec<f32>, base: &AtomicU64, play_head: &AtomicU64, lookbehind: u64) {
        let base_v = base.load(Ordering::Relaxed);
        let keep_from = play_head.load(Ordering::Relaxed).saturating_sub(lookbehind);
        if keep_from > base_v {
            let drop = ((keep_from - base_v) as usize).min(s.len() / 2);
            if drop == 0 {
                return;
            }
            s.drain(0..drop * 2);
            if drop as u64 > lookbehind {
                s.shrink_to_fit();
            }
            base.store(base_v + drop as u64, Ordering::Relaxed);
        }
    }

    /// Append a device-rate chunk and trim the played-out front (drop frames more
    /// than `lookbehind` before the playhead). Pass `u64::MAX` to never trim
    /// (full buffers). Keeps `base`/`ready_abs` consistent under the lock.
    fn push(&self, chunk: &[f32], lookbehind: u64) {
        if chunk.is_empty() {
            return;
        }
        let mut s = self.samples.lock().unwrap();
        s.extend_from_slice(chunk);
        Self::trim_locked(&mut s, &self.base, &self.play_head, lookbehind);
        self.ready_abs.store(
            self.base.load(Ordering::Relaxed) + (s.len() / 2) as u64,
            Ordering::Release,
        );
    }

    /// Trim the played-out front without appending anything — used to bound the
    /// full-decode fallback (which `push`es the whole file up front with
    /// `u64::MAX` lookbehind, so `push` itself never trims it) once playback has
    /// somewhere to trim behind.
    fn trim_behind(&self, lookbehind: u64) {
        let mut s = self.samples.lock().unwrap();
        Self::trim_locked(&mut s, &self.base, &self.play_head, lookbehind);
    }
}

/// Number of peak bins in the live waveform, filled progressively as a track
/// decodes. Matches the UI's request resolution.
const WAVE_BINS: usize = 1600;

/// Shared, progressively-filled waveform: min/max peak per absolute bin across
/// the whole track. The streaming decoder folds each decoded chunk into it (at
/// absolute positions, so it survives seeks); the UI reads a snapshot each frame.
struct WaveShared {
    bins: Mutex<Vec<(f32, f32)>>,
    total: AtomicU64, // total device frames (bin width = total / bins.len())
    /// Set once any real peak has been folded in — until then the UI keeps its
    /// existing waveform instead of showing empty bins.
    active: AtomicBool,
}

impl WaveShared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            bins: Mutex::new(vec![(0.0, 0.0); WAVE_BINS]),
            total: AtomicU64::new(0),
            active: AtomicBool::new(false),
        })
    }

    /// Fold a device-rate chunk starting at absolute frame `abs0` into the bins.
    fn add(&self, chunk: &[f32], abs0: u64) {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 || chunk.is_empty() {
            return;
        }
        self.active.store(true, Ordering::Relaxed);
        let n = WAVE_BINS as u64;
        let mut bins = self.bins.lock().unwrap();
        for (i, s) in chunk.chunks_exact(2).enumerate() {
            let abs = abs0 + i as u64;
            let bi = ((abs * n) / total).min(n - 1) as usize;
            let m = 0.5 * (s[0] + s[1]);
            let b = &mut bins[bi];
            if m < b.0 {
                b.0 = m;
            }
            if m > b.1 {
                b.1 = m;
            }
        }
    }
}

/// Stateful streaming linear resampler for interleaved stereo. Cheap and, unlike
/// resampling each chunk independently, click-free across chunk boundaries
/// because the fractional phase + previous frame carry over. `step == 1` (rate
/// match) is exact passthrough. Slightly softer than the offline windowed-sinc
/// used for full decodes, but the right tradeoff for instant, gap-free streaming.
struct StreamResampler {
    step: f64, // native frames advanced per output frame
    phase: f64,
    prev: [f32; 2],
    have_prev: bool,
}

impl StreamResampler {
    fn new(native_sr: u32, device_sr: u32) -> Self {
        Self {
            step: native_sr as f64 / device_sr as f64,
            phase: 0.0,
            prev: [0.0; 2],
            have_prev: false,
        }
    }

    /// Feed one native interleaved-stereo chunk; append device-rate output.
    fn feed(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for f in 0..input.len() / 2 {
            let cur = [input[f * 2], input[f * 2 + 1]];
            if !self.have_prev {
                self.prev = cur;
                self.have_prev = true;
                continue;
            }
            while self.phase < 1.0 {
                let p = self.phase as f32;
                out.push(self.prev[0] + (cur[0] - self.prev[0]) * p);
                out.push(self.prev[1] + (cur[1] - self.prev[1]) * p);
                self.phase += self.step;
            }
            self.phase -= 1.0;
            self.prev = cur;
        }
    }
}

/// Gate serializing full-decode fallbacks (see `stream_decode`): at most one
/// whole-file decode may be resident at a time, process-wide.
static FALLBACK_GATE: Mutex<()> = Mutex::new(());

/// Background entry: decode `path` into `sb` incrementally, filling `wave` with
/// peaks, starting at `seek_frames` (device frames; 0 = from the top). Streams via
/// symphonia when possible; if that can't even start (unsupported / needs ffmpeg),
/// falls back to a one-shot full decode from the top. Always marks `done`.
fn stream_decode(
    path: &Path,
    device_sr: u32,
    sb: Arc<StreamBuf>,
    wave: Arc<WaveShared>,
    seek_frames: u64,
) {
    // Stream via symphonia (disk file or archive bytes). Only fall back to a full
    // decode when streaming can't even start (e.g. a format that needs ffmpeg).
    let streamed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stream_symphonia(path, device_sr, &sb, &wave, seek_frames)
    }))
    .unwrap_or_else(|_| Err(anyhow!("decoder panicked")));
    // Full-decode fallback only for the initial play (not a seek). The fallback
    // is a single blocking decode of the whole file — running it per seek on a
    // large track (and it can't honor the seek anyway) is how rapid
    // waveform-clicking piled up into an out-of-memory crash.
    if streamed.is_err() && seek_frames == 0 && sb.ready_abs.load(Ordering::Relaxed) == 0 {
        // Serialize fallbacks process-wide: each one holds an entire decoded
        // track resident at its peak, so rapid track cycling must never stack
        // them. Waiting is cancel-aware — a superseded track gives up its turn.
        let gate = loop {
            if sb.cancel.load(Ordering::Relaxed) {
                break None;
            }
            match FALLBACK_GATE.try_lock() {
                Ok(g) => break Some(g),
                Err(std::sync::TryLockError::Poisoned(p)) => break Some(p.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    thread::sleep(Duration::from_millis(10))
                }
            }
        };
        if gate.is_some() {
            // Retry once: a decode can fail transiently, and a track that decodes
            // to nothing would otherwise just fail to play.
            for attempt in 0..2 {
                if sb.cancel.load(Ordering::Relaxed) {
                    break;
                }
                match load_track_safe(path, device_sr, &sb.cancel) {
                    Ok(d) if !sb.cancel.load(Ordering::Relaxed) => {
                        // Fallback can't honor a mid-file seek — it decodes from the top.
                        // It's a full buffer, so never trim (u64::MAX lookbehind).
                        let total = d.frames() as u64;
                        sb.base.store(0, Ordering::Relaxed);
                        sb.total.store(total, Ordering::Relaxed);
                        wave.total.store(total, Ordering::Relaxed);
                        wave.add(&d.samples, 0);
                        sb.push(&d.samples, u64::MAX);
                        break;
                    }
                    _ => {
                        if attempt == 0 {
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                }
            }
        }
    }
    sb.done.store(true, Ordering::Release);
}

/// Stream-decode a symphonia-readable file into `sb` (optionally seeked to
/// `seek_frames`): decode packet-by-packet, fold to stereo, resample to the
/// device rate, append, and fold peaks into `wave` at absolute positions.
fn stream_symphonia(
    path: &Path,
    device_sr: u32,
    sb: &StreamBuf,
    wave: &WaveShared,
    seek_frames: u64,
) -> Result<()> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymErr;
    use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::units::Time;

    // Stream from disk, or from an archive entry's in-memory bytes (also seekable
    // via `MemSource`) — so zipped songs start immediately too, no full-decode
    // wait up front.
    let mut hint = Hint::new();
    let mss = if let Some((arc, inner)) = crate::archive::split_virtual(path) {
        let bytes = crate::archive::read(&arc, &inner)
            .ok_or_else(|| anyhow!("archive entry unreadable: {inner}"))?;
        if let Some(ext) = Path::new(&inner).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        MediaSourceStream::new(Box::new(MemSource::new(bytes)), Default::default())
    } else {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let file = File::open(path)?;
        MediaSourceStream::new(Box::new(file), Default::default())
    };
    let fmt_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &fmt_opts,
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default track"))?
        .clone();
    let track_id = track.id;
    let sr = track.codec_params.sample_rate.unwrap_or(44100);
    // Publish the real total up-front (device frames) so the seek bar + waveform
    // are correct from the first sound, even though only a prefix is decoded.
    let total = track
        .codec_params
        .n_frames
        .map(|n| (n as f64 * device_sr as f64 / sr as f64) as u64)
        .unwrap_or(0);
    sb.total.store(total, Ordering::Release);
    wave.total.store(total, Ordering::Relaxed);
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    // Seek into the file if requested, then reset the decoder's state. `base` is
    // pinned to where the reader actually landed so positions stay accurate.
    if seek_frames > 0 {
        let secs = seek_frames as f64 / device_sr as f64;
        let time = Time::new(secs.trunc() as u64, secs.fract());
        if let Ok(seeked) = format.seek(
            SeekMode::Coarse,
            SeekTo::Time {
                time,
                track_id: Some(track_id),
            },
        ) {
            let base = (seeked.actual_ts as f64 * device_sr as f64 / sr as f64) as u64;
            sb.base.store(base, Ordering::Relaxed);
            sb.ready_abs.store(base, Ordering::Relaxed);
        }
        decoder.reset();
    }

    let full_ahead = device_sr as u64 * WINDOW_AHEAD_SECS;
    let warm_ahead = device_sr as u64 * WARMUP_AHEAD_SECS;
    // The playhead crossing half a second past where this stream started is
    // proof the track is actually being listened to — until then, hold the
    // small warmup window (see WARMUP_AHEAD_SECS).
    let warmed_at = sb.play_head.load(Ordering::Relaxed) + device_sr as u64 / 2;
    let lookbehind = device_sr as u64 * WINDOW_BEHIND_SECS;
    let mut resamp = StreamResampler::new(sr, device_sr);
    let mut native: Vec<f32> = Vec::new();
    let mut out: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        // Throttle: don't decode more than the lookahead past the playhead, so
        // the resident window stays bounded regardless of track length.
        loop {
            if sb.cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            let head = sb.play_head.load(Ordering::Relaxed);
            let lookahead = if head >= warmed_at {
                full_ahead
            } else {
                warm_ahead
            };
            if sb.ready_abs.load(Ordering::Acquire) < head + lookahead {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        let trim_start = packet.trim_start as usize;
        let trim_end = packet.trim_end as usize;
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let ch = decoded.spec().channels.count().max(1);
                if sbuf.is_none() {
                    let spec = *decoded.spec();
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sbf = sbuf.as_mut().unwrap();
                sbf.copy_interleaved_ref(decoded);
                let samples = sbf.samples();
                let frames = samples.len() / ch;
                let start = trim_start.min(frames);
                let end = frames.saturating_sub(trim_end).max(start);
                native.clear();
                for f in start..end {
                    let base = f * ch;
                    let l = samples[base];
                    let r = if ch >= 2 { samples[base + 1] } else { l };
                    native.push(l);
                    native.push(r);
                }
                out.clear();
                resamp.feed(&native, &mut out);
                // Fold peaks at absolute positions before publishing the frames.
                let abs0 = sb.ready_abs.load(Ordering::Relaxed);
                wave.add(&out, abs0);
                sb.push(&out, lookbehind);
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }
    Ok(())
}

/// A seekable, in-memory `MediaSource` over archive-entry bytes, so symphonia
/// can decode audio that lives inside an archive without it touching disk.
struct MemSource {
    data: Arc<Vec<u8>>,
    pos: u64,
}

impl MemSource {
    fn new(data: Arc<Vec<u8>>) -> Self {
        Self { data, pos: 0 }
    }
}

impl std::io::Read for MemSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let pos = self.pos.min(self.data.len() as u64) as usize;
        let remaining = &self.data[pos..];
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.pos = (pos + n) as u64;
        Ok(n)
    }
}

impl std::io::Seek for MemSource {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::SeekFrom;
        let len = self.data.len() as i64;
        let base = match from {
            SeekFrom::Start(p) => {
                self.pos = p;
                return Ok(self.pos);
            }
            SeekFrom::End(d) => len + d,
            SeekFrom::Current(d) => self.pos as i64 + d,
        };
        if base < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        self.pos = base as u64;
        Ok(self.pos)
    }
}

impl symphonia::core::io::MediaSource for MemSource {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }
}

/// Owns the cpal stream (kept alive) and the handles the UI reads each frame.
pub struct AudioEngine {
    cmd_tx: Sender<TransportCmd>,
    /// Requests to the background loader thread to decode a track ahead of time
    /// for gapless transitions.
    load_tx: Sender<PathBuf>,
    /// Path of each track the decode thread spliced into at a gapless boundary,
    /// so the UI can sync `now_playing` without re-issuing `Open`.
    advance_rx: Receiver<PathBuf>,
    /// The current streaming track's progressively-filled waveform, or `None`
    /// (fully-decoded / gapless track — the UI computes its own then).
    current_bins: Arc<Mutex<Option<Arc<WaveShared>>>>,
    /// True while a track is playing but no audio has decoded yet (the UI shows a
    /// loading spinner). Cleared as soon as the first frames are ready.
    buffering: Arc<AtomicBool>,
    scope_out: triple_buffer::Output<Vec<f32>>,
    device_sr: u32,
    pos_frames: Arc<AtomicU64>,
    dur_frames: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    volume: Arc<AtomicU32>, // f32 bits
    _stream: cpal::Stream,  // !Send: AudioEngine must stay on the UI thread
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output device"))?;
        let config = device.default_output_config()?;
        let device_sr = config.sample_rate();
        let out_channels = config.channels() as usize;

        // ~250ms stereo ring between decode and DAC.
        let ring_cap = (device_sr as usize / 4) * 2;
        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(ring_cap);

        let (scope_in, scope_out) =
            triple_buffer::TripleBuffer::new(&vec![0f32; SCOPE_LEN * 2]).split();
        let mut scope_in = scope_in;

        let pos_frames = Arc::new(AtomicU64::new(0));
        let dur_frames = Arc::new(AtomicU64::new(0));
        let playing = Arc::new(AtomicBool::new(false));
        let volume = Arc::new(AtomicU32::new(0.8f32.to_bits()));
        // Bumped by the decode thread on every Open so the cpal callback drops
        // the previous track's buffered tail at once — a manual song switch is
        // instant instead of playing out ~250ms of stale audio first.
        let flush_gen = Arc::new(AtomicU64::new(0));

        // --- cpal output callback (real-time thread) ------------------------
        let vol_cb = volume.clone();
        let flush_cb = flush_gen.clone();
        let mut last_flush = 0u64;
        let scope_cap = SCOPE_LEN * 2; // interleaved stereo
        let mut scope_ring = vec![0f32; scope_cap];
        let mut block: Vec<f32> = Vec::with_capacity(8192);
        // Swallow stream errors: the UI owns the alternate screen, so anything
        // written to stderr would corrupt the TUI.
        let err_fn = |_| {};
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config.into(),
                move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // On a switch, discard the stale buffered tail so the new
                    // track starts immediately (the decode thread refills after).
                    let fg = flush_cb.load(Ordering::Relaxed);
                    if fg != last_flush {
                        while consumer.pop().is_ok() {}
                        last_flush = fg;
                    }
                    let vol = f32::from_bits(vol_cb.load(Ordering::Relaxed));
                    block.clear();
                    for frame in out.chunks_mut(out_channels) {
                        let l = consumer.pop().unwrap_or(0.0) * vol;
                        let r = consumer.pop().unwrap_or(l) * vol;
                        if out_channels == 1 {
                            frame[0] = 0.5 * (l + r);
                        } else {
                            frame[0] = l;
                            frame[1] = r;
                            for s in frame.iter_mut().skip(2) {
                                *s = 0.0;
                            }
                        }
                        block.push(l);
                        block.push(r);
                    }
                    // fold this block (interleaved stereo) into the rolling scope
                    // window, then publish. `block.len()` is always even.
                    let n = block.len();
                    if n >= scope_cap {
                        scope_ring.copy_from_slice(&block[n - scope_cap..]);
                    } else if n > 0 {
                        scope_ring.rotate_left(n);
                        scope_ring[scope_cap - n..].copy_from_slice(&block);
                    }
                    scope_in.input_buffer_mut().copy_from_slice(&scope_ring);
                    scope_in.publish();
                },
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        };
        stream.play()?;

        // --- background loader thread ---------------------------------------
        // Kicks off a bounded *streaming* decode of the predicted-next track
        // ahead of time and hands the live handle to the decode thread, so an
        // Open at a track boundary is instant (no decode latency → the output
        // ring never underruns → gapless). This used to fully decode the whole
        // track into one Vec<f32> (gigabytes for a long track); now it reuses
        // the same bounded `stream_decode` a manual Open uses — since nothing
        // advances its `play_head` yet, the stream's own throttle caps it at
        // WINDOW_AHEAD_SECS and idles there.
        let (load_tx, load_rx) = unbounded::<PathBuf>();
        let (loaded_tx, loaded_rx) = unbounded::<(PathBuf, Arc<StreamBuf>, Arc<WaveShared>)>();
        thread::spawn(move || {
            while let Ok(mut path) = load_rx.recv() {
                // Collapse to the most recent request; older predictions are stale.
                while let Ok(p) = load_rx.try_recv() {
                    path = p;
                }
                let sb = StreamBuf::empty(0);
                let wave = WaveShared::new();
                // Hand off the live handle immediately — don't wait for any
                // decoding to happen; the decode thread adopts it in place.
                // The decode thread owns cancellation from here on: it cancels
                // superseded handles as it drains them. Cancelling here (as
                // this thread used to on the next request) could kill a stream
                // the decode thread had already adopted as the CURRENT track.
                let _ = loaded_tx.send((path.clone(), sb.clone(), wave.clone()));
                thread::spawn(move || stream_decode(&path, device_sr, sb, wave, 0));
            }
        });

        // --- decode thread --------------------------------------------------
        // The decode thread advances into a preloaded track on its own at a
        // sample-accurate boundary (no UI round-trip) and announces the switch
        // on `advance_tx`; the UI drains `advance_rx` to update now-playing.
        let (advance_tx, advance_rx) = unbounded::<PathBuf>();
        let (cmd_tx, cmd_rx) = bounded::<TransportCmd>(64);
        let current_bins: Arc<Mutex<Option<Arc<WaveShared>>>> = Arc::new(Mutex::new(None));
        let buffering = Arc::new(AtomicBool::new(false));
        {
            let playing = playing.clone();
            let volume = volume.clone();
            let pos_frames = pos_frames.clone();
            let dur_frames = dur_frames.clone();
            let bins_slot = current_bins.clone();
            let buffering = buffering.clone();
            thread::spawn(move || {
                decode_loop(
                    cmd_rx, loaded_rx, advance_tx, bins_slot, buffering, producer, device_sr,
                    playing, volume, pos_frames, dur_frames, flush_gen,
                );
            });
        }

        Ok(Self {
            cmd_tx,
            load_tx,
            advance_rx,
            current_bins,
            buffering,
            scope_out,
            device_sr,
            pos_frames,
            dur_frames,
            playing,
            volume,
            _stream: stream,
        })
    }

    /// True while a track is playing but no audio has decoded yet — drives the
    /// loading spinner. Cleared the moment the first frames are ready.
    pub fn is_buffering(&self) -> bool {
        self.buffering.load(Ordering::Relaxed)
    }

    /// Snapshot of the current streaming track's progressively-filled waveform
    /// peaks — `None` until at least one peak has been decoded (so the UI doesn't
    /// flash empty bins) and for fully-decoded / gapless tracks.
    pub fn stream_bins(&self) -> Option<Vec<(f32, f32)>> {
        let g = self.current_bins.lock().unwrap();
        g.as_ref()
            .filter(|w| w.active.load(Ordering::Relaxed))
            .map(|w| w.bins.lock().unwrap().clone())
    }

    /// Whether the current track is being streamed (has a live waveform coming),
    /// even if no peaks have landed yet — so the UI waits instead of recomputing.
    pub fn has_stream(&self) -> bool {
        self.current_bins.lock().unwrap().is_some()
    }

    pub fn send(&self, cmd: TransportCmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Ask the loader thread to decode `path` ahead of time. A later
    /// `Open(path)` for the same file then switches instantly (gapless).
    pub fn preload(&self, path: PathBuf) {
        let _ = self.load_tx.send(path);
    }

    /// Non-blocking: returns the path of a track the decode thread just spliced
    /// into at a gapless boundary, if any. The UI calls this each tick to keep
    /// `now_playing` (and the next preload) in step with auto-advance.
    pub fn poll_advance(&self) -> Option<PathBuf> {
        self.advance_rx.try_recv().ok()
    }

    /// Latest interleaved-stereo scope window (`SCOPE_LEN * 2`, most recent last).
    pub fn scope(&mut self) -> &[f32] {
        self.scope_out.read()
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn position_secs(&self) -> f64 {
        self.pos_frames.load(Ordering::Relaxed) as f64 / self.device_sr as f64
    }

    pub fn duration_secs(&self) -> f64 {
        self.dur_frames.load(Ordering::Relaxed) as f64 / self.device_sr as f64
    }

    pub fn sample_rate(&self) -> u32 {
        self.device_sr
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
}

/// Seek the current track to absolute device frame `target`. If it's already
/// decoded, just move the cursor; otherwise restart the background decoder seeked
/// to that point (the growing waveform keeps accumulating across the restart).
#[allow(clippy::too_many_arguments)]
fn seek_to(
    audio: &mut Option<Arc<StreamBuf>>,
    pos_abs: &mut u64,
    current_path: &Option<PathBuf>,
    wave: &Option<Arc<WaveShared>>,
    device_sr: u32,
    target: u64,
    pos_frames: &Arc<AtomicU64>,
) {
    let Some(sb) = audio.as_ref() else { return };
    let total = sb.total.load(Ordering::Relaxed);
    let target = if total > 0 {
        target.min(total.saturating_sub(1))
    } else {
        target
    };
    let base = sb.base.load(Ordering::Relaxed);
    let ready_abs = sb.ready_abs.load(Ordering::Acquire);
    if target >= base && target <= ready_abs {
        // Inside the resident window — just move the playhead.
        *pos_abs = target;
        sb.play_head.store(target, Ordering::Relaxed);
        pos_frames.store(target, Ordering::Relaxed);
        return;
    }
    // Outside the window: restart the decoder seeked to `target`. Only live
    // streams reach here (a fully-decoded buffer covers its whole range above).
    if let (Some(path), Some(w)) = (current_path, wave) {
        sb.cancel.store(true, Ordering::Relaxed);
        let nsb = StreamBuf::empty(target);
        nsb.total.store(total, Ordering::Relaxed);
        *pos_abs = target;
        pos_frames.store(target, Ordering::Relaxed);
        let (p, w2, nsb2) = (path.clone(), w.clone(), nsb.clone());
        *audio = Some(nsb);
        thread::spawn(move || stream_decode(&p, device_sr, nsb2, w2, target));
    } else {
        *pos_abs = target.clamp(base, ready_abs);
        sb.play_head.store(*pos_abs, Ordering::Relaxed);
        pos_frames.store(*pos_abs, Ordering::Relaxed);
    }
}

/// Decode-thread main loop: owns the current track's growing `StreamBuf` and
/// feeds the ring from whatever has decoded so far.
#[allow(clippy::too_many_arguments)]
fn decode_loop(
    cmd_rx: Receiver<TransportCmd>,
    loaded_rx: Receiver<(PathBuf, Arc<StreamBuf>, Arc<WaveShared>)>,
    advance_tx: Sender<PathBuf>,
    bins_slot: Arc<Mutex<Option<Arc<WaveShared>>>>,
    buffering: Arc<AtomicBool>,
    mut producer: rtrb::Producer<f32>,
    device_sr: u32,
    playing: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    pos_frames: Arc<AtomicU64>,
    dur_frames: Arc<AtomicU64>,
    flush_gen: Arc<AtomicU64>,
) {
    let mut audio: Option<Arc<StreamBuf>> = None;
    let mut pos_abs: u64 = 0; // absolute device frame the consumer has reached
    // The progressively-filled waveform + source path of the current stream, kept
    // for waveform publishing and seek-into-undecoded restarts.
    let mut wave: Option<Arc<WaveShared>> = None;
    let mut current_path: Option<PathBuf> = None;
    // Track being streamed ahead of time by the loader thread — a live,
    // bounded StreamBuf + WaveShared adopted in place for an instant, gapless
    // switch when its path is Open-ed (or auto-advanced into).
    let mut preloaded: Option<(PathBuf, Arc<StreamBuf>, Arc<WaveShared>)> = None;
    // Counts active-playback ticks since the last `trim_behind` sweep of a
    // finished buffer (see below) — roughly once per second.
    let mut trim_countdown: u32 = 0;

    loop {
        // Pick up any inflight preload handle, keeping only the most recent and
        // stopping the decoder of each one it supersedes (this loop is the sole
        // owner of preload cancellation — see the loader thread above).
        while let Ok(item) = loaded_rx.try_recv() {
            if let Some((_, old, _)) = preloaded.replace(item) {
                old.cancel.store(true, Ordering::Relaxed);
            }
        }
        // Drain all pending commands into a batch. Coalescing here is what keeps
        // rapid song-switching responsive: only the LAST Open actually spawns a
        // decoder, so a burst of switches never backs up the command channel.
        let mut cmds = Vec::new();
        while let Ok(cmd) = cmd_rx.try_recv() {
            cmds.push(cmd);
        }
        let got_cmd = !cmds.is_empty();
        let last_open = cmds
            .iter()
            .rposition(|c| matches!(c, TransportCmd::Open(_)));
        // Coalesce absolute seeks: rapid waveform clicks would otherwise each
        // spawn a fresh decoder (and pile up memory) — only the last one matters.
        let last_seek = cmds
            .iter()
            .rposition(|c| matches!(c, TransportCmd::SeekTo(_)));
        for (i, cmd) in cmds.into_iter().enumerate() {
            match cmd {
                TransportCmd::Open(path) => {
                    // Skip every superseded Open without decoding it.
                    if Some(i) != last_open {
                        continue;
                    }
                    // Cancel the previous stream's background decoder so a long
                    // podcast doesn't keep decoding after we've moved on.
                    if let Some(old) = &audio {
                        old.cancel.store(true, Ordering::Relaxed);
                    }
                    pos_abs = 0;
                    current_path = Some(path.clone());
                    pos_frames.store(0, Ordering::Relaxed);
                    dur_frames.store(0, Ordering::Relaxed);
                    playing.store(false, Ordering::Relaxed);
                    // Flush the previous track's buffered tail so the switch is
                    // immediate (no stale audio plays out first).
                    flush_gen.fetch_add(1, Ordering::Relaxed);
                    // Adopt the preloaded stream only when it's for this exact
                    // path. A mismatched leftover is NOT stale garbage — it is
                    // usually the prefetch for the track that will follow this
                    // one (`play_path` sends `Open`, then predicts and preloads
                    // the next), so keep it alive for the upcoming gapless
                    // boundary instead of killing it.
                    let adopted = match &preloaded {
                        Some((p, _, _)) if *p == path => preloaded.take(),
                        _ => None,
                    };
                    match adopted {
                        // Prefetched (gapless): adopt the already-running stream
                        // handle directly — its throttle has kept up to
                        // WINDOW_AHEAD_SECS decoded and idling, plenty of runway,
                        // and its waveform is already filling in.
                        Some((_, sb, w)) => {
                            dur_frames.store(sb.total.load(Ordering::Relaxed), Ordering::Relaxed);
                            audio = Some(sb);
                            wave = Some(w.clone());
                            *bins_slot.lock().unwrap() = Some(w);
                            playing.store(true, Ordering::Relaxed);
                        }
                        None => {
                            // Stream it: play as it decodes, never waiting for the
                            // whole file. The waveform fills in from the same decode.
                            let sb = StreamBuf::empty(0);
                            let w = WaveShared::new();
                            audio = Some(sb.clone());
                            wave = Some(w.clone());
                            *bins_slot.lock().unwrap() = Some(w.clone());
                            playing.store(true, Ordering::Relaxed);
                            thread::spawn(move || stream_decode(&path, device_sr, sb, w, 0));
                        }
                    }
                }
                TransportCmd::Toggle => {
                    let p = !playing.load(Ordering::Relaxed);
                    playing.store(p, Ordering::Relaxed);
                }
                TransportCmd::Pause => playing.store(false, Ordering::Relaxed),
                TransportCmd::Play => playing.store(true, Ordering::Relaxed),
                TransportCmd::SeekRel(secs) => {
                    if audio.is_some() {
                        let target = (pos_abs as f64 + secs * device_sr as f64).max(0.0) as u64;
                        seek_to(
                            &mut audio,
                            &mut pos_abs,
                            &current_path,
                            &wave,
                            device_sr,
                            target,
                            &pos_frames,
                        );
                    }
                }
                TransportCmd::SeekTo(secs) => {
                    if Some(i) != last_seek {
                        continue; // superseded by a later click in this batch
                    }
                    if audio.is_some() {
                        let target = (secs * device_sr as f64).max(0.0) as u64;
                        seek_to(
                            &mut audio,
                            &mut pos_abs,
                            &current_path,
                            &wave,
                            device_sr,
                            target,
                            &pos_frames,
                        );
                    }
                }
                TransportCmd::VolRel(d) => {
                    let v = (f32::from_bits(volume.load(Ordering::Relaxed)) + d).clamp(0.0, 1.0);
                    volume.store(v.to_bits(), Ordering::Relaxed);
                }
                TransportCmd::SetVol(v) => {
                    volume.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
                }
            }
        }

        let active = playing.load(Ordering::Relaxed);
        // Buffering = trying to play but nothing decoded yet (drives the spinner).
        buffering.store(
            active
                && audio.as_ref().is_some_and(|sb| {
                    sb.ready_abs.load(Ordering::Acquire) <= sb.base.load(Ordering::Relaxed)
                }),
            Ordering::Relaxed,
        );
        if active && audio.is_some() {
            let sb = audio.as_ref().unwrap().clone();
            // Duration: the container total once known, else the decode edge.
            let total_est = sb.total.load(Ordering::Relaxed);
            dur_frames.store(
                if total_est > 0 {
                    total_est
                } else {
                    sb.ready_abs.load(Ordering::Relaxed)
                },
                Ordering::Relaxed,
            );
            let mut pushed = false;
            let (ready_abs, empty);
            {
                // Lock once and push a batch (decode thread, not the realtime cpal
                // callback, so a short lock is fine). Read `base` under the lock so
                // it can't shift under a concurrent trim.
                let samples = sb.samples.lock().unwrap();
                let base = sb.base.load(Ordering::Relaxed);
                let len = samples.len() / 2;
                ready_abs = base + len as u64;
                empty = len == 0;
                // Clamp into the window (a seek may have landed just inside it).
                if pos_abs < base {
                    pos_abs = base;
                }
                let mut local = (pos_abs - base) as usize;
                while local < len {
                    if producer.slots() < 2 {
                        break;
                    }
                    let l = samples[local * 2];
                    let r = samples[local * 2 + 1];
                    if producer.push(l).is_err() {
                        break;
                    }
                    let _ = producer.push(r);
                    local += 1;
                    pushed = true;
                }
                pos_abs = base + local as u64;
            }
            // Publish the playhead so the decoder can throttle + trim around it.
            sb.play_head.store(pos_abs, Ordering::Relaxed);
            if pushed {
                pos_frames.store(pos_abs, Ordering::Relaxed);
            }
            // Once decoding has finished, no more `push` calls will trim behind
            // the playhead for us (the streaming path trims on every push; the
            // full-decode fallback pushes the whole file once with no trim at
            // all). Sweep it ourselves roughly once a second so both cases stay
            // bounded to the behind-window instead of holding the whole track —
            // or the fallback's initial full decode — resident for the rest of
            // playback.
            if sb.done.load(Ordering::Acquire) {
                trim_countdown += 1;
                if trim_countdown >= 333 {
                    trim_countdown = 0;
                    sb.trim_behind(device_sr as u64 * WINDOW_BEHIND_SECS);
                }
            } else {
                trim_countdown = 0;
            }
            // End only when the playhead reaches the decode edge AND decoding is
            // finished. If we've just caught up to the live edge, hold and let the
            // decoder get ahead. A buffer that finished with ZERO frames failed to
            // load — do NOT auto-advance (that looks like a false `p`/`n` skip).
            if pos_abs >= ready_abs && sb.done.load(Ordering::Acquire) {
                if empty {
                    playing.store(false, Ordering::Relaxed);
                } else {
                    match preloaded.take() {
                        Some((path, sb, w)) => {
                            // Adopt the live stream handle in place, same as the
                            // Open splice above — no full-decode wait.
                            current_path = Some(path.clone());
                            wave = Some(w.clone());
                            *bins_slot.lock().unwrap() = Some(w);
                            dur_frames.store(sb.total.load(Ordering::Relaxed), Ordering::Relaxed);
                            audio = Some(sb);
                            pos_abs = 0;
                            pos_frames.store(0, Ordering::Relaxed);
                            let _ = advance_tx.send(path);
                        }
                        None => playing.store(false, Ordering::Relaxed),
                    }
                }
            }
            thread::sleep(Duration::from_millis(3));
        } else if !got_cmd {
            thread::sleep(Duration::from_millis(8));
        }
    }
}

/// Decode `path`, but never let a panic in a codec/resampler escape the
/// decode/loader thread. A panic there would unwind the thread and kill it; from
/// then on every `TransportCmd` is dropped silently (the `cmd_tx` receiver is
/// gone) and playback is dead until the app restarts. Catching it and returning a
/// normal `Err` leaves a clean stopped state, so the *next* track still plays.
fn load_track_safe(path: &Path, device_sr: u32, cancel: &AtomicBool) -> Result<DecodedAudio> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        load_track(path, device_sr, cancel)
    }))
    .unwrap_or_else(|_| Err(anyhow!("decoder panicked on {}", path.display())))
}

/// Decode a file to interleaved stereo at the device sample rate.
fn load_track(path: &Path, device_sr: u32, cancel: &AtomicBool) -> Result<DecodedAudio> {
    let (dec, _) = decode_any(path, None, Some(cancel))?;
    Ok(to_device_rate(dec, device_sr))
}

/// Resample a native-rate decode to the output device rate (no-op if it already
/// matches).
fn to_device_rate(dec: DecodedAudio, device_sr: u32) -> DecodedAudio {
    if dec.sample_rate == device_sr {
        dec
    } else {
        DecodedAudio {
            sample_rate: device_sr,
            samples: resample_stereo(&dec.samples, dec.sample_rate, device_sr),
        }
    }
}

/// Decode `path` to interleaved stereo at its native rate, transparently
/// handling both disk files and in-archive virtual paths (bytes from the
/// in-memory store). symphonia first, ffmpeg as a fallback. When `cancel` is
/// set, the decode aborts (with an error) as soon as the flag flips — the
/// caller has moved on and the result would be thrown away.
fn decode_any(
    path: &Path,
    limit_secs: Option<f64>,
    cancel: Option<&AtomicBool>,
) -> Result<(DecodedAudio, Option<u64>)> {
    // ffmpeg has no prefix mode, so its fallback always returns the full track;
    // its total frame count is therefore exact.
    let full = |d: DecodedAudio| {
        let n = d.frames() as u64;
        (d, Some(n))
    };
    // Don't even launch ffmpeg for a decode that's already been abandoned.
    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    if let Some((arc, inner)) = crate::archive::split_virtual(path) {
        let bytes = crate::archive::read(&arc, &inner)
            .ok_or_else(|| anyhow!("archive entry unreadable: {inner}"))?;
        let ext = Path::new(&inner).extension().and_then(|e| e.to_str());
        let src = Box::new(MemSource::new(bytes.clone()));
        decode_symphonia_source(src, ext, limit_secs, cancel).or_else(|e| {
            if cancelled() {
                Err(e)
            } else {
                decode_ffmpeg_bytes(&bytes).map(full)
            }
        })
    } else {
        decode_symphonia(path, limit_secs, cancel).or_else(|e| {
            if cancelled() {
                Err(e)
            } else {
                decode_ffmpeg(path).map(full)
            }
        })
    }
}

fn decode_symphonia(
    path: &Path,
    limit_secs: Option<f64>,
    cancel: Option<&AtomicBool>,
) -> Result<(DecodedAudio, Option<u64>)> {
    use symphonia::core::io::MediaSourceStream;
    let file = File::open(path)?;
    let src = MediaSourceStream::new(Box::new(file), Default::default());
    let ext = path.extension().and_then(|e| e.to_str());
    decode_symphonia_stream(src, ext, limit_secs, cancel)
}

/// Decode from any symphonia `MediaSource` (disk file or in-memory bytes).
fn decode_symphonia_source(
    src: Box<dyn symphonia::core::io::MediaSource>,
    ext: Option<&str>,
    limit_secs: Option<f64>,
    cancel: Option<&AtomicBool>,
) -> Result<(DecodedAudio, Option<u64>)> {
    use symphonia::core::io::MediaSourceStream;
    let mss = MediaSourceStream::new(src, Default::default());
    decode_symphonia_stream(mss, ext, limit_secs, cancel)
}

/// Decode a symphonia stream to interleaved stereo at its native rate. When
/// `limit_secs` is set, stop after roughly that many seconds of audio (used to
/// decode just a fast prefix for instant playback). Also returns the track's
/// total native frame count when the container advertises it, so callers can
/// report the real duration even from a truncated prefix.
fn decode_symphonia_stream(
    mss: symphonia::core::io::MediaSourceStream,
    ext: Option<&str>,
    limit_secs: Option<f64>,
    cancel: Option<&AtomicBool>,
) -> Result<(DecodedAudio, Option<u64>)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let mut hint = Hint::new();
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }
    // enable_gapless makes the demuxer read the MP3 LAME/Xing tag (and the
    // equivalent for other formats) and mark each packet's encoder delay /
    // padding as trim_start / trim_end, which the decode loop below applies.
    let fmt_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &fmt_opts,
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default track"))?
        .clone();
    let track_id = track.id;
    let sr = track.codec_params.sample_rate.unwrap_or(44100);
    let n_frames = track.codec_params.n_frames;
    let limit_frames = limit_secs.map(|s| (s * sr as f64) as usize);
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut out: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        // Abandoned mid-decode (the track was superseded): bail out instead of
        // finishing a whole-file decode nobody will use.
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(anyhow!("decode cancelled"));
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        // Gapless trim: frames of encoder delay (front) / padding (back) to drop
        // from this packet. Nonzero only on the first/last packets; 0 otherwise.
        let trim_start = packet.trim_start as usize;
        let trim_end = packet.trim_end as usize;
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let ch = decoded.spec().channels.count().max(1);
                if sbuf.is_none() {
                    let spec = *decoded.spec();
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sb = sbuf.as_mut().unwrap();
                sb.copy_interleaved_ref(decoded);
                let samples = sb.samples();
                let frames = samples.len() / ch;
                // Fold to interleaved stereo over the kept frame range only.
                let start = trim_start.min(frames);
                let end = frames.saturating_sub(trim_end).max(start);
                for f in start..end {
                    let base = f * ch;
                    let l = samples[base];
                    let r = if ch >= 2 { samples[base + 1] } else { l };
                    out.push(l);
                    out.push(r);
                }
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(_) => break,
        }
        // Prefix mode: stop once we have enough audio for an instant start.
        if let Some(lim) = limit_frames
            && out.len() / 2 >= lim
        {
            break;
        }
    }
    if out.is_empty() {
        return Err(anyhow!("symphonia decoded no audio"));
    }
    Ok((
        DecodedAudio {
            sample_rate: sr,
            samples: out,
        },
        n_frames,
    ))
}

fn decode_ffmpeg(path: &Path) -> Result<DecodedAudio> {
    let sr = 48000u32;
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "f32le", "-ac", "2", "-ar", "48000", "-"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut samples = Vec::with_capacity(out.stdout.len() / 4);
    for c in out.stdout.chunks_exact(4) {
        samples.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
    }
    if samples.is_empty() {
        return Err(anyhow!("ffmpeg produced no audio"));
    }
    Ok(DecodedAudio {
        sample_rate: sr,
        samples,
    })
}

/// ffmpeg fallback for in-archive entries: pipe the in-memory bytes through
/// stdin (`-i pipe:0`) so nothing is written to disk. stdin is written on a
/// separate thread to avoid a pipe-buffer deadlock against stdout.
fn decode_ffmpeg_bytes(bytes: &[u8]) -> Result<DecodedAudio> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let sr = 48000u32;
    let mut child = Command::new("ffmpeg")
        .args([
            "-v", "error", "-i", "pipe:0", "-f", "f32le", "-ac", "2", "-ar", "48000", "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("no ffmpeg stdin"))?;
    let data = bytes.to_vec();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(&data);
        // drop(stdin) closes the pipe so ffmpeg sees EOF
    });
    let out = child.wait_with_output()?;
    let _ = writer.join();
    if !out.status.success() {
        return Err(anyhow!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut samples = Vec::with_capacity(out.stdout.len() / 4);
    for c in out.stdout.chunks_exact(4) {
        samples.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
    }
    if samples.is_empty() {
        return Err(anyhow!("ffmpeg produced no audio"));
    }
    Ok(DecodedAudio {
        sample_rate: sr,
        samples,
    })
}

/// Cheap linear-interpolation resampler for interleaved stereo.
/// Resample interleaved-stereo f32 from `from` Hz to `to` Hz. Uses a windowed-
/// sinc (rubato) for clean band-limited conversion; falls back to linear
/// interpolation if the resampler can't be constructed. Runs offline on the
/// load thread, so the extra cost never touches the real-time path.
fn resample_stereo(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    resample_sinc(input, from, to).unwrap_or_else(|| resample_linear(input, from, to))
}

/// Windowed-sinc resample of interleaved stereo via rubato. `None` if the
/// resampler fails to construct or process (caller falls back to linear).
fn resample_sinc(input: &[f32], from: u32, to: u32) -> Option<Vec<f32>> {
    use rubato::audioadapter_buffers::direct::InterleavedSlice;
    use rubato::{
        Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
        WindowFunction, calculate_cutoff,
    };

    const CHANNELS: usize = 2;
    const SINC_LEN: usize = 256;
    let in_frames = input.len() / CHANNELS;
    let ratio = to as f64 / from as f64;

    let params = SincInterpolationParameters {
        sinc_len: SINC_LEN,
        f_cutoff: calculate_cutoff(SINC_LEN, WindowFunction::Blackman2),
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: 256,
        window: WindowFunction::Blackman2,
    };
    let mut resampler =
        Async::<f32>::new_sinc(ratio, 1.1, &params, 1024, CHANNELS, FixedAsync::Input).ok()?;

    // Generous output capacity: resampled length plus a chunk of headroom for
    // the resampler's delay/flush. Trimmed to the actual count afterwards.
    let out_cap = (in_frames as f64 * ratio).ceil() as usize + 1024;
    let mut out = vec![0f32; out_cap * CHANNELS];
    let in_adapter = InterleavedSlice::new(input, CHANNELS, in_frames).ok()?;
    let mut out_adapter = InterleavedSlice::new_mut(&mut out, CHANNELS, out_cap).ok()?;
    let (_, out_frames) = resampler
        .process_all_into_buffer(&in_adapter, &mut out_adapter, in_frames, None)
        .ok()?;
    out.truncate(out_frames * CHANNELS);
    Some(out)
}

/// Linear-interpolation resample (fallback). Cheap, slightly soft/aliased.
fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    let frames = input.len() / 2;
    let ratio = to as f64 / from as f64;
    let out_frames = (frames as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let i0 = idx.min(frames - 1);
        let i1 = (idx + 1).min(frames - 1);
        for ch in 0..2 {
            let a = input[i0 * 2 + ch];
            let b = input[i1 * 2 + ch];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

/// Cheap waveform *without decoding*: demux the file and use each compressed
/// packet's byte size as an energy/complexity proxy, binned by time. ~10-50×
/// faster than a full decode (no codec runs), so it appears near-instantly even
/// for long files. Returns symmetric `(-v, v)` bins.
///
/// `None` when the file is (near-)constant-bitrate — packet sizes are all equal
/// then, so the envelope would be a flat line and the caller should fall back to
/// a real decode-scan.
pub fn waveform_envelope(path: &Path, n_bins: usize) -> Option<Vec<(f32, f32)>> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Same source resolution as the streaming decoder: disk file or archive bytes.
    let mut hint = Hint::new();
    let mss = if let Some((arc, inner)) = crate::archive::split_virtual(path) {
        let bytes = crate::archive::read(&arc, &inner)?;
        if let Some(ext) = Path::new(&inner).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        MediaSourceStream::new(Box::new(MemSource::new(bytes)), Default::default())
    } else {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let file = File::open(path).ok()?;
        MediaSourceStream::new(Box::new(file), Default::default())
    };
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track_id = format.default_track()?.id;

    // Demux only — collect each packet's compressed byte size (no decode).
    let mut sizes: Vec<f32> = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() == track_id {
            sizes.push(packet.data.len() as f32);
        }
    }
    if sizes.len() < n_bins.min(8) {
        return None;
    }
    // Constant bitrate → all packets ~equal → useless flat envelope.
    let (mut lo, mut hi) = (f32::MAX, 0.0f32);
    for &s in &sizes {
        lo = lo.min(s);
        hi = hi.max(s);
    }
    if hi <= 0.0 || (hi - lo) / hi < 0.05 {
        return None;
    }

    // Resample the per-packet sizes to `n_bins` (packets are ~uniform in time),
    // then normalize. sqrt tames the wide dynamic range for a nicer shape.
    let m = sizes.len();
    let mut bins = Vec::with_capacity(n_bins);
    for b in 0..n_bins {
        let start = b * m / n_bins;
        let end = (((b + 1) * m / n_bins).max(start + 1)).min(m);
        let mut peak = 0.0f32;
        for &s in &sizes[start..end] {
            peak = peak.max(s);
        }
        let v = (peak / hi).clamp(0.0, 1.0).sqrt();
        bins.push((-v, v));
    }
    Some(bins)
}

/// Compute `n_bins` (min,max) mono peak bins for a whole file (static waveform).
pub fn waveform_bins(path: &Path, n_bins: usize) -> Result<Vec<(f32, f32)>> {
    // Streaming path: symphonia, packet-by-packet, never holding the whole
    // track. Only formats that need the ffmpeg fallback (which has no
    // packet-streaming API of its own) fall through to the old full-decode fold.
    waveform_bins_stream(path, n_bins).or_else(|_| {
        let (dec, _) = decode_any(path, None, None)?;
        Ok(waveform_bins_fold(&dec.samples, dec.frames(), n_bins))
    })
}

/// Fold a fully-decoded interleaved-stereo buffer into `n_bins` (min,max) mono
/// peak bins. Used only by the ffmpeg fallback, which has no streaming API.
fn waveform_bins_fold(samples: &[f32], frames: usize, n_bins: usize) -> Vec<(f32, f32)> {
    if frames == 0 {
        return vec![(0.0, 0.0); n_bins];
    }
    let mut bins = Vec::with_capacity(n_bins);
    let per = (frames as f64 / n_bins as f64).max(1.0);
    for b in 0..n_bins {
        let start = (b as f64 * per) as usize;
        let end = (((b + 1) as f64 * per) as usize).min(frames);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for f in start..end {
            let m = 0.5 * (samples[f * 2] + samples[f * 2 + 1]);
            lo = lo.min(m);
            hi = hi.max(m);
        }
        bins.push((lo, hi));
    }
    bins
}

/// Streaming version of `waveform_bins`: demux + decode packet-by-packet and
/// fold peaks directly, never accumulating raw samples. When the container
/// advertises `n_frames` (almost always), bins are pre-sized and each frame
/// folds straight into its absolute bucket. Otherwise (rare — unknown length),
/// one (min,max) pair is kept per packet — thousands of entries, still just
/// KBs — and that small pair-list is resampled to `n_bins` at the end.
fn waveform_bins_stream(path: &Path, n_bins: usize) -> Result<Vec<(f32, f32)>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    if n_bins == 0 {
        return Ok(Vec::new());
    }

    // Same source resolution as the streaming decoder: disk file or archive bytes.
    let mut hint = Hint::new();
    let mss = if let Some((arc, inner)) = crate::archive::split_virtual(path) {
        let bytes = crate::archive::read(&arc, &inner)
            .ok_or_else(|| anyhow!("archive entry unreadable: {inner}"))?;
        if let Some(ext) = Path::new(&inner).extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        MediaSourceStream::new(Box::new(MemSource::new(bytes)), Default::default())
    } else {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let file = File::open(path)?;
        MediaSourceStream::new(Box::new(file), Default::default())
    };
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no default track"))?
        .clone();
    let track_id = track.id;
    let n_frames = track.codec_params.n_frames.filter(|&n| n > 0);
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    // Known length: pre-size bins and fold each frame into its bucket directly.
    let mut bins: Vec<(f32, f32)> = match n_frames {
        Some(_) => vec![(0.0, 0.0); n_bins],
        None => Vec::new(),
    };
    // Unknown length: one (min,max) pair per packet, resampled to n_bins at the end.
    let mut packet_pairs: Vec<(f32, f32)> = Vec::new();

    let mut frame_pos: u64 = 0;
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let ch = decoded.spec().channels.count().max(1);
                if sbuf.is_none() {
                    let spec = *decoded.spec();
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sb = sbuf.as_mut().unwrap();
                sb.copy_interleaved_ref(decoded);
                let samples = sb.samples();
                let frames = samples.len() / ch;
                if let Some(total) = n_frames {
                    for f in 0..frames {
                        let base = f * ch;
                        let l = samples[base];
                        let r = if ch >= 2 { samples[base + 1] } else { l };
                        let m = 0.5 * (l + r);
                        let bi = (((frame_pos + f as u64) * n_bins as u64) / total)
                            .min(n_bins as u64 - 1) as usize;
                        let b = &mut bins[bi];
                        if m < b.0 {
                            b.0 = m;
                        }
                        if m > b.1 {
                            b.1 = m;
                        }
                    }
                } else if frames > 0 {
                    let (mut lo, mut hi) = (0.0f32, 0.0f32);
                    for f in 0..frames {
                        let base = f * ch;
                        let l = samples[base];
                        let r = if ch >= 2 { samples[base + 1] } else { l };
                        let m = 0.5 * (l + r);
                        lo = lo.min(m);
                        hi = hi.max(m);
                    }
                    packet_pairs.push((lo, hi));
                }
                frame_pos += frames as u64;
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(_) => break,
        }
    }

    if frame_pos == 0 {
        return Ok(vec![(0.0, 0.0); n_bins]);
    }
    if n_frames.is_some() {
        return Ok(bins);
    }
    // Resample the per-packet pairs down to n_bins (packets are ~uniform in time).
    let m = packet_pairs.len().max(1);
    let mut out = Vec::with_capacity(n_bins);
    for b in 0..n_bins {
        let start = (b * m / n_bins).min(packet_pairs.len());
        let end = (((b + 1) * m / n_bins).max(start + 1)).min(packet_pairs.len());
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for &(l, h) in &packet_pairs[start..end] {
            lo = lo.min(l);
            hi = hi.max(h);
        }
        out.push((lo, hi));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sinc_resample_length_and_finite() {
        // 0.5s of 440Hz stereo at 44.1k -> 48k.
        let from = 44_100u32;
        let to = 48_000u32;
        let frames = from as usize / 2;
        let mut input = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = (n as f32 * 440.0 * std::f32::consts::TAU / from as f32).sin() * 0.5;
            input.push(s);
            input.push(s);
        }
        let out = resample_stereo(&input, from, to);
        let out_frames = out.len() / 2;
        let expected = (frames as f64 * to as f64 / from as f64) as usize;
        // Within a small tolerance of the ideal ratio.
        let diff = out_frames.abs_diff(expected);
        assert!(diff < 256, "out_frames={out_frames} expected≈{expected}");
        assert!(out.iter().all(|s| s.is_finite()), "non-finite sample");
        // No silly clipping blowups from the sinc kernel.
        assert!(out.iter().all(|s| s.abs() <= 1.5), "sample out of range");
    }

    #[test]
    fn resample_noop_when_rates_match() {
        let input = vec![0.1f32, -0.1, 0.2, -0.2];
        assert_eq!(resample_stereo(&input, 48_000, 48_000), input);
    }
}
