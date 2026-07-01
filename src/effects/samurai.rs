//! Samurai: an ink-and-steel theme. Cherry-blossom petals drift down ambiently;
//! strong beats **unsheathe katana slashes** — bright steel diagonals with white
//! edges; the border is a crimson-pulsing ink brush; and a single gold **kanji
//! seal** (侍→道→武→魂) shimmers in the bottom-right corner. Clicks land a slash
//! impact.
//!
//! Note: full-width CJK glyphs occupy two terminal cells, so they are NOT rained
//! across the grid (that corrupts the layout — see the matrix theme's use of
//! half-width kana). The only kanji is the fixed seal, drawn width-safely.
//!
//! Knobs: intensity (slash count) · density (petal fall) · speed (fall rate) ·
//! beat-sync (reaction). beat-sync 0 = a calm petal garden, no slashes.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

/// Kanji cycled through by the corner seal (the only full-width glyph on screen).
const SEAL: [char; 4] = ['侍', '道', '武', '魂'];

const STEEL: Color = Color::Rgb(0xc2, 0xcc, 0xd6);
const WHITE: Color = Color::Rgb(0xff, 0xff, 0xff);
const CRIMSON: Color = Color::Rgb(0xc4, 0x1e, 0x3a);
const GOLD: Color = Color::Rgb(0xd4, 0xaf, 0x37);
const BLOSSOM: Color = Color::Rgb(0xff, 0x9e, 0xcf);

pub struct Samurai;

impl ThemeEffect for Samurai {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Density, Knob::Speed, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            density: 0.4,
            speed: 0.4,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Ink brush: dim steel with occasional bright strokes, flaring crimson on
        // the beat.
        let n = noise(frame as u32 / 2, (offset * 140.0) as u32) % 100;
        if beat > 0.5 || n < 4 {
            CRIMSON
        } else {
            let b = (0.35 + 0.45 * (n as f64 / 100.0) + beat as f64 * 0.3).min(1.0);
            scale(STEEL, b)
        }
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 20, 0.8, 20);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let w = s.width as u32;
        let speed = ctx.tuning.speed;
        let vy = |seed: u32| 0.1 + speed * 0.35 + (seed % 3) as f32 * 0.04;
        // Sparse: petals drift, count from density. Spawn on alternate frames so
        // it never becomes a curtain.
        let count = (ctx.tuning.density * 3.0) as u32 + (ctx.frame % 2) as u32;
        for i in 0..count {
            let seed = noise(ctx.frame as u32 + i * 71, 0x5A31);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32,
                vx: ((seed % 7) as f32 - 3.0) * 0.05,
                vy: vy(seed),
                age: 0,
                life: 200,
                seed,
            });
        }
        // Beat gust: a puff of petals on the hit.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        if beat > 0.35 {
            let gust = (beat * 6.0) as u32;
            for i in 0..gust {
                let seed = noise(ctx.frame as u32 ^ (i * 131), 0x5A32);
                sim.push(Spark {
                    x: (s.x as u32 + seed % w) as f32,
                    y: s.y as f32,
                    vx: ((seed % 7) as f32 - 3.0) * 0.06,
                    vy: vy(seed) + beat * 0.3,
                    age: 0,
                    life: 160,
                    seed,
                });
            }
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        render_sparks(f, sim, area, ctx.frame as u32, petal_glyph);

        let (w, h) = (area.width as u32, area.height as u32);
        if w < 4 || h < 4 {
            return;
        }
        let frame = ctx.frame as u32;
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let intensity = ctx.tuning.intensity;
        let buf = f.buffer_mut();

        // --- Katana slashes: only on a strong beat, a few decisive diagonal
        // strikes (not a constant flurry). Box-drawing glyphs are width-1. ---
        if beat > 0.45 {
            let strikes = 1 + (beat * intensity * 3.0) as u32;
            for sidx in 0..strikes {
                let seed = noise(frame / 3 + sidx * 911, 0xB1AD);
                let dir: i32 = if seed & 1 == 0 { 1 } else { -1 };
                let ch = if dir > 0 { '╲' } else { '╱' };
                let len = (h / 3 + seed % (h / 2).max(1)) as i32;
                let base_x = area.x as i32 + (seed / 3 % w) as i32;
                let y0 = area.y as i32 + (seed / 7 % h.saturating_sub(len as u32).max(1)) as i32;
                for i in 0..len {
                    let cx = base_x + dir * i;
                    let cy = y0 + i;
                    if cx < area.x as i32 || cx >= area.right() as i32 || cy >= area.bottom() as i32
                    {
                        continue;
                    }
                    let col = if i < 3 || noise(seed + i as u32, 0).is_multiple_of(6) {
                        WHITE
                    } else {
                        STEEL
                    };
                    let cell = &mut buf[(cx as u16, cy as u16)];
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        }

        // --- Kanji seal: one shimmering gold stamp in the bottom-right, drawn
        // width-safely (the trailing cell is blanked for the double-width glyph). ---
        if area.width > 4 && area.height > 2 {
            let ch = SEAL[(frame / 40) as usize % SEAL.len()];
            let shimmer = 0.6 + 0.4 * ((frame as f64 * 0.12).sin() * 0.5 + 0.5);
            put_wide(
                buf,
                area.right() - 3,
                area.bottom() - 2,
                ch,
                scale(GOLD, shimmer),
            );
        }
    }
}

/// Write a double-width glyph at `(x,y)` and blank its continuation cell, so the
/// terminal grid stays aligned (ratatui renders a wide symbol then skips the next
/// cell — which must be emptied, not left with stale content).
fn put_wide(buf: &mut Buffer, x: u16, y: u16, ch: char, col: Color) {
    let a = &mut buf[(x, y)];
    a.set_char(ch);
    a.set_fg(col);
    buf[(x + 1, y)].set_symbol("");
}

/// Cherry-blossom petal, soft pink fading as it ages. All width-1 glyphs.
fn petal_glyph(t: f64, seed: u32) -> (char, Color) {
    let g = ['❀', '✿', '❁', '❃'][seed as usize % 4];
    let col = if t < 0.5 {
        BLOSSOM
    } else {
        Color::Rgb(0xd8, 0x88, 0xb0)
    };
    (g, col)
}
