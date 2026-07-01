# Plan: beat-reactive + configurable animated themes

Goal: every animated theme (a) reacts to the beat in some way and (b) exposes
tunable knobs with a reset-to-default (the modal already has the editor + reset
row). Non-animated themes (Static) are untouched.

Signals already available on `FrameCtx`: `beat` (full-band onset pulse 0..1),
`beat_bands` `[bass, mid, treble]`, `scope_peak`, `play_frac`, `hover`, `tuning`.

## Shared infrastructure (do first)

1. **Knob system, richer.** Today `Knob` is 3 range floats (Intensity /
   Persistence / Disruption) and `Tuning` a fixed struct. Extend:
   - Add `KnobKind { Range, Toggle }`. Range renders the existing bar; Toggle
     renders `on/off` and flips on `←/→`/`Enter` in the editor.
   - Grow the knob vocabulary (all `Range` 0..1 unless noted):
     `Intensity`, `Speed`, `Density`, `BeatSync`, `Wind`, `Persistence`,
     `Disruption`, `Strobe`, `FollowMouse` (Toggle).
   - `Tuning` gets a field per knob; `get/set` handle each; `TuningCfg` (serde)
     gains matching optional fields (backward-compatible).
   - Editor: format a Toggle row as `on`/`off`; everything else unchanged.
2. **`BeatSync` is universal.** Every theme reads `beat`/`beat_bands` scaled by
   its `BeatSync` knob. `BeatSync = 0` ⇒ pure ambient (today's look), so anyone
   who dislikes the reactivity can dial it out — and it's the natural "off".
3. **Beat in borders.** `border()` has no audio today. Add `beat: f32` to its
   signature (all impls ignore it by default) so border-only themes can pulse.
4. Every animated effect implements `knobs()` + `default_tuning()`. Reset works
   for free.

## Per-theme plans

Each: **beat** = how it reacts · **knobs** (defaults).

### Border-only (basic.rs)
- **Prismatic** — beat: hue jumps forward + saturation kick on each beat; hue
  cadence otherwise steady. Knobs: `Speed` (0.5), `BeatSync` (0.5).
- **Trans slow** — beat: gentle brightness pulse on the flag drift. Knobs:
  `Speed` (0.3), `BeatSync` (0.3).
- **Ripple** — beat: the glow pulse is *driven* by bass instead of a fixed sine.
  Knobs: `Speed` (0.5), `BeatSync` (0.6).
- **Cmyk** — beat: oscillation speed-ups + saturation kick on beat. Knobs:
  `Speed` (0.5), `BeatSync` (0.5).

### Particle / overlay themes
- **Snow** — beat: bass hit = a wind gust (sway burst) + brief faster fall;
  treble = extra sparkle flakes. Knobs: `Density` (0.5), `Speed` (0.4),
  `Wind` (0.3), `BeatSync` (0.4).
- **Flame** — beat: bass flares the burning playhead + border flare + a spark
  burst; scope shoot-offs scale with beat. Calcifer bobs on the beat. Knobs:
  `Intensity` (0.6), `BeatSync` (0.6), `Speed` (0.5).
- **Flag** — beat: fireworks auto-launch on strong beats (not just clicks); the
  flag shimmers/waves on beat. Knobs: `Intensity` (firework size, 0.5),
  `BeatSync` (0.5).
- **Electric** — beat: lightning strikes fire on bass beats (replaces the raw
  `scope_peak` trigger); border white-arcs on beat; treble = crackle. Knobs:
  `Intensity` (0.6), `BeatSync` (0.7), `Speed` (0.5).
- **Matrix** — beat: rain spawn-rate + head brightness pulse on beat; bass hit
  spawns a wave of columns. Knobs: `Density` (0.5), `Speed` (0.5),
  `Persistence` (trail length, 0.5), `BeatSync` (0.5).
- **Bubbles (aqua)** — beat: bass = a burst of fast-rising bubbles; spawn rate
  pulses. Knobs: `Density` (0.5), `Speed` (0.4), `BeatSync` (0.4).
- **Starfield (cosmic)** — beat: warp speed surges on the beat; bass = warp
  burst, strong beat = supernova. Knobs: `Speed` (warp, 0.5), `Density`
  (star count, 0.5), `BeatSync` (0.6).
- **Sakura** — beat: petals get a downward gust + spawn pulse on beat. Knobs:
  `Density` (0.4), `Wind` (drift strength, 0.3), `FollowMouse` (toggle, **on** —
  the mouse-blows-petals behavior, now optional), `Speed` (fall rate, 0.3),
  `BeatSync` (gust, 0.3). (Matches the requested "how petals fall / follow mouse"
  controls.)
- **Rave** — beat: convert the `scope_peak` triggers to `beat_bands` — bass =
  fireworks + a strobe-band flash, treble = confetti burst; border hue rate
  tracks energy. Knobs: `Intensity` (confetti density, 0.7), `Strobe` (band
  amount, 0.6), `BeatSync` (0.8), `Speed` (0.6).

### Already done (keep, optionally add `BeatSync`)
- **Glitch / Datamosh / Meltdown** — already beat-driven with knobs. Optionally
  add `BeatSync` for parity so they can be calmed the same way.

## Rollout

1. **Phase 1 — infra:** ✅ done — `KnobKind { Range, Toggle }`, expanded `Knob`
   vocabulary + `Tuning` (with `Default`) + serde `TuningCfg`, and the modal
   editor renders Toggle rows as `● on`/`○ off` (flip with `←`/`→`). No behavior
   change: existing themes still expose only Range knobs.
   - **Deferred:** `beat` in `border()`. Threading it through every UI `border()`
     call site is invasive and only border-only themes need it; do it in that
     Phase-2 batch (overlay themes already read `ctx.beat`).
2. **Phase 2 — themes in batches:** ✅ done — all animated themes now expose
   `knobs()`/`default_tuning()` and react to the beat:
   - **Particle:** Snow (density/speed/wind/beat-sync — bass gust), Bubbles
     (density/speed/beat-sync — bass burst), Starfield (density/speed/beat-sync —
     warp surge + supernova), Sakura (density/speed/wind/beat-sync + **FollowMouse
     toggle** — beat gust).
   - **Border-only:** Prismatic, TransSlow, Ripple, Cmyk — pulse via `beat`
     (already beat-sync-scaled by `App::beat_pulse`), so they expose only
     `BeatSync`. `border()` gained a `beat` param threaded from every UI call site.
   - **Rich:** Flame (intensity/beat-sync/speed — ember eruptions + border flare),
     Electric (bolts on bass beat + white-arc border), Matrix (density/speed/
     persistence/beat-sync — bass column wave), Rave (intensity/strobe/beat-sync/
     speed — bass fireworks, beat strobe bands), Flag (intensity/beat-sync —
     auto-fireworks).
   - Glitch/Datamosh/Meltdown unchanged (already beat-driven).

   **Design notes:** `App::beat_pulse()` = raw pulse × active theme's `beat_sync`,
   so borders get beat-sync for free without threading `Tuning` into `border()`.
   Overlay/ambient themes read `ctx.tuning` + `ctx.beat`/`ctx.beat_bands`
   directly. `Tuning::set` snaps toggles to 0/1; `scale()` is only ever called
   with f ≤ 1 (channels would wrap past 255).
3. **Phase 3 — polish:** ✅ docs cross-linked, help note updated
   (`tab (in picker) tune animated themes`). Defaults are reasonable starting
   values but the *feel* pass (whether each theme's default `BeatSync` is too
   hot / too subtle) needs eyes + ears on a real terminal — adjust the
   `default_tuning()` numbers per theme as needed. Note: beat pulses stay above
   threshold for a few frames, so burst-spawning themes fire a short swell per
   beat (bounded by the 700-particle cap); if any read as too dense, gate their
   spawn on a rising-edge onset instead of `beat > threshold`.

Estimate: Phase 1 is the real work (shared plumbing + editor toggle). Phases 2/3
are mechanical per-theme once the plumbing exists.
