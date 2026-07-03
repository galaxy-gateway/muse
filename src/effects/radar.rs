//! Radar: a phosphor sweep rotating inside the oscilloscope panel. Beat
//! onsets drop blips at the swept bearing — bass lands close to the antenna,
//! treble out near the rim — and each blip fades over the next rotations,
//! leaving a readable history of the rhythm painted around the dial. Cell
//! aspect (~2:1) is corrected so the dial reads round, not squashed.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const PHOSPHOR: Color = Color::Rgb(0x39, 0xff, 0x8a);
const RIM: Color = Color::Rgb(0x1a, 0x66, 0x40);

/// Dial geometry inside the scope panel: (center x, center y, y-radius).
/// The x-radius is doubled to correct the terminal cell aspect.
fn dial(r: Rect) -> Option<(f64, f64, f64)> {
    if r.width < 14 || r.height < 7 {
        return None;
    }
    let cy = r.y as f64 + r.height as f64 / 2.0;
    let cx = r.x as f64 + r.width as f64 / 2.0;
    let ry = (r.height as f64 / 2.0 - 1.5).min((r.width as f64 / 2.0 - 2.0) / 2.0);
    (ry >= 2.0).then_some((cx, cy, ry))
}

fn sweep_angle(frame: u64, speed: f32) -> f64 {
    frame as f64 * (0.02 + speed as f64 * 0.06)
}

pub struct Radar;

impl ThemeEffect for Radar {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Speed, Knob::Persistence, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            speed: 0.45,
            persistence: 0.6,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Stepped + quantized to avoid per-frame border rewrites.
        let t = (frame / 6) as f64 * 0.12 + offset;
        let hum = 0.8 + 0.08 * (t * std::f64::consts::TAU).sin();
        let beat = crate::color::quant(beat as f64, 4.0);
        crate::color::qscale(base, (hum + beat * 0.2).min(1.05))
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let Some((cx, cy, ry)) = dial(ctx.scope_rect) else {
            return;
        };
        // A band firing hard drops a blip at the current bearing: bass near
        // the antenna, mid halfway out, treble at the rim. Thinned so one hit
        // doesn't smear a whole arc of contacts.
        if !ctx.frame.is_multiple_of(4) {
            return;
        }
        let a = sweep_angle(ctx.frame, ctx.tuning.speed);
        let life = 80 + (ctx.tuning.persistence * 260.0) as u16;
        for (bi, frac) in [(0usize, 0.35), (1, 0.62), (2, 0.88)] {
            let pulse = ctx.beat_bands[bi] * ctx.tuning.beat_sync;
            if pulse < 0.45 {
                continue;
            }
            let jitter = (noise(ctx.frame as u32, bi as u32 * 53) % 20) as f64 * 0.01 - 0.1;
            let r = ry * frac * (1.0 + jitter);
            sim.push(Spark {
                x: (cx + a.cos() * r * 2.0) as f32,
                y: (cy + a.sin() * r) as f32,
                vx: 0.0,
                vy: 0.0,
                age: 0,
                life,
                seed: bi as u32,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let Some((cx, cy, ry)) = dial(ctx.scope_rect) else {
            return;
        };
        let area = ctx.scope_rect;
        let inside = |x: i32, y: i32| {
            x > area.x as i32
                && x < area.right() as i32 - 1
                && y > area.y as i32
                && y < area.bottom() as i32 - 1
        };
        let buf = f.buffer_mut();

        // Rim: a faint ellipse of ticks.
        let steps = (ry * 16.0) as u32;
        for i in 0..steps {
            let a = i as f64 / steps as f64 * std::f64::consts::TAU;
            let (x, y) = (
                (cx + a.cos() * ry * 2.0).round() as i32,
                (cy + a.sin() * ry).round() as i32,
            );
            if inside(x, y) {
                let cell = &mut buf[(x as u16, y as u16)];
                if cell.symbol() == " " {
                    cell.set_char('·');
                    cell.set_fg(RIM);
                }
            }
        }

        // Sweep + wake: the leading edge bright, ghosts trailing behind it.
        let a0 = sweep_angle(ctx.frame, ctx.tuning.speed);
        for (k, back) in [(0.0f64, 1.0f64), (0.09, 0.55), (0.20, 0.30), (0.34, 0.15)] {
            let a = a0 - k;
            let mut r = 1.0;
            while r <= ry {
                let (x, y) = (
                    (cx + a.cos() * r * 2.0).round() as i32,
                    (cy + a.sin() * r).round() as i32,
                );
                if inside(x, y) {
                    let cell = &mut buf[(x as u16, y as u16)];
                    if back >= 1.0 {
                        cell.set_char('▪');
                    }
                    cell.set_fg(scale(PHOSPHOR, back * (0.5 + r / ry * 0.5)));
                }
                r += 0.5;
            }
        }

        // Contacts: blips fading through the phosphor as they age.
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if !inside(x, y) {
                continue;
            }
            let fade = crate::color::quant(1.0 - p.age as f64 / p.life.max(1) as f64, 10.0);
            let cell = &mut buf[(x as u16, y as u16)];
            cell.set_char(if fade > 0.66 {
                '●'
            } else if fade > 0.33 {
                '◉'
            } else {
                '○'
            });
            cell.set_fg(scale(PHOSPHOR, 0.25 + fade * 0.75));
        }

        // Antenna.
        let (ax, ay) = (cx.round() as i32, cy.round() as i32);
        if inside(ax, ay) {
            let cell = &mut buf[(ax as u16, ay as u16)];
            cell.set_char('╋');
            cell.set_fg(PHOSPHOR);
        }
    }
}
