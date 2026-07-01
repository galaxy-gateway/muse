//! Theme effects: each animated theme is one `ThemeEffect` implementor in its
//! own file. A theme's whole behavior — border color, screen overlay, and the
//! particle reactions to navigation/click/scroll/ambient — lives together,
//! instead of being smeared across `app` and `ui` match arms.
//!
//! Adding an animation = add one file here implementing `ThemeEffect`, expose a
//! `static` instance, and point a `Theme` at it in `config`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

mod basic;
mod bubbles;
mod datamosh;
mod electric;
mod flag;
mod flame;
mod glitch;
mod matrix;
mod meltdown;
mod rave;
mod sakura;
mod snow;
mod starfield;

/// How a knob is edited/displayed in the theme modal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KnobKind {
    /// Continuous 0..1, shown as a bar.
    Range,
    /// Boolean, stored as 0.0/1.0, shown as `on`/`off`.
    Toggle,
}

/// One user-tunable knob on a configurable theme. Values live on `Tuning` (all
/// stored as f32 — a `Toggle` is just 0.0/1.0 so one editor handles both kinds).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    /// Overall amount / density / frequency of the effect.
    Intensity,
    /// Animation rate.
    Speed,
    /// How many particles/elements are alive at once.
    Density,
    /// How strongly the theme reacts to the beat. 0 = pure ambient (no beat
    /// coupling) — the universal "calm it down" / off switch.
    BeatSync,
    /// Horizontal drift strength (snow, sakura).
    Wind,
    /// Trail length — how long things linger before fading (meltdown, matrix).
    Persistence,
    /// How much the effect displaces the *real* UI (glitch family).
    Disruption,
    /// Strobe / flashing-band amount (rave).
    Strobe,
    /// Toggle: particles chase the mouse pointer (sakura).
    FollowMouse,
}

impl Knob {
    pub fn label(self) -> &'static str {
        match self {
            Knob::Intensity => "intensity",
            Knob::Speed => "speed",
            Knob::Density => "density",
            Knob::BeatSync => "beat sync",
            Knob::Wind => "wind",
            Knob::Persistence => "persistence",
            Knob::Disruption => "disruption",
            Knob::Strobe => "strobe",
            Knob::FollowMouse => "follow mouse",
        }
    }

    pub fn kind(self) -> KnobKind {
        match self {
            Knob::FollowMouse => KnobKind::Toggle,
            _ => KnobKind::Range,
        }
    }
}

/// A configurable theme's live knob values. Non-configurable themes ignore it.
/// Every field is 0..1 (toggles use 0.0/1.0). `Default` is a neutral baseline;
/// each theme overrides only the fields it exposes via `default_tuning`.
#[derive(Clone, Copy)]
pub struct Tuning {
    pub intensity: f32,
    pub speed: f32,
    pub density: f32,
    pub beat_sync: f32,
    pub wind: f32,
    pub persistence: f32,
    pub disruption: f32,
    pub strobe: f32,
    pub follow_mouse: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            intensity: 0.6,
            speed: 0.5,
            density: 0.5,
            beat_sync: 0.5,
            wind: 0.3,
            persistence: 0.3,
            disruption: 0.5,
            strobe: 0.5,
            follow_mouse: 1.0,
        }
    }
}

impl Tuning {
    pub fn get(self, k: Knob) -> f32 {
        match k {
            Knob::Intensity => self.intensity,
            Knob::Speed => self.speed,
            Knob::Density => self.density,
            Knob::BeatSync => self.beat_sync,
            Knob::Wind => self.wind,
            Knob::Persistence => self.persistence,
            Knob::Disruption => self.disruption,
            Knob::Strobe => self.strobe,
            Knob::FollowMouse => self.follow_mouse,
        }
    }

    pub fn set(&mut self, k: Knob, v: f32) {
        // Snap toggles to 0/1 so stored values stay clean.
        let v = if k.kind() == KnobKind::Toggle {
            if v >= 0.5 { 1.0 } else { 0.0 }
        } else {
            v.clamp(0.0, 1.0)
        };
        match k {
            Knob::Intensity => self.intensity = v,
            Knob::Speed => self.speed = v,
            Knob::Density => self.density = v,
            Knob::BeatSync => self.beat_sync = v,
            Knob::Wind => self.wind = v,
            Knob::Persistence => self.persistence = v,
            Knob::Disruption => self.disruption = v,
            Knob::Strobe => self.strobe = v,
            Knob::FollowMouse => self.follow_mouse = v,
        }
    }

    /// Convenience for `Toggle` knobs.
    pub fn is_on(self, k: Knob) -> bool {
        self.get(k) >= 0.5
    }
}

/// Per-frame geometry + playback snapshot handed to effects. Owned/Copy so it
/// can be built once and passed around without borrowing `App`.
#[derive(Clone, Copy)]
pub struct FrameCtx {
    pub frame: u64,
    pub screen: Rect,
    pub tree_rect: Rect,
    pub scope_rect: Rect,
    pub wave_rect: Rect,
    /// Now-playing panel (the inspector's top row) — used by the flag overlay.
    pub np_rect: Rect,
    pub hover: Option<(u16, u16)>,
    /// Peak abs amplitude of the current scope window.
    pub scope_peak: f32,
    /// Beat onset pulse, 0..1: spikes on a beat, decays over a few frames.
    pub beat: f32,
    /// Per-band onset pulses `[bass, mid, treble]`, each 0..1.
    pub beat_bands: [f32; 3],
    /// The active theme's user-tuned knob values.
    pub tuning: Tuning,
    /// Selection's visible row within the tree's inner area (sel - scroll), if any.
    pub cursor_row: Option<u16>,
    /// Absolute selected list index — seeds nav-burst RNG so the scatter pattern
    /// is stable per item regardless of scroll offset.
    pub cursor_index: Option<u32>,
    /// Playhead fraction (0..1) when a track is playing, else `None`.
    pub play_frac: Option<f64>,
}

/// One animated theme's behavior. Every hook has a no-op/identity default, so an
/// effect only overrides the axes it actually uses.
pub trait ThemeEffect: Sync {
    /// Whether this theme animates at all (drives the picker's `✦` tag).
    fn is_animated(&self) -> bool {
        true
    }

    /// User-editable knobs for this theme (empty = not configurable). Order is
    /// the order shown in the theme modal.
    fn knobs(&self) -> &'static [Knob] {
        &[]
    }

    /// Starting knob values, used until the user overrides them. Only meaningful
    /// for themes that expose `knobs`.
    fn default_tuning(&self) -> Tuning {
        Tuning::default()
    }

    /// Border accent for a panel: the theme's static `base`, or an animated
    /// color. `offset` spreads the animation across panels; `beat` (0..1, already
    /// scaled by the theme's beat-sync knob upstream) lets borders pulse.
    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        base
    }

    /// Draw on top of the finished frame (particles, sprites, screen artifacts).
    fn overlay(&self, _f: &mut Frame, _sim: &ParticleSim, _ctx: &FrameCtx) {}

    /// Cursor moved by `dir` (-1 up, +1 down).
    fn on_nav(&self, _sim: &mut ParticleSim, _ctx: &FrameCtx, _dir: f32) {}

    /// Mouse clicked at (col,row).
    fn on_click(&self, _sim: &mut ParticleSim, _ctx: &FrameCtx, _col: u16, _row: u16) {}

    /// Mouse-wheel scrolled the list.
    fn on_scroll(&self, _sim: &mut ParticleSim, _ctx: &FrameCtx) {}

    /// Per-tick continuous spawning (rain, bubbles, scope shoot-offs, ...).
    fn ambient(&self, _sim: &mut ParticleSim, _ctx: &FrameCtx) {}

    /// Optional horizontal wind target for particle integration (Sakura gust).
    fn wind(&self, _ctx: &FrameCtx) -> Option<f32> {
        None
    }
}

/// Draw every live particle with `glyph(age_fraction, seed)`. Shared by the
/// simple particle themes and reused inside richer overlays.
pub(crate) fn render_sparks(
    f: &mut Frame,
    sim: &ParticleSim,
    screen: Rect,
    frame: u32,
    glyph: fn(f64, u32) -> (char, Color),
) {
    let buf = f.buffer_mut();
    for p in &sim.sparks {
        let (x, y) = (p.x.round() as i32, p.y.round() as i32);
        if x < screen.x as i32
            || x >= screen.right() as i32
            || y < screen.y as i32
            || y >= screen.bottom() as i32
        {
            continue;
        }
        let (ch, col) = glyph(p.age as f64 / p.life.max(1) as f64, p.seed ^ frame);
        let cell = &mut buf[(x as u16, y as u16)];
        cell.set_char(ch);
        cell.set_fg(col);
    }
}

/// Navigation burst at the cursor row, travelling vertically in `dir` with
/// x-jitter. Shared by the Flame and Electric themes.
pub(crate) fn nav_sparks(sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {
    let r = ctx.tree_rect;
    if r.width < 3 || r.height < 3 {
        return;
    }
    let vis = ctx.cursor_row.unwrap_or(0);
    let idx = ctx.cursor_index.unwrap_or(0);
    let y = (r.y + 1 + vis).min(r.y + r.height - 2) as f32;
    let base_x = (r.x + 1) as f32;
    let span = r.width.saturating_sub(2).max(1) as u32;
    for i in 0..18u32 {
        let seed = noise(ctx.frame as u32, i + idx * 7);
        let vx = ((seed % 7) as f32 - 3.0) * 0.12;
        let vy = dir * (0.4 + (seed % 3) as f32 * 0.18);
        sim.push(Spark {
            x: base_x + (seed % span) as f32,
            y,
            vx,
            vy,
            age: 0,
            life: 9 + (seed % 8) as u16,
            seed,
        });
    }
    sim.cap();
}

/// Shoot particles off the oscilloscope when it peaks loud. Shared by the Flame
/// and Electric themes; called each ambient tick (self-gates on frame + peak).
pub(crate) fn scope_sparks(sim: &mut ParticleSim, ctx: &FrameCtx) {
    if ctx.frame % 2 != 0 {
        return;
    }
    let r = ctx.scope_rect;
    if r.width < 3 || r.height < 3 {
        return;
    }
    let peak = ctx.scope_peak;
    if peak < 0.08 {
        return;
    }
    let n = (((peak - 0.08) * 12.0) as u32 + 1).min(6);
    let span = (r.width - 2) as u32;
    // Launch from the middle of the scope (where the trace sits), not the top.
    let mid = (r.y + r.height / 2) as f32;
    for i in 0..n {
        let seed = noise(ctx.frame as u32 + i, peak.to_bits());
        sim.push(Spark {
            x: (r.x + 1) as f32 + (seed % span) as f32,
            y: mid + ((seed / 7 % 3) as f32 - 1.0),
            vx: ((seed % 9) as f32 - 4.0) * 0.18,
            vy: -(0.4 + (seed % 4) as f32 * 0.18), // upward off the scope
            age: 0,
            life: 8 + (seed % 8) as u16,
            seed,
        });
    }
    sim.cap();
}

// --- Registry: one static instance per effect, referenced from `config`. -----
pub static NONE: basic::Static = basic::Static;
pub static PRISMATIC: basic::Prismatic = basic::Prismatic;
pub static TRANS_SLOW: basic::TransSlow = basic::TransSlow;
pub static RIPPLE: basic::Ripple = basic::Ripple;
pub static CMYK: basic::Cmyk = basic::Cmyk;
pub static SNOW: snow::Snow = snow::Snow;
pub static FLAME: flame::Flame = flame::Flame;
pub static FLAG: flag::Flag = flag::Flag;
pub static GLITCH: glitch::Glitch = glitch::Glitch;
pub static DATAMOSH: datamosh::Datamosh = datamosh::Datamosh;
pub static MELTDOWN: meltdown::Meltdown = meltdown::Meltdown;
pub static ELECTRIC: electric::Electric = electric::Electric;
pub static MATRIX: matrix::Matrix = matrix::Matrix;
pub static BUBBLES: bubbles::Bubbles = bubbles::Bubbles;
pub static STARFIELD: starfield::Starfield = starfield::Starfield;
pub static SAKURA: sakura::Sakura = sakura::Sakura;
pub static RAVE: rave::Rave = rave::Rave;
