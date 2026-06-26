//! Audio spine: decode (symphonia, ffmpeg fallback) -> rtrb ring -> cpal output,
//! with a triple_buffer tap in the cpal callback feeding the live oscilloscope.
//!
//! The cpal callback is the single source of truth for "what is audible now": it
//! pulls PCM from the ring and copies the same block into the scope buffer, so the
//! visualization is sample-synced to the DAC. No locks/allocations on that path.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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

/// Owns the cpal stream (kept alive) and the handles the UI reads each frame.
pub struct AudioEngine {
    cmd_tx: Sender<TransportCmd>,
    /// Requests to the background loader thread to decode a track ahead of time
    /// for gapless transitions.
    load_tx: Sender<PathBuf>,
    /// Path of each track the decode thread spliced into at a gapless boundary,
    /// so the UI can sync `now_playing` without re-issuing `Open`.
    advance_rx: Receiver<PathBuf>,
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

        // --- cpal output callback (real-time thread) ------------------------
        let vol_cb = volume.clone();
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
        // Decodes the predicted-next track ahead of time and hands the finished
        // buffer to the decode thread, so an Open at a track boundary is instant
        // (no decode latency → the output ring never underruns → gapless).
        let (load_tx, load_rx) = unbounded::<PathBuf>();
        let (loaded_tx, loaded_rx) = unbounded::<(PathBuf, DecodedAudio)>();
        thread::spawn(move || {
            while let Ok(mut path) = load_rx.recv() {
                // Collapse to the most recent request; older predictions are stale.
                while let Ok(p) = load_rx.try_recv() {
                    path = p;
                }
                if let Ok(dec) = load_track(&path, device_sr) {
                    let _ = loaded_tx.send((path, dec));
                }
            }
        });

        // --- decode thread --------------------------------------------------
        // The decode thread advances into a preloaded track on its own at a
        // sample-accurate boundary (no UI round-trip) and announces the switch
        // on `advance_tx`; the UI drains `advance_rx` to update now-playing.
        let (advance_tx, advance_rx) = unbounded::<PathBuf>();
        let (cmd_tx, cmd_rx) = bounded::<TransportCmd>(64);
        {
            let playing = playing.clone();
            let volume = volume.clone();
            let pos_frames = pos_frames.clone();
            let dur_frames = dur_frames.clone();
            thread::spawn(move || {
                decode_loop(
                    cmd_rx, loaded_rx, advance_tx, producer, device_sr, playing, volume,
                    pos_frames, dur_frames,
                );
            });
        }

        Ok(Self {
            cmd_tx,
            load_tx,
            advance_rx,
            scope_out,
            device_sr,
            pos_frames,
            dur_frames,
            playing,
            volume,
            _stream: stream,
        })
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

/// Decode-thread main loop: owns the current track in memory and feeds the ring.
fn decode_loop(
    cmd_rx: Receiver<TransportCmd>,
    loaded_rx: Receiver<(PathBuf, DecodedAudio)>,
    advance_tx: Sender<PathBuf>,
    mut producer: rtrb::Producer<f32>,
    device_sr: u32,
    playing: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    pos_frames: Arc<AtomicU64>,
    dur_frames: Arc<AtomicU64>,
) {
    let mut audio: Option<DecodedAudio> = None;
    let mut cursor: usize = 0; // frame index into the current track
    // Track decoded ahead of time by the loader thread, ready for an instant,
    // gapless switch when its path is Open-ed.
    let mut preloaded: Option<(PathBuf, DecodedAudio)> = None;

    loop {
        // Pick up any finished preload (keep only the most recent).
        while let Ok(item) = loaded_rx.try_recv() {
            preloaded = Some(item);
        }
        // Drain all pending commands first.
        let mut got_cmd = false;
        while let Ok(cmd) = cmd_rx.try_recv() {
            got_cmd = true;
            match cmd {
                TransportCmd::Open(path) => {
                    // Reset transport up-front so a failed decode leaves a clean
                    // stopped state (dur = 0) rather than stale values, which
                    // would otherwise make the UI's end-of-track detector fire
                    // every tick and storm-advance through the list.
                    //
                    // The output ring is intentionally not cleared: any tail of
                    // the previous track plays out and the new track's samples
                    // follow contiguously, which is what makes the boundary
                    // seamless when the next track was preloaded.
                    audio = None;
                    cursor = 0;
                    pos_frames.store(0, Ordering::Relaxed);
                    dur_frames.store(0, Ordering::Relaxed);
                    playing.store(false, Ordering::Relaxed);
                    // Use the preloaded buffer if it matches (no decode latency);
                    // otherwise decode synchronously now.
                    let dec = match preloaded.take() {
                        Some((p, d)) if p == path => Ok(d),
                        _ => load_track(&path, device_sr),
                    };
                    if let Ok(dec) = dec {
                        dur_frames.store(dec.frames() as u64, Ordering::Relaxed);
                        audio = Some(dec);
                        playing.store(true, Ordering::Relaxed);
                    }
                }
                TransportCmd::Toggle => {
                    let p = !playing.load(Ordering::Relaxed);
                    playing.store(p, Ordering::Relaxed);
                }
                TransportCmd::Pause => playing.store(false, Ordering::Relaxed),
                TransportCmd::Play => playing.store(true, Ordering::Relaxed),
                TransportCmd::SeekRel(secs) => {
                    if let Some(a) = &audio {
                        let delta = (secs * device_sr as f64) as i64;
                        let nc = (cursor as i64 + delta).clamp(0, a.frames() as i64);
                        cursor = nc as usize;
                        pos_frames.store(cursor as u64, Ordering::Relaxed);
                    }
                }
                TransportCmd::SeekTo(secs) => {
                    if let Some(a) = &audio {
                        let nc = (secs * device_sr as f64).clamp(0.0, a.frames() as f64);
                        cursor = nc as usize;
                        pos_frames.store(cursor as u64, Ordering::Relaxed);
                    }
                }
                TransportCmd::VolRel(d) => {
                    let v = (f32::from_bits(volume.load(Ordering::Relaxed)) + d).clamp(0.0, 1.0);
                    volume.store(v.to_bits(), Ordering::Relaxed);
                }
            }
        }

        let active = playing.load(Ordering::Relaxed);
        if active && audio.is_some() {
            let mut pushed = false;
            // Push until the ring is full or the track ends.
            let total = {
                let a = audio.as_ref().unwrap();
                let total = a.frames();
                while cursor < total {
                    if producer.slots() < 2 {
                        break;
                    }
                    let l = a.samples[cursor * 2];
                    let r = a.samples[cursor * 2 + 1];
                    if producer.push(l).is_err() {
                        break;
                    }
                    let _ = producer.push(r);
                    cursor += 1;
                    pushed = true;
                }
                total
            };
            if pushed {
                pos_frames.store(cursor as u64, Ordering::Relaxed);
            }
            if cursor >= total {
                // End of track. If the next track was preloaded, splice straight
                // into it: keep the ring fed across the seam (its ~250ms tail is
                // still draining to the DAC) so the boundary is sample-accurate
                // with no UI round-trip. Otherwise stop and let the UI's
                // end-of-track detector handle advance / end-of-list.
                match preloaded.take() {
                    Some((path, dec)) => {
                        dur_frames.store(dec.frames() as u64, Ordering::Relaxed);
                        audio = Some(dec);
                        cursor = 0;
                        pos_frames.store(0, Ordering::Relaxed);
                        let _ = advance_tx.send(path);
                    }
                    None => playing.store(false, Ordering::Relaxed),
                }
            }
            thread::sleep(Duration::from_millis(3));
        } else if !got_cmd {
            thread::sleep(Duration::from_millis(8));
        }
    }
}

/// Decode a file to interleaved stereo at the device sample rate.
fn load_track(path: &Path, device_sr: u32) -> Result<DecodedAudio> {
    let dec = decode_symphonia(path).or_else(|_| decode_ffmpeg(path))?;
    if dec.sample_rate == device_sr {
        Ok(dec)
    } else {
        let samples = resample_stereo(&dec.samples, dec.sample_rate, device_sr);
        Ok(DecodedAudio {
            sample_rate: device_sr,
            samples,
        })
    }
}

fn decode_symphonia(path: &Path) -> Result<DecodedAudio> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymErr;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
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
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut out: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
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
    }
    if out.is_empty() {
        return Err(anyhow!("symphonia decoded no audio"));
    }
    Ok(DecodedAudio {
        sample_rate: sr,
        samples: out,
    })
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

/// Cheap linear-interpolation resampler for interleaved stereo.
fn resample_stereo(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
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

/// Compute `n_bins` (min,max) mono peak bins for a whole file (static waveform).
pub fn waveform_bins(path: &Path, n_bins: usize) -> Result<Vec<(f32, f32)>> {
    let dec = decode_symphonia(path).or_else(|_| decode_ffmpeg(path))?;
    let frames = dec.frames();
    if frames == 0 {
        return Ok(vec![(0.0, 0.0); n_bins]);
    }
    let mut bins = Vec::with_capacity(n_bins);
    let per = (frames as f64 / n_bins as f64).max(1.0);
    for b in 0..n_bins {
        let start = (b as f64 * per) as usize;
        let end = (((b + 1) as f64 * per) as usize).min(frames);
        let (mut lo, mut hi) = (0.0f32, 0.0f32);
        for f in start..end {
            let m = 0.5 * (dec.samples[f * 2] + dec.samples[f * 2 + 1]);
            lo = lo.min(m);
            hi = hi.max(m);
        }
        bins.push((lo, hi));
    }
    Ok(bins)
}
