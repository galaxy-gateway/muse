//! Seismograph: a red pen traces amplitude onto paper that scrolls right to
//! left across the middle of the screen. The particle sim IS the drum memory —
//! every tick the pen drops one mark at the right edge deflected by the live
//! signal, with leftward velocity as the paper feed; old ink dries (dims) as
//! it crosses. Navigation knocks the pen: a hard spike that rings down over a
//! few ticks, permanently inked into the trace like a real bump to the table.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const INK: Color = Color::Rgb(0xd8, 0x3a, 0x2a);
const INK_DRY: Color = Color::Rgb(0x8a, 0x2e, 0x24);

/// Deflection is encoded in the spark's `seed` (signed rows + 32) so the
/// overlay can redraw the full pen stroke, not just the tip.
fn encode(defl: i32) -> u32 {
    (defl + 32).clamp(0, 64) as u32
}

fn decode(seed: u32) -> i32 {
    (seed & 0x7f) as i32 - 32
}

pub struct Seismograph;

impl ThemeEffect for Seismograph {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Speed, Knob::Persistence]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            speed: 0.45,
            persistence: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Paper-steady, with a tiny tremor when the signal hits hard.
        let tremor = if beat > 0.6 {
            0.9 + (noise(frame as u32, (offset * 53.0) as u32) % 20) as f64 * 0.01
        } else {
            1.0
        };
        scale(base, 0.9 * tremor)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 8 {
            return;
        }
        // One pen mark per tick at the right edge. Deflection = live signal
        // (peak + beat kick), sign flipping pseudo-randomly so the trace is
        // jagged like a needle, not a bar graph.
        let gain = 2.0 + ctx.tuning.intensity * 6.0;
        let amp = (ctx.scope_peak * gain + ctx.beat * 2.0).min(9.0);
        let s = noise(ctx.frame as u32, 0x5E15);
        let sign = if s & 1 == 0 { 1.0 } else { -1.0 };
        // Sub-row noise keeps even a quiet trace faintly alive.
        let defl = (amp * sign * (0.5 + (s % 100) as f32 * 0.005)).round() as i32;
        let paper = 0.25 + ctx.tuning.speed * 0.9;
        let base_y = pen_base(ctx);
        let life = ((area.width as f32 - 3.0) / paper) as u16;
        sim.push(Spark {
            x: (area.right() - 2) as f32,
            y: base_y as f32,
            vx: -paper,
            vy: 0.0,
            age: 0,
            life: life.max(10),
            seed: encode(defl),
        });
        sim.cap();
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {
        // The pen takes a knock: a hard spike in the nav direction that rings
        // down over the next few marks, inked permanently into the drum.
        let area = ctx.screen;
        if area.width < 8 || area.height < 8 {
            return;
        }
        let paper = 0.25 + ctx.tuning.speed * 0.9;
        let base_y = pen_base(ctx);
        let life = ((area.width as f32 - 3.0) / paper) as u16;
        for (i, mag) in [7i32, -5, 3, -2, 1].into_iter().enumerate() {
            let defl = if dir >= 0.0 { mag } else { -mag };
            sim.push(Spark {
                x: (area.right() - 2 - i as u16) as f32,
                y: base_y as f32,
                vx: -paper,
                vy: 0.0,
                age: 0,
                life: life.max(10),
                seed: encode(defl),
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 8 {
            return;
        }
        let base_y = pen_base(ctx);
        let fade_floor = 0.25 + ctx.tuning.persistence as f64 * 0.65;
        let buf = f.buffer_mut();

        // Faint baseline rule across the drum, under the ink.
        for x in (area.x + 1)..(area.right() - 1) {
            let cell = &mut buf[(x, base_y)];
            if cell.symbol() == " " {
                cell.set_char('┄');
                cell.set_fg(Color::Rgb(0x3a, 0x34, 0x2c));
            }
        }

        for p in &sim.sparks {
            let x = p.x.round() as i32;
            if x <= area.x as i32 || x >= area.right() as i32 - 1 {
                continue;
            }
            let defl = decode(p.seed);
            let age_f = p.age as f64 / p.life.max(1) as f64;
            let wet = p.age < 3;
            let fade = (1.0 - age_f * (1.0 - fade_floor)).clamp(0.2, 1.0);
            let col = if wet { INK } else { scale(INK_DRY, fade) };
            // Full pen stroke: vertical run from baseline to the tip, so big
            // hits read as tall strokes rather than floating dots.
            let tip = (base_y as i32 - defl).clamp(area.y as i32 + 1, area.bottom() as i32 - 2);
            let (lo, hi) = (tip.min(base_y as i32), tip.max(base_y as i32));
            for y in lo..=hi {
                let cell = &mut buf[(x as u16, y as u16)];
                let ch = if y == tip {
                    if wet { '●' } else { '•' }
                } else {
                    '│'
                };
                cell.set_char(ch);
                cell.set_fg(col);
            }
        }
    }
}

/// The drum's baseline row: just below the vertical midpoint of the screen.
fn pen_base(ctx: &FrameCtx) -> u16 {
    ctx.screen.y + (ctx.screen.height as u32 * 11 / 20) as u16
}
