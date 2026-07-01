# Beat-reactive Glitch theme

The Glitch theme animates in time with whatever is playing. On silence it is
near-still; louder passages fizz more, and every beat snaps in tears and flashes
that decay over a few frames.

## Signal path

1. `src/beat.rs` — `BeatState` watches the raw scope window every ~60 Hz tick and
   derives one cheap signal (no FFT):
   - `pulse` (0..1): onset spike when RMS energy jumps above its slow running
     baseline, decayed each tick → "the beat".
   When nothing plays, it bleeds to zero, so effects gated on it go quiet for
   free.
2. `App::handle` (`src/app/mod.rs`) calls `self.beat.update(&scope_buf, is_playing())`
   on every `Tick`, before the FFT (which only runs for the spectrum preset).
3. `frame_ctx()` copies `beat.pulse()` into `FrameCtx.beat`.
4. `src/effects/glitch.rs` reads it in `overlay`.

## What the beat drives (glitch.rs)

The overlay is **purely beat-driven** — there is no sustained fizz. Below the
`GATE` (`beat < 0.18`) it returns early, so between beats and on silence the
screen is clean. Above the gate, `hit` remaps the active range to 0..1 and every
element ramps from there:

- **Tears** — horizontal datamosh: single rows are snapshotted and shoved
  sideways with a cyan/magenta tint (`tear_row`). Count (`1..5`) and displacement
  scale with `hit`.
- **Thick tear band** — on a strong hit (`beat > 0.6`) a chunk of adjacent rows
  all slide together by the same offset, like a slab of the screen ripping
  sideways.
- **Scanline** — a bright swept line on a hit (`beat > 0.4`).

Every artifact is a **horizontal shift or sweep** — there is deliberately no
scattered per-pixel noise, so it reads as tearing/datamosh rather than confetti.

The border has no audio signal (fixed trait signature) so it stays a calm
breathing green with rare RGB micro-jumps.

## Tuning

All constants live in `src/beat.rs` (detection: `* 3.2` RMS gain, `* 1.25 + 0.02`
onset threshold, `* 0.80` pulse decay) and the thresholds/scales in
`glitch.rs::overlay`.
