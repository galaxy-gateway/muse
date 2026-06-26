//! Rising bubbles; clicking pops a cluster upward. Gentle glow border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect, render_sparks};
use crate::color::glow;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Bubbles;

impl ThemeEffect for Bubbles {
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        glow(base, frame, offset)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        let frame = ctx.frame as u32;
        for i in 0..18u32 {
            let seed = noise(frame + i, col as u32 * 131);
            sim.push(Spark {
                x: col as f32 + ((seed % 5) as f32 - 2.0),
                y: row as f32,
                vx: ((seed % 7) as f32 - 3.0) * 0.12,
                vy: -(0.3 + (seed % 4) as f32 * 0.12),
                age: 0,
                life: 60,
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
        for i in 0..2u32 {
            let seed = noise(f + i, 0xB0B);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: (s.y + s.height - 1) as f32,
                vx: ((seed % 5) as f32 - 2.0) * 0.05,
                vy: -(0.25 + (seed % 4) as f32 * 0.1),
                age: 0,
                life: 150,
                seed,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, bubble_glyph);
    }
}

fn bubble_glyph(t: f64, seed: u32) -> (char, Color) {
    const G: [char; 6] = ['○', '◯', '◌', '∘', '°', '·'];
    let ch = G[seed as usize % G.len()];
    let col = if t < 0.5 {
        Color::Rgb(0x9a, 0xf0, 0xff)
    } else {
        Color::Rgb(0x46, 0xc0, 0xe0)
    };
    (ch, col)
}
