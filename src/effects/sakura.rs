//! Drifting cherry-blossom petals; the mouse blows them like wind. Static border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Sakura;

impl ThemeEffect for Sakura {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Density,
            Knob::Speed,
            Knob::Wind,
            Knob::BeatSync,
            Knob::FollowMouse,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.4,
            speed: 0.3,
            wind: 0.3,
            beat_sync: 0.3,
            follow_mouse: 1.0, // mouse blows the petals by default
            ..Default::default()
        }
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 16, 0.4, 46);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let w = s.width as u32;
        let (density, speed, wind) = (ctx.tuning.density, ctx.tuning.speed, ctx.tuning.wind);
        // Petal spawn: at least occasionally, more with `density`.
        let count = 1 + (density * 3.0) as u32;
        let drift = 0.02 + wind * 0.12;
        let fall = |seed: u32| 0.1 + speed * 0.3 + (seed % 3) as f32 * 0.04;
        for i in 0..count {
            let seed = noise(ctx.frame as u32 + i * 71, 0x5A6);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32,
                vx: ((seed % 7) as f32 - 3.0) * drift,
                vy: fall(seed),
                age: 0,
                life: 200,
                seed,
            });
        }
        // Beat: a downward gust — a row of petals with extra fall speed.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        if beat > 0.3 {
            let gust = (beat * 10.0) as u32;
            for i in 0..gust {
                let seed = noise(ctx.frame as u32 ^ (i * 131), 0x5A7);
                sim.push(Spark {
                    x: (s.x as u32 + seed % w) as f32,
                    y: s.y as f32,
                    vx: ((seed % 7) as f32 - 3.0) * drift,
                    vy: fall(seed) + beat * 0.5,
                    age: 0,
                    life: 160,
                    seed,
                });
            }
        }
        sim.cap();
    }

    /// The mouse blows petals like a gust of wind toward the pointer column —
    /// only when the `FollowMouse` toggle is on.
    fn wind(&self, ctx: &FrameCtx) -> Option<f32> {
        if ctx.tuning.is_on(Knob::FollowMouse) {
            ctx.hover.map(|(hx, _)| hx as f32)
        } else {
            None
        }
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
