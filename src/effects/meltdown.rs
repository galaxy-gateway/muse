//! Meltdown: datamosh done properly — 2D macroblock smear with P-frame
//! persistence. Where `datamosh` rips whole rows and snaps clean, meltdown
//! corrupts in *rectangular blocks* that drag content across the screen along a
//! motion vector, and — crucially — the corruption *persists*: a ghost buffer
//! holds each smear and fades it over ~15 frames, so the picture melts on a beat
//! and slowly heals between hits, exactly like a video codec losing its
//! reference frames.
//!
//! Frequency-split: bass drives big, slow-drifting blocks; treble drives fine
//! skittering breakup. Sustained energy blooms the whole picture brighter.
//!
//! State lives in a module-local `Mutex<Ghost>` so the effect stays a plain
//! `&self` static — no trait/App changes needed.

use std::sync::Mutex;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

const GREEN: Color = Color::Rgb(0x33, 0xff, 0x66);
const CYAN: Color = Color::Rgb(0x00, 0xff, 0xff);
const MAGENTA: Color = Color::Rgb(0xff, 0x00, 0xcc);

const GATE: f32 = 0.15;

/// A decaying grid of corrupted cells laid over the finished frame. Each active
/// cell holds a smeared glyph + color and a `life` (1.0 fresh → 0 healed).
struct Ghost {
    w: u16,
    h: u16,
    sym: Vec<String>,
    fg: Vec<Color>,
    life: Vec<f32>,
    /// True while any cell is still alive — lets the overlay bail cheaply once
    /// fully healed and silent.
    alive: bool,
}

impl Ghost {
    const fn new() -> Self {
        Self {
            w: 0,
            h: 0,
            sym: Vec::new(),
            fg: Vec::new(),
            life: Vec::new(),
            alive: false,
        }
    }

    /// (Re)allocate + clear when the screen size changes.
    fn ensure(&mut self, area: Rect) {
        if self.w != area.width || self.h != area.height {
            self.w = area.width;
            self.h = area.height;
            let n = area.width as usize * area.height as usize;
            self.sym = vec![String::new(); n];
            self.fg = vec![GREEN; n];
            self.life = vec![0.0; n];
            self.alive = false;
        }
    }

    /// Fade every cell one frame; recompute `alive`.
    fn decay(&mut self, k: f32) {
        let mut any = false;
        for l in &mut self.life {
            if *l > 0.0 {
                *l *= k;
                if *l < 0.03 {
                    *l = 0.0;
                } else {
                    any = true;
                }
            }
        }
        self.alive = any;
    }

    /// Stamp a fresh corrupted cell (relative coords).
    fn stamp(&mut self, rx: u16, ry: u16, sym: String, fg: Color) {
        if rx < self.w && ry < self.h {
            let i = ry as usize * self.w as usize + rx as usize;
            self.sym[i] = sym;
            self.fg[i] = fg;
            self.life[i] = 1.0;
            self.alive = true;
        }
    }

    /// Paint every live cell over `buf`, dimmed by its remaining life and lifted
    /// by `bloom` (sustained loudness).
    fn render(&self, buf: &mut Buffer, area: Rect, bloom: f32) {
        for ry in 0..self.h {
            for rx in 0..self.w {
                let i = ry as usize * self.w as usize + rx as usize;
                let l = self.life[i];
                if l <= 0.0 {
                    continue;
                }
                let cell = &mut buf[(area.x + rx, area.y + ry)];
                cell.set_symbol(&self.sym[i]);
                cell.set_fg(scale(self.fg[i], (l * bloom).min(1.0) as f64));
            }
        }
    }
}

static GHOST: Mutex<Ghost> = Mutex::new(Ghost::new());

pub struct Meltdown;

impl ThemeEffect for Meltdown {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Persistence, Knob::Disruption]
    }

    fn default_tuning(&self) -> Tuning {
        // Toned down by default so the app stays usable — the user can crank
        // persistence/disruption up for full meltdown chaos.
        Tuning {
            intensity: 0.5,
            persistence: 0.35,
            disruption: 0.25,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, _beat: f32) -> Color {
        let n = noise(frame as u32 / 9, (offset * 200.0) as u32) % 100;
        if n < 3 {
            CYAN
        } else if n < 6 {
            MAGENTA
        } else {
            scale(base, 0.7 + 0.3 * ((n % 40) as f64 / 40.0))
        }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width, area.height);
        if w < 4 || h < 4 {
            return;
        }

        let [bass, mid, treble] = ctx.beat_bands.map(|v| v.clamp(0.0, 1.0));
        let hot = bass >= GATE || mid >= GATE || treble >= GATE;

        // Tuning: `persistence` sets trail length, `disruption` sets how big/far
        // the smears drag the real UI, `intensity` sets how many fire.
        let intensity = ctx.tuning.intensity;
        let disruption = ctx.tuning.disruption;
        // Fade per frame: 0.78 (short blink) .. 0.98 (long, gooey trails).
        let fade = 0.78 + ctx.tuning.persistence * 0.20;

        let mut g = GHOST.lock().unwrap();
        g.ensure(area);
        // Nothing to draw and nothing to heal: bail before touching the buffer.
        if !hot && !g.alive {
            return;
        }
        g.decay(fade);

        let frame = ctx.frame as u32;
        let buf = f.buffer_mut();

        // --- Macroblock smear: on a beat, drag rectangular blocks of the real
        // UI across the screen along a motion vector and bake them into the
        // ghost, where they persist and fade. Bass = big slow blocks; mid adds
        // more, medium. ---
        let drive = bass.max(mid);
        if drive >= GATE && intensity > 0.05 {
            let hit = ((drive - GATE) / (1.0 - GATE)).clamp(0.0, 1.0);
            let blocks = 1 + (hit * 7.0 * intensity) as u32;
            for k in 0..blocks {
                let s = noise(frame ^ k.wrapping_mul(2_654_435_761), k * 41 + 3);
                // Block size scales with disruption: small local smears stay
                // readable; big ones swallow the screen.
                let bw = (1 + (bass * 12.0 * disruption) as u32 + (s % 3)) as u16;
                let bh = (1 + (bass * 4.0 * disruption) as u32 + (s / 3 % 2)) as u16;
                let x0 = (s % w.saturating_sub(bw).max(1) as u32) as u16;
                let y0 = (s / 7 % h.saturating_sub(bh).max(1) as u32) as u16;
                // Motion vector: mostly horizontal drag, scaled by disruption.
                let sign = if s & 1 == 0 { 1i32 } else { -1 };
                let mvx = sign * (1 + (hit * 16.0 * disruption) as i32);
                let mvy = ((s / 11 % 3) as i32 - 1) * (bass > 0.6 && disruption > 0.5) as i32;
                // Occasionally force a chroma channel; else keep the source color
                // so dragged text stays recognizable.
                let tint = match s % 5 {
                    0 => Some(CYAN),
                    1 => Some(MAGENTA),
                    _ => None,
                };
                smear_block(&mut g, buf, area, x0, y0, bw, bh, mvx, mvy, tint);
            }
        }

        // --- Treble skitter: many tiny 1-2 cell blocks with short random jumps,
        // for fine high-frequency breakup. ---
        if treble >= GATE && intensity > 0.05 {
            let hit = ((treble - GATE) / (1.0 - GATE)).clamp(0.0, 1.0);
            let bits = (hit * 40.0 * intensity) as u32;
            for k in 0..bits {
                let s = noise(
                    frame.wrapping_add(k.wrapping_mul(2_246_822_519)),
                    k * 17 + 91,
                );
                let x0 = (s % w as u32) as u16;
                let y0 = (s / 3 % h as u32) as u16;
                let mvx = (s / 5 % 5) as i32 - 2;
                let mvy = (s / 29 % 3) as i32 - 1;
                let tint = if s & 1 == 0 { Some(CYAN) } else { None };
                smear_block(&mut g, buf, area, x0, y0, 1, 1, mvx, mvy, tint);
            }
        }

        // Bloom: sustained energy lifts the whole picture a touch brighter.
        let bloom = 1.0 + bass.max(mid).max(treble) * 0.4;
        g.render(buf, area, bloom);
    }
}

/// Copy a `bw`x`bh` block of the current frame at `(x0,y0)` offset by the motion
/// vector `(mvx,mvy)` into the ghost, so the block appears dragged from its
/// source. Reads source cells from `buf` (owned copies) before stamping `g`.
#[allow(clippy::too_many_arguments)]
fn smear_block(
    g: &mut Ghost,
    buf: &Buffer,
    area: Rect,
    x0: u16,
    y0: u16,
    bw: u16,
    bh: u16,
    mvx: i32,
    mvy: i32,
    tint: Option<Color>,
) {
    for dy in 0..bh {
        for dx in 0..bw {
            let dstx = x0 + dx;
            let dsty = y0 + dy;
            if dstx >= area.width || dsty >= area.height {
                continue;
            }
            // Source = destination pulled back along the motion vector.
            let srcx = dstx as i32 - mvx;
            let srcy = dsty as i32 - mvy;
            if srcx < 0 || srcy < 0 || srcx >= area.width as i32 || srcy >= area.height as i32 {
                continue;
            }
            let src = &buf[(area.x + srcx as u16, area.y + srcy as u16)];
            let sym = src.symbol().to_string();
            let fg = tint.unwrap_or(src.fg);
            g.stamp(dstx, dsty, sym, fg);
        }
    }
}
