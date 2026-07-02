//! Tide: a water band along the bottom of the screen. Two traveling sine
//! components give the surface a natural cross-chop; bass energy swells the
//! whole band upward. Text below the waterline keeps its glyphs but is tinted
//! toward deep blue (readable underwater); the surface itself is drawn as
//! bright chop. Beat crests throw spray particles that the wind hook drifts
//! sideways as they fall out of the air.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const DEEP: (f64, f64, f64) = (30.0, 90.0, 140.0);
const SURFACE: Color = Color::Rgb(0x7a, 0xd8, 0xe8);
const FOAM: Color = Color::Rgb(0xe8, 0xff, 0xff);

pub struct Tide;

/// Water surface height (rows above the band floor) for column `x`.
fn surface(x: u16, frame: u64, swell: f64, intensity: f64) -> f64 {
    let t = frame as f64 * 0.05;
    let xf = x as f64;
    let chop = (xf * 0.23 - t).sin() + 0.6 * (xf * 0.11 + t * 0.7).sin();
    1.5 + swell + chop * (0.8 + intensity * 1.6)
}

impl ThemeEffect for Tide {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Intensity,
            Knob::Density,
            Knob::Wind,
            Knob::BeatSync,
            Knob::BeatSource,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.5,
            density: 0.5,
            wind: 0.4,
            beat_sync: 0.6,
            beat_source: 1.0 / 3.0, // bass swells the water
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = frame as f64 * 0.015 + offset * 1.3;
        let lap = 0.84 + 0.10 * (t * std::f64::consts::TAU).sin();
        scale(base, (lap + beat as f64 * 0.2).min(1.1))
    }

    fn wind(&self, ctx: &FrameCtx) -> Option<f32> {
        // A slowly shifting onshore/offshore breeze for the spray.
        let t = ctx.frame as f64 * 0.008;
        Some((t.sin() * 1.6 * ctx.tuning.wind as f64) as f32)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 10 {
            return;
        }
        let bass = (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        // Gentle ambient spume plus a burst on a bass crest.
        let base_rate = if ctx.frame.is_multiple_of(6) { 1 } else { 0 };
        let n = base_rate + (bass * 5.0 * ctx.tuning.density) as u32;
        if n == 0 {
            return;
        }
        let floor = ctx.transport_rect.y;
        let swell = bass as f64 * 3.0;
        for i in 0..n {
            let s = noise(ctx.frame as u32, i * 97 + 13);
            let x = area.x + (s % area.width as u32) as u16;
            let h = surface(x, ctx.frame, swell, ctx.tuning.intensity as f64);
            // Launch only from genuinely high columns (crests).
            if h < 2.2 && bass < 0.4 {
                continue;
            }
            sim.push(Spark {
                x: x as f32,
                y: (floor as f64 - h - 1.0).max(area.y as f64 + 1.0) as f32,
                vx: ((s % 5) as f32 - 2.0) * 0.15,
                vy: -(0.25 + (s % 4) as f32 * 0.15) - bass * 0.4,
                age: 0,
                life: 8 + (s % 8) as u16,
                seed: s,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 10 {
            return;
        }
        let bass = (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64;
        let swell = bass * 3.0;
        let intensity = ctx.tuning.intensity as f64;
        let floor = ctx.transport_rect.y; // water sits above the transport bar
        let buf = f.buffer_mut();

        for x in area.x..area.right() {
            let h = surface(x, ctx.frame, swell, intensity).max(0.5);
            let top = (floor as f64 - h).max(area.y as f64 + 1.0) as u16;
            // Surface chop: bright glyph riding the waterline.
            let cell = &mut buf[(x, top)];
            let s = noise(x as u32, ctx.frame as u32 / 4);
            // Single-width glyphs only: wide chars here would drift columns.
            cell.set_char(match s % 4 {
                0 => '~',
                1 => '≈',
                2 => '∿',
                _ => '~',
            });
            cell.set_fg(mix(SURFACE, FOAM, bass * 0.7));
            // Below the waterline: tint existing glyphs toward deep blue, and
            // texture blank cells lightly so the body of water reads as water.
            for y in (top + 1)..floor {
                let depth = (y - top) as f64 / h.max(1.0);
                let k = 0.35 + depth * 0.4;
                let cell = &mut buf[(x, y)];
                if cell.symbol() == " " {
                    if noise(x as u32 * 31, y as u32 * 17).is_multiple_of(11) {
                        cell.set_char('·');
                        cell.set_fg(Color::Rgb(0x3a, 0x74, 0xa0));
                    }
                } else {
                    let (r, g, b) = crate::color::rgb_of(cell.fg);
                    cell.set_fg(Color::Rgb(
                        (r + (DEEP.0 - r) * k) as u8,
                        (g + (DEEP.1 - g) * k) as u8,
                        (b + (DEEP.2 - b) * k) as u8,
                    ));
                }
            }
        }

        // Spray: bright droplets fading as they fly.
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= floor as i32
            {
                continue;
            }
            let fade = 1.0 - p.age as f64 / p.life.max(1) as f64;
            let cell = &mut buf[(x as u16, y as u16)];
            cell.set_char(if p.age < 3 { '∘' } else { '·' });
            cell.set_fg(scale(FOAM, 0.4 + fade * 0.6));
        }
    }
}
