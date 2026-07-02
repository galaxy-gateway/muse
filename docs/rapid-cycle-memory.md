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
