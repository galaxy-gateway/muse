# Rapid track-cycling memory blowup (held n/p)

## Symptom

Holding `n`/`p` to cycle quickly through even a small set of songs made RSS
climb by hundreds of MB and stay there long after releasing the key. Measured
with five 90 s MP3s and `n` held at ~25 presses/sec for 10 s (v0.37.0):

```
baseline ~22 MB  ->  peak 540 MB during the hold
still 370-440 MB for ~20 s after releasing
back to ~36 MB only ~35 s later
```

`vmmap --summary` mid-burst showed the tell: live `MALLOC_LARGE` was only
~11 MB, but **`MALLOC_LARGE (empty)` held ~711 MB resident** across 64 regions.
So it was never a dangling-reference leak — the program freed everything, but
macOS libmalloc keeps freed large blocks cached dirty for a long while. The
real problem was *churn*: allocating and freeing tens of MB of PCM per second.

## Causes (each press did all of this)

1. **A fresh 30 s decode window per press.** Every `Open` (and every gapless
   `preload`) spawned a `stream_decode` thread whose `StreamBuf` raced to fill
   `WINDOW_AHEAD_SECS` (~11.5 MB at 48 kHz, reached in well under the ~40 ms
   between key repeats, plus the Vec doubling-realloc chain on the way up)
   before the next press cancelled it.
2. **A gapless prefetch per press.** `begin_now_playing` called `preload_next`
   immediately, so the loader spawned a second doomed decoder per press.
3. **Raw waveform/art threads per press.** `begin_now_playing` bypassed the
   debounced browse workers and spawned one thread per call; the single-slot
   `wave_pending`/`art_pending` guards cannot dedupe across several tracks in
   flight, so cache-cold cycling spawned duplicates.
4. **Uncancellable full-decode fallback.** A track symphonia cannot stream
   fell back to a blocking whole-file decode (`load_track_safe`) that checked
   its cancel flag only before/after — rapid cycling could stack several
   ~100 MB decodes (and ffmpeg child processes) concurrently.

There were also two preload-ownership bugs (not memory, but found in the same
audit): the loader thread cancelled its previous inflight handle on every new
request even after the decode thread had **adopted it as the current track**
(a gapless-adopted track stopped decoding at the window edge and falsely
auto-advanced), and the `Open` handler cancelled a mismatched leftover preload
that is usually the legitimate prefetch for the *following* track (silently
defeating gapless).

## Fixes (v0.37.1)

- **Warmup lookahead** (`audio.rs`): a new stream throttles at
  `WARMUP_AHEAD_SECS = 3` until the playhead has advanced ~0.5 s past where the
  stream started — proof someone is actually listening — then expands to the
  full 30 s window. A hammered track now churns ~1 MB instead of ~12 MB per
  press. Decode outruns playback ~50-100x, so the expansion never underruns;
  preloads idle at the small window too and expand after adoption.
- **Debounced fan-out** (`app/playback.rs`, `app/mod.rs`): `begin_now_playing`
  routes waveform/art through the existing `wave_want`/`art_want` 150 ms
  debounce slots (persistent single workers), and sets `preload_want` instead
  of preloading immediately; the Tick arm fires `preload_next` after 400 ms of
  now-playing stability, computing the prediction at fire time. A held key does
  no per-press background work at all.
- **Single-owner preload cancellation** (`audio.rs`): the loader thread never
  cancels handles after handoff. The decode thread cancels superseded handles
  as it drains `loaded_rx`, adopts an exact-match preload, and *keeps* a
  mismatched one alive for the upcoming gapless boundary.
- **Cancellable, serialized fallback** (`audio.rs`): the whole-file decode
  checks the cancel flag per packet (and skips launching ffmpeg when already
  cancelled), and all fallbacks serialize on a process-wide `FALLBACK_GATE`
  mutex with cancel-aware waiting, so at most one full decode is resident.

## Result

Same test, after the fixes:

```
baseline ~26 MB  ->  peak 41 MB during the hold
settles at ~53 MB (surviving track's full window + prefetch)
```

Gapless auto-advance verified intact: with 8 s tracks, quitting mid-second
track persisted `session_track = s2.mp3` at the expected playhead.

## Reproducing the measurement

Generate silent MP3s (`ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 90
trackN.mp3`), run muse against them in a pty with `HOME` pointed at a scratch
dir (isolates config writes), feed `n` at ~25/sec via a piped `script -q
/dev/null`, and sample `ps -o rss=` / `ps -M | wc -l` once a second. Use
`vmmap --summary <pid>` mid-burst to split live vs allocator-cached memory.

## Follow-up regression: frozen waveform (fixed in v0.38.0)

The v0.37.0 "prefer the up-front envelope once it's cached" guard in
`sync_stream_waveform` (`src/app/mod.rs`) froze the now-playing waveform:

1. The progressive stream fill inserts its snapshot into the same
   `wave_cache` the envelope lands in. On the very next tick the
   `contains_key` guard treated that partial snapshot as a final waveform and
   stopped refreshing it — so the waveform froze at whatever sliver had
   decoded on the first tick (tiny, since v0.37.1's 3 s warmup window slows
   early decode on purpose).
2. Worse, the 150 ms-debounced envelope request in the Tick arm checks
   `!wave_cache.contains_key(&path)` before firing, so the stopgap snapshot
   also suppressed the envelope compute entirely. Nothing ever replaced the
   frozen sliver.

Fix: track provenance with `wave_stream_stopgap: Option<PathBuf>`. A cache
entry written from `stream_bins()` is marked as a stopgap: it keeps refreshing
every tick, stays eligible for the envelope request, and the flag clears when
the real envelope arrives (`AppEvent::Wave`). On track switch,
`begin_now_playing` evicts a stale stopgap entry left behind for a different
track so it can't masquerade as that track's final waveform later.

## Round three: burst-debounced Open + allocator relief (v0.38.0)

v0.37.1 capped the per-press churn at the ~1 MB warmup window, but a held
`n`/`p` still spawned one decoder per press (the decode loop drains its cmd
channel every ~3 ms, so at ~25 presses/sec each press lands in its own batch),
and macOS libmalloc kept the freed windows cached dirty for ~35 s — RSS was
measured at ~229 MB on a real library. Each press also did an OS
media-metadata push and a cold-cache tag parse.

### Leading+trailing-edge Open debounce (`play_path_inner`)

- A lone press (or presses spaced > `OPEN_BURST_GAP` = 200 ms) sends
  `TransportCmd::Open` synchronously, exactly as before — zero added latency
  for Enter / clicks / single `n`.
- A press within 200 ms of the previous one is a burst: the UI
  (`note_now_playing` — now-playing title, debounced waveform/art/preload
  slots, history, shuffle bag) updates per press, but the engine keeps playing
  the old track. The surviving pick opens once presses stop for
  `OPEN_TRAILING_MS` = 150 ms (fired from the Tick arm), running
  `on_engine_open` (spectrum clear, tag ensure, one OS metadata push) once per
  burst instead of per press.
- Forced flushes keep transport actions on the displayed track: space/media
  Play/Pause/Toggle/Stop, all seeks (`seek_rel`/`seek_to_secs`), and `u`'s
  resume-SeekTo fire the deferred Open first. The old track ending mid-burst
  (`check_track_end`) and a stale gapless auto-advance (`poll_advance`) also
  fire it immediately. Session persist stores pos 0.0 for a pending pick.
- `sync_stream_waveform` returns early while an Open is deferred — the
  engine's live bins still belong to the OLD track and must not be cached
  under the new path.

### Allocator pressure relief (macOS)

`util::malloc_pressure_relief` wraps `malloc_zone_pressure_relief(NULL, 0)`
(libSystem; raw `unsafe extern "C"`, no new crate). Armed by
`fire_deferred_open` via `relief_want` and run from the Tick arm on a
throwaway thread ~2 s after a burst settles — after the survivor's 30 s window
expansion and the 400 ms-debounced preload have allocated — so the sweep
returns only truly dead pages. Lone presses never pay the zone walk.

### Batch-drained event loop

`main.rs` now drains everything queued on the event bus per iteration and
processes it as one batch: inputs (and other events) first, then a single
coalesced Tick, then at most one draw. This is load-bearing for the debounce:
if the UI thread stalls (slow draw, `vmmap` suspending the process, sleep/
wake), dozens of ticks and presses queue up — processed one-at-a-time in FIFO
order, a stale tick would run *ahead* of the queued presses, see an `open_want`
older than 150 ms, and fire the deferred Open mid-burst (measured: 7 spurious
opens per 10 s burst when a 1 Hz `vmmap` sampler kept suspending the process).
Inputs-first batching makes the trailing-edge check immune to loop lag; tick
coalescing also stops a backlog from replaying N housekeeping passes. Key
repeat shares the 16 ms `min_frame` redraw gate mouse moves use, with
`needs_settle` guaranteeing the final frame of a burst is drawn.

### Measured result (v0.38.0)

Same five-track pty test, `n` held at ~20 presses/sec for 12 s:
251 presses produced exactly 3 engine Opens (initial play, burst leading
edge, trailing-edge survivor) instead of ~250. RSS stayed flat (~54 MB) for
the entire hold and plateaued at ~73 MB once the survivor's 15 s look-behind
window filled — the plateau is the designed steady state, not growth. The
pressure-relief sweep reported 0 bytes to release in these runs: with the
churn gone at the source there is little for it to do; it stays as cheap
insurance for heavier real-world libraries (hi-res files, ffmpeg fallbacks).

Caveat for anyone re-measuring: sample with `ps -o rss=` or `footprint`, not
a tight `vmmap` loop — `vmmap` suspends the target while it snapshots, which
both distorts timing and (pre-batch-drain) triggered the very churn being
measured. And answer terminal probes (DA1 at minimum) if driving a pty
harness, or the graphics-capability probe reader will eat keystrokes.
