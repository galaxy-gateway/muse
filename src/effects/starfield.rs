//! Warp starfield from screen center; navigation triggers a warp burst, a click
//! a supernova. Static border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::particles::ParticleSim;

pub struct Starfield;

impl ThemeEffect for Starfield {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::Speed, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            speed: 0.5,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, _dir: f32) {
        sim.warp(ctx.frame, ctx.screen, 36);
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 50, 1.3, 18); // supernova
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        // Star spawn rate scales with `density`; `speed` adds a steady stream.
        let stream = 1 + (ctx.tuning.density * 4.0 + ctx.tuning.speed * 2.0) as u32;
        sim.warp(ctx.frame, ctx.screen, stream);
        // Beat: bass warps a burst outward; a strong hit goes supernova at center.
        let bass = ctx.beat_bands[0] * ctx.tuning.beat_sync;
        if bass > 0.25 {
            sim.warp(ctx.frame, ctx.screen, (bass * 40.0) as u32);
            if bass > 0.6 {
                let (cx, cy) = (
                    ctx.screen.x + ctx.screen.width / 2,
                    ctx.screen.y + ctx.screen.height / 2,
                );
                sim.burst(ctx.frame, cx, cy, 40, 1.2, 16);
            }
        }
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
