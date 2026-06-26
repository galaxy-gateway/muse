# muse — Architecture

A terminal music explorer: a lazy file tree on the left, a metadata/waveform/
oscilloscope inspector on the right, a transport bar across the bottom, and a
per-theme animation layer painted over everything.

This document describes how the pieces fit together and where to add new ones.
It reflects the module layout after the `effects` + `app/` + `ui/` refactor.

## Threading model

muse runs four threads, joined by a single `crossbeam` channel of `AppEvent`s.
The UI thread owns all mutable state; every other thread only *produces events*.

```
            ┌──────────────┐   AppEvent::Input/Mouse   ┌────────────────┐
 terminal ─▶│ input thread │──────────────────────────▶│                │
            └──────────────┘                            │                │
            ┌──────────────┐   AppEvent::Tick (~60Hz)   │   UI thread    │
            │ ticks thread │──────────────────────────▶│  (event loop)  │
            └──────────────┘                            │                │
            ┌──────────────┐   AppEvent::Index          │  App::handle   │
            │ index thread │──────────────────────────▶│  ui::draw      │
            └──────────────┘                            │                │
            ┌──────────────┐   AppEvent::Wave           │                │
            │ waveform thr │──────────────────────────▶│                │
            └──────────────┘                            └───────┬────────┘
                                                                │ TransportCmd
            ┌──────────────────────────────────────────────────▼────────┐
            │ AudioEngine: decode thread ─rtrb ring─▶ cpal callback (RT) │
            │                              triple_buffer tap ─▶ scope    │
            └────────────────────────────────────────────────────────────┘
```

- **input** (`event::spawn_input`) — blocking `crossterm::read`, posts key/mouse.
- **ticks** (`event::spawn_ticks`) — ~60Hz heartbeat driving the live scope,
  playhead, and particle integration.
- **index** (`event::spawn_index`) — walks the whole tree once and posts a flat
  media-file list for the fuzzy filter, so filtering never blocks the UI.
- **waveform** (`app/nav.rs::request_waveform`) — one-shot per selected/played
  track; decodes + bins off-thread, posts `AppEvent::Wave`.

The audio spine (`audio.rs`) is self-contained: its own decode thread feeds a
lock-free `rtrb` ring consumed by the real-time `cpal` callback. The callback is
the single source of truth for "what is audible now" — it copies the same PCM
block into a `triple_buffer` tap that feeds the oscilloscope, so the visual is
sample-synced to the DAC. No locks or allocations on that path. The UI thread
talks to it only through `TransportCmd` (open/toggle/seek/volume) and reads
atomics (position, duration, playing, volume).

`main.rs` wires the channel, spawns the threads, and runs the loop: drain an
event → `app.handle(ev)` → redraw (tick redraws are coalesced to ~60fps).

## Module map

| module | responsibility |
|---|---|
| `main.rs` | terminal setup/teardown, event loop, `--probe` smoke test |
| `event.rs` | `AppEvent` enum + the producer threads |
| `model.rs` | `TreeModel`: lazy file tree + flattened visible list |
| `media.rs` | `MediaProvider` trait + `Registry` (the media-type seam) |
| `audio.rs` | `AudioEngine`, decode/resample, `waveform_bins` |
| `spectrum.rs` | `SpectrumState`: FFT (rustfft) → mel-spaced bands for the spectrum visualizer |
| `config.rs` | `Theme`/`THEMES`, `ScopePreset`, persisted `Settings` |
| `color.rs` | shared color math (hue, gradient, glow, scale) |
| `particles.rs` | `ParticleSim`: the `Spark` pool + spawn/integrate primitives |
| `effects/` | one `ThemeEffect` per animation (see below) |
| `app/` | `App` state + behavior (see below) |
| `ui/` | all rendering, one file per screen region (see below) |
| `util.rs` | formatting + `fuzzy_score` + `noise` |

### `app/` — state and behavior

`App` is a single struct (in `app/mod.rs`) holding all mutable UI-thread state.
Its behavior is split across sibling modules; their methods are `pub(super)` so
they compose within `app` without exposing internals to the rest of the crate.

| file | what it owns |
|---|---|
| `app/mod.rs` | the `App` struct + fields, `new`, `handle` dispatch, `frame_ctx`, read-only accessors (`cursor`, `cursor_path`, `scope_preset`, …), `LoopMode` |
| `app/input.rs` | keyboard: normal keys, fuzzy-filter editor, cursor move/select, scope cycle, theme picker |
| `app/nav.rs` | tree expand/collapse/enter, cursor-stable rebuilds, lazy metadata + waveform requests |
| `app/playback.rs` | `play_path`, next/prev, end-of-track loop logic |
| `app/mouse.rs` | wheel scroll, click/drag seek, tree row hit-testing |
| `app/mediakeys.rs` | souvlaki OS now-playing controls + metadata push |

`App::handle` is the only entry point. It matches on `AppEvent` and routes to
`on_key` / `on_mouse` / `on_media`, or runs the per-tick update (scope copy,
end-of-track check, OS sync, effect ambient spawn, particle integration).

### `ui/` — rendering

Immediate-mode: reads `App`, draws, never mutates (except recording hit-test
rects for the mouse handler). `ui::draw` lays out the regions and calls one
function per panel.

| file | panel |
|---|---|
| `ui/mod.rs` | layout + `draw` entry, records hit rects, invokes the effect overlay |
| `ui/widgets.rs` | shared `panel`/`panel_hint` blocks, the effect-driven `border`, hover-seek guide |
| `ui/tree.rs` | file tree / fuzzy results + selection detail panel |
| `ui/inspector.rs` | now-playing header, static waveform, live oscilloscope |
| `ui/transport.rs` | bottom progress/volume/loop bar |
| `ui/modals.rs` | help sheet + theme picker overlays |

### `effects/` — theme animations

Each animated theme is one `ThemeEffect` implementor in its own file, owning its
**whole** behavior: border color, screen overlay, and particle reactions to
navigation / click / scroll / ambient ticks. Before the refactor this was a
15-variant `Anim` enum dispatched by `match` in ~40 places across `app` and
`ui`; now it is trait dispatch through `Theme.effect: &'static dyn ThemeEffect`.

```rust
pub trait ThemeEffect: Sync {
    fn is_animated(&self) -> bool { true }
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color { base }
    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {}
    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {}
    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {}
    fn on_scroll(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {}
    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {}
    fn wind(&self, ctx: &FrameCtx) -> Option<f32> { None }
}
```

Every hook has a no-op/identity default, so an effect overrides only the axes it
uses (e.g. `basic::Prismatic` overrides just `border`; `Snow` just `overlay`).

`FrameCtx` is a `Copy` snapshot of per-frame geometry + playback state
(`frame`, screen/panel rects, hover, `scope_peak`, `cursor_row`, `play_frac`).
`App::frame_ctx()` builds it; effects read it instead of borrowing `App`. This
keeps the borrow checker happy: callers copy the `&'static` effect pointer out,
build the owned ctx, then pass `&mut self.sim` freely.

`ParticleSim` (`particles.rs`) owns the `Spark` pool and the spawn/integrate
primitives (`burst`, `warp`, `push`, `cap`, `update`). The *policy* (which
spawn, when) lives per-theme in the effect; the *mechanism* lives in the sim.

## Key data flows

**Selection → inspector.** Moving the cursor calls `on_selection_changed`
(`app/nav.rs`), which lazily fills `meta_cache` (synchronous tag read) and fires
an off-thread `request_waveform`. The waveform arrives later as `AppEvent::Wave`
and lands in `wave_cache`, keyed by path. `ui/inspector.rs` reads both caches.

**Play.** `play_path` sends `TransportCmd::Open`, sets `now_playing`, pushes OS
metadata, and ensures a waveform exists. The decode thread loads the track; the
cpal callback starts pulling PCM and publishing scope frames.

**End-of-track.** Each tick, `check_track_end` watches the play→stop falling
edge near the duration and applies `LoopMode` (off = advance-then-stop, all =
wrap, one = repeat).

**Fuzzy filter.** `/` enters filter mode; keystrokes rebuild `filtered` by
scoring the background `index` with `util::fuzzy_score`. While a filter is
active the list view is the flat results, and `cursor()` returns `None` (no tree
node), which gates expand/collapse.

**Mouse seek.** `ui` records `wave_rect` / `transport_rect` each draw;
`app/mouse.rs` hit-tests clicks against them and converts column → fraction →
`TransportCmd::SeekTo`. A left-drag keeps scrubbing the latched rect. All seeks
route through `seek_rel` / `seek_to_secs` (in `app/mouse.rs`), which clear the
spectrum's FFT history so it doesn't smear across the discontinuity.

**Spectrum visualizer.** When the `spectrum` scope preset is active,
`ui/inspector.rs::draw_spectrum` reads `SpectrumState::bands()` and renders them
as braille bars. The bands are recomputed each tick in `App::handle` (FFT of the
scope window while playing, exponential decay while paused). It is a `ScopeStyle`
variant, so it shares the `v`/`V` preset cycle with the oscilloscope modes.

## Extending muse

- **New animation/theme** — add a file in `effects/` implementing `ThemeEffect`,
  expose a `pub static`, add a `&crate::effects::NAME` to a `Theme` literal in
  `config.rs`. No core file changes; no `match` arms to update.
- **New media type** (image/video) — add a `MediaProvider` in `media.rs` and
  register it in `Registry::new`. The tree, inspector, and player ask the
  registry; they never name "audio" directly. `Meta.fields` carries arbitrary
  display rows so unrelated media show different details generically.
- **New panel** — add a file in `ui/`, render from `App` state, wire it into the
  layout in `ui/mod.rs::draw`. Record a hit rect on `App` if it needs mouse.
- **New key / mouse gesture** — extend the match in `app/input.rs` or
  `app/mouse.rs`. Cross-module helpers are already `pub(super)`.
- **New scope style/mode** — add a `ScopeStyle`/`ScopeMode` variant and a
  `ScopePreset` in `config.rs`; handle it in `ui/inspector.rs::draw_scope`.

## Invariants worth keeping

- The cpal callback stays allocation-free and lock-free. Anything heavier than
  copying a block belongs on the decode thread or the UI thread.
- `ui` reads `App`, never mutates it — except writing the hit-test rects that
  the mouse handler reads back. Keep that the only exception.
- Effects mutate only the `ParticleSim` handed to them; they read `FrameCtx`,
  not `App`. Don't thread `&App` into an effect.
- Waveform bins are cached by path and are always valid for that path,
  regardless of the generation token (the token only cancels the *pending*
  marker, never the cache write).
```
