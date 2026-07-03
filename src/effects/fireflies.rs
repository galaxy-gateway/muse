//! Fireflies at dusk: a sparse population of slow-drifting lights that pulse
//! on their own rhythms, flare together on treble onsets (hi-hats), and — when
//! follow-mouse is on — drift gently toward the pointer like it's a lantern.
//! `ambient` steers the existing particles every tick (wander + attraction +
//! edge avoidance), which is what turns the sim's linear motion into meander.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const GLOW: Color = Color::Rgb(0xd8, 0xf0, 0x6a);
const FLARE: Color = Color::Rgb(0xff, 0xff, 0xc0);
const EMBER: Color = Color::Rgb(0x8a, 0xa0, 0x3a);

pub struct Fireflies;

impl ThemeEffect for Fireflies {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Density,
            Knob::Speed,
            Knob::BeatSync,
            Knob::BeatSource,
            Knob::FollowMouse,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            speed: 0.35,
            beat_sync: 0.6,
            beat_source: 1.0, // treble sparkles the field
            follow_mouse: 1.0,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Deep-dusk borders that shimmer faintly when the treble sparkles.
        // Stepped + quantized to avoid per-frame border rewrites.
        let t = (frame / 6) as f64 * 0.06 + offset * 0.9;
        let dusk = 0.78 + 0.08 * (t * std::f64::consts::TAU).sin();
        let beat = crate::color::quant(beat as f64, 4.0);
        crate::color::qscale(base, (dusk + beat * 0.22).min(1.05))
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 6 {
            return;
        }
        let target = 6 + (ctx.tuning.density * 26.0) as usize;
        let max_v = 0.06 + ctx.tuning.speed * 0.22;

        // Steer the living: random wander, lantern attraction, edge avoidance.
        let lantern = ctx
            .tuning
            .is_on(Knob::FollowMouse)
            .then_some(ctx.hover)
            .flatten();
        for (i, p) in sim.sparks.iter_mut().enumerate() {
            let s = noise(ctx.frame as u32 / 3, i as u32 * 71 + p.seed);
            p.vx += ((s % 7) as f32 - 3.0) * 0.008;
            p.vy += ((s / 7 % 7) as f32 - 3.0) * 0.006;
            if let Some((hx, hy)) = lantern {
                p.vx += (hx as f32 - p.x).signum() * 0.010;
                p.vy += (hy as f32 - p.y).signum() * 0.008;
            }
            // Shy of the edges: steer back inside rather than flying off.
            if p.x < (area.x + 2) as f32 {
                p.vx += 0.03;
            }
            if p.x > (area.right() - 3) as f32 {
                p.vx -= 0.03;
            }
            if p.y < (area.y + 2) as f32 {
                p.vy += 0.03;
            }
            if p.y > (area.bottom() - 3) as f32 {
                p.vy -= 0.03;
            }
            p.vx = p.vx.clamp(-max_v, max_v);
            p.vy = p.vy.clamp(-max_v * 0.7, max_v * 0.7);
        }

        // Keep the population topped up; newcomers fade in from anywhere.
        if sim.sparks.len() < target && ctx.frame.is_multiple_of(4) {
            let s = noise(ctx.frame as u32, sim.sparks.len() as u32 * 131);
            sim.push(Spark {
                x: (area.x + 2 + (s % (area.width as u32 - 4)) as u16) as f32,
                y: (area.y + 2 + (s / 7 % (area.height as u32 - 4)) as u16) as f32,
                vx: 0.0,
                vy: 0.0,
                age: 0,
                life: 400 + (s % 400) as u16,
                seed: s,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 6 {
            return;
        }
        // Treble makes the whole field flare together for a few frames
        // (quantized so the decay doesn't re-color every fly every frame).
        let treble = crate::color::quant(
            (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64,
            4.0,
        );
        let buf = f.buffer_mut();
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= area.bottom() as i32
            {
                continue;
            }
            // Each fly blinks on its own slow cycle (stepped ~10Hz); treble
            // lifts everyone.
            let t = ((ctx.frame / 6) as f64 + (p.seed % 97) as f64 * 0.62) * 0.3;
            let own = 0.5 + 0.5 * (t * std::f64::consts::TAU).sin();
            let lum = crate::color::quant((own * 0.75 + treble * 0.6).min(1.0), 10.0);
            // Fade in on spawn and out near end of life.
            let age_f = p.age as f64 / p.life.max(1) as f64;
            let envelope = (age_f * 12.0).min(1.0) * ((1.0 - age_f) * 8.0).min(1.0);
            let lum = lum * envelope;
            if lum < 0.08 {
                continue;
            }
            let cell = &mut buf[(x as u16, y as u16)];
            let (ch, col) = if lum > 0.75 {
                ('✦', mix(GLOW, FLARE, treble))
            } else if lum > 0.4 {
                ('•', GLOW)
            } else {
                ('·', EMBER)
            };
            cell.set_char(ch);
            cell.set_fg(scale(col, 0.35 + lum * 0.65));
        }
    }
}
