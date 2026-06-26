//! Warp starfield from screen center; navigation triggers a warp burst, a click
//! a supernova. Static border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect, render_sparks};
use crate::particles::ParticleSim;

pub struct Starfield;

impl ThemeEffect for Starfield {
    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, _dir: f32) {
        sim.warp(ctx.frame, ctx.screen, 36);
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 50, 1.3, 18); // supernova
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        sim.warp(ctx.frame, ctx.screen, 2);
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, star_glyph);
    }
}

fn star_glyph(t: f64, seed: u32) -> (char, Color) {
    // Older (farther from center) = bigger + brighter -> warp streak feel.
    if t < 0.25 {
        ('·', Color::Rgb(0x9a, 0x96, 0xd0))
    } else if t < 0.55 {
        ('+', Color::Rgb(0xc8, 0xc2, 0xff))
    } else {
        let g = ['✦', '✶', '✷'][seed as usize % 3];
        (g, Color::Rgb(0xff, 0xff, 0xff))
    }
}
