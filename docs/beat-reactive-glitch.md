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

## Datamosh theme (`src/effects/datamosh.rs`)

A glitchier sibling of Glitch that keeps the same beat-driven, horizontal-tearing
rule but adds three things, all fed by the per-band pulses in `FrameCtx.beat_bands`
(`[bass, mid, treble]`, from `BeatState::bands()`):

1. **Frequency-split beats.** Bands drive different artifacts instead of one
   whole-signal pulse: **bass** → a thick chromatic slab tear (`bass > 0.35`) and
   a vertical roll on a hard hit (`bass > 0.6`); **mid** → individual torn rows
   (count scales); **treble** → fine bright scanline sweeps (`treble > 0.25`).
2. **RGB chromatic split** (`chroma_tear_row`). Torn rows are drawn as
   interleaved cyan/magenta/green subpixel columns, each pulled from a different
   `split` offset, so displaced glyphs fringe apart like a mistracked CRT.
3. **Vertical roll** (`vertical_roll`). A hard bass hit wraps the whole frame
   up/down 1–2 rows for one frame — the picture losing vertical sync.

`bands()` splits the mono scope with two cascaded one-pole low-passes (bass =
`lp 0.025`, mid = residual `lp 0.20`, treble = the leftover), then runs the same
onset detector per band with per-band gains (bass ×4, mid ×6, treble ×10).

## Meltdown theme (`src/effects/meltdown.rs`)

The glitchiest of the three: datamosh with **P-frame persistence**. Instead of
row shifts that snap clean, it corrupts in **rectangular macroblocks** that drag
content along a motion vector and *linger*, fading over ~15 frames — the picture
melts on a beat and slowly heals between hits, like a codec losing reference
frames.

- **Ghost buffer** — a module-local `static Mutex<Ghost>` holds a decaying grid
  of `(glyph, color, life)` cells laid over the finished frame each render. Kept
  self-contained so the effect stays a plain `&self` static (no trait/App/ctx
  changes). Resizes/clears on screen-size change; bails cheaply once healed and
  silent.
- **Macroblock smear** (`smear_block`) — on a bass/mid beat, blocks of the real
  UI are copied offset by a motion vector and baked into the ghost. Bass →
  bigger, faster-dragging blocks; mid → more blocks.
- **Treble skitter** — many tiny 1-cell smears with short random jumps for fine
  high-frequency breakup.
- **Chroma** — most smears keep the source color (so dragged text stays
  readable); some are forced to a cyan/magenta channel.
- **Bloom** — sustained band energy lifts the whole ghost brighter.

`FADE` (0.88/frame) controls trail length; `GATE` the per-band floor.

Spectrum of the three: `glitch` = clean CRT row tears · `datamosh` = aggressive
RGB row-rip + vertical roll · `meltdown` = codec melt, smearing and healing.

## Per-theme tuning (theme modal)

The three glitch themes are user-configurable. In the theme picker (`t`),
highlight one and press **Tab** to open an inline knob editor; `↑↓` selects a
knob, `←→` adjusts (±0.05, live-previewed), `Esc` returns to the list, `⏎`
applies. Values persist per theme in `config.toml` under `theme_tuning`.

- Knobs are 0..1 floats defined by `effects::Knob` (`Intensity`, `Persistence`,
  `Disruption`). Each effect declares which it exposes via `ThemeEffect::knobs()`
  and its starting values via `default_tuning()`.
- The active theme's values ride to the effect on `FrameCtx.tuning`. Stored on
  `App.tunings` (parallel to `THEMES`), seeded from `default_tuning()` then
  overridden by saved `TuningCfg`s; a `tunings_backup` snapshot restores them if
  the picker is cancelled.
- **Intensity** scales how many artifacts fire; **Disruption** scales how far
  they displace the real UI (drag distance, and it gates datamosh's vertical roll
  and meltdown's block size); **Persistence** (meltdown only) maps to the ghost
  fade — short blink up to long gooey trails.

**Meltdown ships toned down** (`intensity 0.5, persistence 0.35, disruption
0.25`) so the app stays readable by default; crank the knobs for full chaos.

## Tuning

All constants live in `src/beat.rs` (detection: `* 3.2` RMS gain, `* 1.25 + 0.02`
onset threshold, `* 0.80` pulse decay) and the thresholds/scales in
`glitch.rs::overlay`.
