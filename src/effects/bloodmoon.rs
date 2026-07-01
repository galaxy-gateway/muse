//! Blood moon: a gothic crimson night. A dim red moon hangs and pulses; blood
//! drips run down and bats flit across; and the border **throbs like a heartbeat**
//! locked to the beat. Drips/bats paint over the UI sparsely; the moon fills only
//! empty cells.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const BLOOD: Color = Color::Rgb(0xa4, 0x14, 0x22);
const CRIMSON: Color = Color::Rgb(0xc4, 0x1e, 0x3a);
const DARK: Color = Color::Rgb(0x5a, 0x0e, 0x18);

pub struct Bloodmoon;

impl ThemeEffect for Bloodmoon {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Density, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            density: 0.4,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Heartbeat: a slow idle throb plus a hard flare on each beat.
        let idle = (frame as f64 * 0.05 + offset).sin() * 0.15 + 0.35;
        scale(CRIMSON, (idle + beat as f64 * 0.6).min(1.0))
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 18, 0.6, 20);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let (f, w) = (ctx.frame as u32, s.width as u32);
        let beat = ctx.beat * ctx.tuning.beat_sync;
        // Blood drips fall from the top; count from density + beat.
        let drips = 1 + (ctx.tuning.density * 3.0) as u32 + (beat * 6.0) as u32;
        for i in 0..drips {
            let seed = noise(f + i * 41, 0xB100D);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: s.y as f32,
                vx: 0.0,
                vy: 0.2 + (seed % 3) as f32 * 0.08,
                age: 0,
                life: 120,
                seed: seed | 1, // odd = drip
            });
        }
        // An occasional bat drifts across.
        if f % 20 == 0 {
            let seed = noise(f, 0xBA7) & !1; // even = bat
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: (s.y as u32 + seed / 7 % (s.height as u32 / 2).max(1)) as f32,
                vx: ((seed % 5) as f32 - 2.0) * 0.15,
                vy: -0.03 + (seed % 3) as f32 * 0.02,
                age: 0,
                life: 120,
                seed,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as i32, area.height as i32);
        if w < 8 || h < 8 {
            return;
        }
        render_sparks(f, sim, area, ctx.frame as u32, moon_glyph);

        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let buf = f.buffer_mut();
        // The moon: a crimson disc upper-right, mottled with darker craters,
        // brightening as it throbs. Empty cells only.
        let cx = area.x as i32 + w * 3 / 4;
        let cy = area.y as i32 + h / 4;
        let rr = (h as f32 * 0.18).max(4.0);
        for dy in -(rr as i32)..=(rr as i32) {
            for dx in -(rr as i32 * 2)..=(rr as i32 * 2) {
                let (nx, ny) = (dx as f32 / (rr * 2.0), dy as f32 / rr);
                if nx * nx + ny * ny > 1.0 {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x < area.x as i32 || x >= area.right() as i32 || y < area.y as i32 {
                    continue;
                }
                let cell = &mut buf[(x as u16, y as u16)];
                if cell.symbol() != " " {
                    continue;
                }
                let crater = noise((x as u32) ^ (y as u32).wrapping_mul(31), 0).is_multiple_of(5);
                let col = if crater { DARK } else { BLOOD };
                cell.set_char(if crater { '▒' } else { '█' });
                cell.set_fg(scale(col, (0.55 + beat as f64 * 0.45).min(1.0)));
            }
        }
    }
}

/// Odd seed = a falling blood drip; even seed = a bat silhouette.
fn moon_glyph(t: f64, seed: u32) -> (char, Color) {
    if seed & 1 == 1 {
        let ch = if t < 0.5 { '╿' } else { '!' };
        (ch, if t < 0.6 { CRIMSON } else { DARK })
    } else {
        ('^', Color::Rgb(0x2e, 0x22, 0x26)) // a distant bat silhouette
    }
}
