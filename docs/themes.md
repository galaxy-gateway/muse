# Theme system

Every animated theme is one `ThemeEffect` implementor in `src/effects/`
(border color, overlay, and input/track reactions live together); palettes
live in `src/config.rs` `THEMES`. Cycle with `t`, tune knobs with `tab`
inside the picker.

## Signal inputs available to effects (FrameCtx)

- `beat` — full-band onset pulse (0..1, decaying), `beat_bands` — per-band
  [bass, mid, treble] pulses.
- `tempo_phase` / `tempo_phase2` / `bpm` — the tempo tracker's phase-locked
  beat grid (v0.42.0, `src/beat.rs`): onset intervals are octave-folded into
  60-180 BPM, a PLL anchors phase to on-grid onsets, and confidence gates the
  values behind `Option` until trusted. `tempo_phase2` spans two beats (a
  pendulum's full swing). Effects can anticipate beats, not just react.
- `routed_beat()` — the pulse selected by the theme's `beat source` knob
  (all / bass / mid / treble), raw so themes apply their own beat-sync.
- `hover`, `cursor_row/index`, `play_frac`, `scope_peak`, panel rects
  (including `transport_rect` and `np_rect`).

## Hooks

`border`, `overlay`, `on_nav`, `on_click`, `on_scroll`, `ambient`, `wind`,
plus two newer ones:

- `on_track` (v0.40.0) — fired once per real audio switch (from
  `on_engine_open`), e.g. Tape's rewind smear.
- `wave_tint(base, ctx, frac, played)` (v0.49.0) — per-column waveform color,
  turning the waveform panel into a subtle visualizer (crt, tide, neon sign,
  metronome implement it).

## Themes added in v0.39-v0.47

| theme | idea | signature knobs |
|---|---|---|
| crt | amber phosphor: scanline sweep, shadow-mask rows, vignette, bass tube flicker, nav afterglow | persistence, beat source |
| tape deck | wow/flutter row shifts on the waveform, hiss dither on blank cells, rewind smear on track change | disruption |
| seismograph | three-channel strip recorder: bass/mid/treble pens in distinct inks on stacked baselines, scrolling time grid, track-change event stamps, nav knocks every pen (v0.54.0) | speed (paper), persistence (ink) |
| metronome | pendulum tick on the transport bar phase-locked via tempo_phase2, bpm readout when locked | beat sync, speed (idle sway) |
| tide | water band above the transport, bass swell, wind-drifted spray, underwater text tinting | wind, beat source |
| fireflies | steered wander (ambient mutates particle velocities), treble flares, hover = lantern | density, follow mouse |
| radar | full-screen PPI sweep tinting the UI it passes over; band onsets drop drifting contacts (bass inner, treble rim) that re-light each beam pass (v0.54.0) | speed, persistence, density |
| neon sign | per-glyph gas flicker over the now-playing panel, dead-tube letters, whole-sign dropouts, bass buzz | intensity, speed |

## Global modifiers (v0.48-v0.53)

- **Beat source knob** (`Choice` knob kind, v0.48.0): route which band drives
  a theme's reactions; stored as 0..1 detents so it persists like any knob.
- **Palette crossfade** (v0.50.0): theme switches lerp all nine palette
  colors over 300 ms (smoothstep) from whatever is currently displayed;
  effects switch instantly. `App::set_theme` / `refresh_palette` /
  `tick_theme_fade` in `src/app/mod.rs`.
- **Adaptive palette** (`a` in the picker, v0.51.0): retints the accent
  family toward the now-playing cover's two dominant colors
  (`media::dominant_colors`, saturation-weighted coarse histogram); body
  text stays untouched; colorless covers keep the theme palette.
- **Master effect level** (`[` / `]`, v0.52.0): scales every theme's tuning
  on its way into `FrameCtx` (0.5 = neutral); the knob editor still shows
  raw values.
- **No motion** (`m`, v0.52.0): keep any palette, force the static effect.
- **Live preview swatches** (v0.53.0): the picker's leading swatch cells run
  each theme's border animation in place.

## Adding a theme

1. New file in `src/effects/` implementing `ThemeEffect`; expose a `static`
   in `effects/mod.rs`.
2. Add a `Theme` palette entry in `config.rs` pointing at it.
3. Single-width glyphs only in overlays (wide chars drift columns).
4. If beat-reactive, prefer `routed_beat()` + a `BeatSource` knob with a
   default matching the theme's character.

## Flicker discipline (v0.53.1)

Animated color factors must be **stepped and quantized** (`color::qscale` /
`color::quant`): a per-frame sine or a decaying beat pulse fed straight into
`scale()` changes every affected cell's RGB every frame, which ratatui
faithfully rewrites — measured at 320-600k cell updates per 6s for the
v0.39-0.47 themes vs ~31k for snow, and perceived as whole-UI flicker.
Quantizing factors to ~1/16 steps and stepping noise/phase clocks to 4-10Hz
cut the churn 60-80% with no visible loss. Rules of thumb:

- Never alternate on frame parity (strobe); never re-roll noise per frame.
- Pass brightness factors through `quant`/`qscale` so identical frames map
  to identical RGB and the diff skips the cell.
- Full-screen tint loops must skip `FrameCtx::np_thumb` — repainting cells
  under the graphics-protocol cover image makes the terminal blink it.

Addendum (v0.53.2): never decimate the overlay itself. v0.37.0's paused-CPU
saving skipped the effect overlay on 3 of 4 frames, which strobed any
large-area theme on and off at 15Hz while paused (invisible on the old
beat-gated/sparse themes). The overlay now draws on every frame that reaches
the terminal; the saving moved to the main loop, which lowers the paused
tick-redraw rate to ~15fps (`App::effect_decimate`) so every frame is
complete and consistent.

Addendum (v0.53.3): two more anti-flash rules. (1) Borders stay STATIC —
no pre-existing theme animates border brightness, because any modulation
(smooth or quantized) re-colors every panel edge in lockstep and reads as
the palette flashing; rare, brief identity accents (crt dropout, neon
stutter) are fine. Same for whole-screen beat factors (crt's removed) — put
beat expression in localized overlay elements or wave_tint. (2) The theme
picker animates ONLY the highlighted row's swatch: running every row's
border animation at once (v0.53.0) made the list a wall of unsynchronized
color flashes, and the highlighted theme already previews across the whole
UI.

## Colorways (v0.55.0)

Themes with variants expose a `palette` Choice knob (first knob in the
editor). It indexes both the effect's internal color tables
(`tuning.choice(Knob::Palette)`) and `config::variant_anchors`, whose
(primary, secondary) anchors retint the whole UI through the same `retint`
machinery as the adaptive palette — so a colorway recolors borders, waveform,
and accents coherently, with a crossfade. Restored at startup (snapped, no
fade). Current sets:

| theme | colorways |
|---|---|
| crt | amber / green / ice |
| radar | green / amber / arctic |
| seismograph | classic / blueprint / thermal |
| tide | ocean / lagoon / abyss |
| neon sign | magenta / cyan / gold |
| tape deck | oxide / chrome / gold |
| fireflies | meadow / ember / ghost |
| metronome | brass / silver / rosewood |
| bamboo moon | moonlit / lantern / plum blossom |

As of v0.56.0 EVERY theme has a colorway set (the originals get UI-anchor
variants — the overlay keeps its identity colors). The registry is
`config::colorways` (names + anchor pairs in one table); `config::theme_knobs`
prepends the Palette knob for any theme with an entry, so shared/static
effects get it too. Also in v0.56.0: `m` toggles no-motion globally (not just
inside the picker), and the help modal is sectioned, styled, and scrollable
(j/k, g/G) with a percent indicator when the window is too small.

Also in this release: seismograph ink TINTS text it crosses instead of
overwriting glyphs (`ink_cell`), and the metronome grew a full-height
pendulum arm hung from the top of the screen (glyph-tinting like the radar
beam), with its bob flaring on each strike.

## Inspector wave-pane views (v0.57-v0.64)

The inspector's middle panel is a cycleable view (`W`, persisted):
waveform · spectrogram · bands · bitrate · dynamics · beat map · up next ·
art · lyrics. The data views (spectrogram/bands/bitrate/dynamics) read a
per-track analysis (band energy, RMS, packet bitrate) computed lazily by a
single-slot worker (`audio::analyze_track`) only while such a view is active.
The beat map logs live onsets against the playhead as you listen; lyrics load
from `.lrc` sidecars or embedded tags, sync-highlighted when stamped. All
time-axis views keep click-to-seek and the playhead column.

## Bamboo moon, and drawing scenery behind the UI (v0.65.0)

A terafox-palette night forest (`src/effects/bamboo_moon.rs`): a crescent in
the upper right, stars in the open sky, mist drifting across the lower third,
spirit motes wandering (one in twelve warm amber, the panda's satchel), and
moonlight travelling the waveform via `wave_tint`.

**Layer toggles.** The magic layers switch independently, reusing the knob
machinery rather than a parallel system. Layers with a natural magnitude ride
existing Range knobs where 0 = off (motes on `Density`, mist on `Wind`); the
binary ones got three new generic `Toggle` knobs — `Knob::Glow` / `Shimmer` /
`Glint`. `TuningCfg`'s fields are all `Option<f32>`, so old configs still
deserialize and the knob editor handles `KnobKind::Toggle` generically: no UI
changes were needed. **Keep new Toggle knobs out of `App::scaled_tuning`'s
list** — it scales Range knobs by the master effect level, and a toggle in
there would let `[` silently switch layers off.

**The moon is an overlay, not a backdrop.** It shows as light on what is
already on screen: a drawn crescent where it crosses open sky, and a lift of
the existing glyphs everywhere else, with a distance-falloff glow around it.
The effect is moonlight through the canopy — present in the gaps and on the
panel text, never a shape sitting behind the UI.

An earlier cut painted the crescent into the cell *background* (a terafox bg
ramp, guarded on `cell.bg == Color::Reset`). That does read as a single solid
shape through the panels, and it is recorded here because it is the obvious
thing to reach for — but it was rejected: it turns a chunk of the UI into a
lit patch, and it assumes the terminal's own background is dark. The overlay
is the quieter, more portable read, and it is the one the theme ships with.
Either way the factors are constant per cell with no time term, so the diff
rewrites nothing.

**Two rules for anything that draws over the UI:**

1. *Tint text, never overwrite it* (seismograph's `ink_cell` lesson). Motes
   calling `set_char` unconditionally turned `4.0 MB` into `4.ˑ MB` and ate
   letters out of filenames. Draw the glyph only on blank cells; elsewhere
   blend the existing fg.
2. *A blank cell is not automatically sky.* The lone space between two words
   is a gap in the UI — dropping a star into one renders
   `a walk in a park at✦night.mp3`. `open_sky` requires the cell and both
   horizontal neighbours to be blank.

Glyphs are limited to `·`, `•`, `✦`: `ˑ` (U+02D1) reads as tofu in Menlo and
other common terminal fonts.

`bamboo_moon.rs` carries unit tests over a `TestBackend`, including
`frames_within_a_clock_step_are_identical` — the flicker rule as an assertion
(two frames that share `frame/6` and `frame/8` must produce byte-identical
cells). `cargo test print_sky -- --ignored --nocapture` prints the crescent as
ASCII for eyeballing.
