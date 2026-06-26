//! Drifting cherry-blossom petals; the mouse blows them like wind. Static border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect, render_sparks};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Sakura;

impl ThemeEffect for Sakura {
    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 16, 0.4, 46);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let w = s.width as u32;
        let seed = noise(ctx.frame as u32, 0x5A6);
        sim.push(Spark {
            x: (s.x as u32 + seed % w) as f32,
            y: s.y as f32,
            vx: ((seed % 7) as f32 - 3.0) * 0.05,
            vy: 0.18 + (seed % 3) as f32 * 0.05,
            age: 0,
            life: 200,
            seed,
        });
        sim.cap();
    }

    /// The mouse blows petals like a gust of wind toward the pointer column.
    fn wind(&self, ctx: &FrameCtx) -> Option<f32> {
        ctx.hover.map(|(hx, _)| hx as f32)
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, petal_glyph);
    }
}

fn petal_glyph(_t: f64, seed: u32) -> (char, Color) {
    const G: [char; 5] = ['✿', '❀', '✾', '❁', '❃'];
    let ch = G[seed as usize % G.len()];
    let col = match seed % 3 {
        0 => Color::Rgb(0xff, 0x9e, 0xcf),
        1 => Color::Rgb(0xff, 0xc4, 0xe0),
        _ => Color::Rgb(0xe8, 0x7d, 0xb8),
    };
    (ch, col)
}
