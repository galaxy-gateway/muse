//! RAVE: confetti, strobe bands, fireworks on the beat, mega-explosions on click.
//! Rainbow border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect, render_sparks};
use crate::color::hue;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Rave;

impl ThemeEffect for Rave {
    fn border(&self, _base: Color, frame: u64, offset: f64) -> Color {
        hue(frame as f64 * 0.05 + offset)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 80, 1.6, 22); // mega-explosion
        sim.burst(ctx.frame, col, row, 40, 0.7, 30);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let (f, w) = (ctx.frame as u32, s.width as u32);
        for i in 0..4u32 {
            let seed = noise(f + i, 0xACE);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: (s.y as u32 + (seed / 7) % s.height as u32) as f32,
                vx: ((seed % 9) as f32 - 4.0) * 0.1,
                vy: ((seed / 9 % 9) as f32 - 4.0) * 0.1,
                age: 0,
                life: 28,
                seed,
            });
        }
        // Fireworks on loud beats.
        if ctx.scope_peak > 0.3 && f % 3 == 0 {
            let seed = noise(f, 0xF12E);
            let cx = s.x + (seed % w) as u16;
            let cy = s.y + (seed / 11 % (s.height as u32 / 2).max(1)) as u16;
            sim.burst(ctx.frame, cx, cy, 30, 1.0, 20);
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, confetti_glyph);
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 2 || h < 2 {
            return;
        }
        let frame = ctx.frame as u32;
        let buf = f.buffer_mut();
        // A couple of strobing color bands that jump rows every few frames.
        for b in 0..2u32 {
            let row = noise(frame / 3 + b * 41, b * 7) % h;
            let col = hue((frame as f64 * 0.07) + b as f64 * 0.5);
            let y = area.y + row as u16;
            for c in 0..area.width {
                let cell = &mut buf[(area.x + c, y)];
                cell.set_bg(col);
            }
        }
    }
}

fn confetti_glyph(t: f64, seed: u32) -> (char, Color) {
    const G: [char; 6] = ['▪', '◆', '●', '★', '✦', '▰'];
    let ch = G[seed as usize % G.len()];
    // Vivid rainbow that also shifts as it ages.
    let col = hue((seed % 360) as f64 / 360.0 + t * 0.5);
    (ch, col)
}
