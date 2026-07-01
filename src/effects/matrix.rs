//! Matrix digital rain: columns fall continuously, clicks punch a short cascade,
//! and a scroll spawns an extra wave. Green flicker border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Matrix;

impl ThemeEffect for Matrix {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Density,
            Knob::Speed,
            Knob::Persistence,
            Knob::BeatSync,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            speed: 0.5,
            persistence: 0.5,
            beat_sync: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let n = noise(frame as u32 / 2, (offset * 90.0) as u32) % 100;
        let b = (0.5 + 0.5 * (n as f64 / 100.0) + beat as f64 * 0.35).min(1.0);
        scale(base, b)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        let frame = ctx.frame as u32;
        for i in 0..14u32 {
            let seed = noise(frame + i, col as u32 * 17);
            sim.push(Spark {
                x: col as f32 + ((seed % 5) as f32 - 2.0),
                y: row as f32,
                vx: 0.0,
                vy: 0.45 + (seed % 3) as f32 * 0.15,
                age: 0,
                life: 90,
                seed,
            });
        }
        sim.cap();
    }

    fn on_scroll(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        let w = (s.width as u32).max(1);
        for i in 0..12u32 {
            let seed = noise(ctx.frame as u32 + i, 0xC0DE);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32,
                vx: 0.0,
                vy: 0.6 + (seed % 3) as f32 * 0.2,
                age: 0,
                life: 150,
                seed,
            });
        }
        sim.cap();
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let (f, w) = (ctx.frame as u32, s.width as u32);
        // Density = columns/frame; speed = fall rate; persistence = trail length.
        let cols = 1 + (ctx.tuning.density * 5.0) as u32;
        let vy = |seed: u32| 0.3 + ctx.tuning.speed * 0.6 + (seed % 3) as f32 * 0.15;
        let life = (90.0 + ctx.tuning.persistence * 160.0) as u16;
        for i in 0..cols {
            let seed = noise(f + i, 0x4A7);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32,
                vx: 0.0,
                vy: vy(seed),
                age: 0,
                life,
                seed,
            });
        }
        // Bass beat: a wave of new columns rains down at once.
        let bass = ctx.beat_bands[0] * ctx.tuning.beat_sync;
        if bass > 0.25 {
            let wave = (bass * 20.0) as u32;
            for i in 0..wave {
                let seed = noise(f.wrapping_add(i * 61), 0x4A8);
                sim.push(Spark {
                    x: (s.x as u32 + seed % w) as f32,
                    y: s.y as f32,
                    vx: 0.0,
                    vy: vy(seed) + bass * 0.4,
                    age: 0,
                    life,
                    seed,
                });
            }
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, matrix_glyph);
    }
}

fn matrix_glyph(t: f64, seed: u32) -> (char, Color) {
    const G: [char; 12] = ['0', '1', 'ﾊ', 'ｱ', 'ﾝ', 'ｦ', 'ﾘ', 'ｷ', 'ﾂ', '7', ':', '*'];
    let ch = G[seed as usize % G.len()];
    let col = if t < 0.12 {
        Color::Rgb(0xd6, 0xff, 0xd6) // bright head
    } else if t < 0.5 {
        Color::Rgb(0x00, 0xff, 0x41)
    } else {
        let f = 1.0 - (t - 0.5);
        Color::Rgb(0, (0xc0 as f64 * f) as u8, (0x30 as f64 * f) as u8)
    };
    (ch, col)
}
