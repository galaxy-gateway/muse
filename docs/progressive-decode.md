# Progressive decode (instant playback)

## Problem

Opening a track used to fully decode **and resample the entire file** on the
decode thread before flipping `playing = true` (`audio.rs`, `TransportCmd::Open`).
For a multi-minute song that is tens–hundreds of ms of dead air between clicking
and hearing audio. Only the predicted-next track escaped it, via the gapless
preloader — arbitrary clicks did not.

## Fix: prefix-then-swap

On an `Open` that is **not** already prefetched:

1. **Decode a ~2 s prefix synchronously** (`load_prefix_safe` → `decode_any(path,
   Some(PREFIX_SECS))`). Decoding 2 s of audio takes a few ms (decode runs far
   faster than realtime), so playback starts effectively instantly. The real
   track duration comes from the container's frame count, so the seek bar is
   correct immediately even though only the prefix is decoded.
2. **Start playing the prefix** and fire a request to the background
   full-decoder thread for the complete track.
3. **Swap the full buffer in seamlessly** when it arrives (`full_done_rx` drain
   at the top of `decode_loop`). The playback cursor is preserved.

The swap is glitch-free because the prefix and the full track are produced by the
**same deterministic resampler** from the same leading input, so their early
frames are sample-identical. The swap happens at the cursor position reached
during the background decode (well under a second in), which is deep inside that
identical region and far from the prefix's truncation boundary.

### Why 2 s

`PREFIX_SECS = 2.0` gives ~2 s of playable audio while the full decode (typically
sub-second wall time) completes. If the full buffer somehow isn't ready when the
cursor reaches the prefix end, the loop **holds** rather than advancing (guarded
by `partial`), so a pathologically slow decode degrades to a brief silence, never
a premature track-skip.

## Threads

- **Preloader** (`load_tx` → `loaded_rx`): unchanged; decodes the predicted-next
  track for gapless boundaries.
- **Full-decoder** (`full_req_tx` → `full_done_rx`): new; decodes the full track
  behind a prefix start. Separate channel so a click never fights the gapless
  prefetch.

Both collapse rapid requests to the most recent, so fast navigation never backs
up work.

## Edge cases

- **ffmpeg / archive fallback:** ffmpeg has no prefix mode, so those paths return
  the whole file as the "prefix" (a full sync decode — the old behavior for the
  rare ffmpeg-only file). Symphonia-decodable archive entries still get a real
  prefix.
- **Rapid switching:** superseded `Open`s are still skipped (only the last is
  decoded); a stale full-decode result whose path no longer matches
  `current_path` is dropped.
- **Gapless splice** at end-of-track is untouched — it already had the full
  buffer in hand.

## Not chosen

- *Preload every visible row:* thrashes CPU on scroll and still needs streaming
  playback to use a prefix.
- *Full streaming decode with a stateful streaming resampler:* the theoretically
  cleanest (zero speculative work) but a larger, riskier rewrite. Prefix-then-swap
  gets the same instant-start result reusing the existing whole-buffer resampler.
