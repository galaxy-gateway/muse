//! Constellations: a quiet star field that twinkles, and on each beat sends a
//! **shooting star** streaking across the sky. Stars sit at fixed seeded
//! positions (analytic twinkle); the streaks are short-lived fast particles.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::{mix, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const STAR: Color = Color::Rgb(0xcf, 0xd6, 0xff);
const BLUE: Color = Color::Rgb(0x8a, 0xa6, 0xff);
const BRIGHT: Color = Color::Rgb(0xff, 0xff, 0xff);

pub struct Constellations;

impl ThemeEffect for Constellations {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let n = noise(frame as u32 / 3, (offset * 80.0) as u32) % 100;
        scale(
            base,
            (0.5 + 0.3 * (n as f64 / 100.0) + beat as f64 * 0.4).min(1.0),
        )
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        // A rare, single shooting star on a strong beat — a `frame % 12` stride
        // ensures at most one per beat (the pulse lasts a few frames), so the sky
        // stays calm instead of swarming.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        let frame = ctx.frame as u32;
        if beat > 0.55 && frame % 12 == 0 {
            let s = ctx.screen;
            let w = (s.width as u32).max(1);
            let seed = noise(frame, 0x5407);
            let right = seed & 1 == 0;
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32 + (seed / 7 % 3) as f32,
                vx: if right { 1.3 } else { -1.3 },
                vy: 0.6 + (seed % 3) as f32 * 0.1,
                age: 0,
                life: 22 + (seed % 6) as u16,
                seed,
            });
            sim.cap();
        }
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 8 || h < 8 {
            return;
        }
        // Shooting-star streaks (fast particles), drawn first.
        render_sparks(f, sim, area, ctx.frame as u32, streak_glyph);

        let frame = ctx.frame as u32;
        let n_stars = 16 + (ctx.tuning.density * 60.0) as u32;
        let buf = f.buffer_mut();
        for k in 0..n_stars {
            let s = noise(k.wrapping_mul(2_654_435_761), 0x57A2);
            let x = area.x + (s % w) as u16;
            let y = area.y + (s / 7 % h) as u16;
            let tw = ((frame as f32 * 0.1 + k as f32).sin() * 0.5 + 0.5) as f64;
            let (ch, col) = if tw > 0.85 {
                ('✦', BRIGHT)
            } else if tw > 0.5 {
                ('+', STAR)
            } else {
                ('·', scale(STAR, 0.5 + tw * 0.4))
            };
            let cell = &mut buf[(x, y)];
            if cell.symbol() == " " {
                cell.set_char(ch);
                cell.set_fg(col);
            }
        }
    }
}

/// A shooting-star streak: bright white head cooling to blue, angled by heading.
fn streak_glyph(t: f64, seed: u32) -> (char, Color) {
    let ch = if seed & 1 == 0 { '╲' } else { '╱' };
    (ch, mix(BRIGHT, BLUE, t))
}
