//! Radar: the whole screen is the scope. A phosphor beam sweeps the full
//! display, momentarily tinting the glyphs it passes over (the UI itself is
//! the terrain) and leaving a short wake. Band onsets drop contacts at the
//! swept bearing — bass near the antenna, treble out at the rim — which drift
//! slowly and, like a real PPI display, glow brightest just after the beam
//! passes and fade until the next rotation. Dim range rings and bearing ticks
//! sit on blank cells only, so the interface stays readable underneath.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{quant, rgb_of, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const PHOSPHOR: Color = Color::Rgb(0x39, 0xff, 0x8a);
const RIM: Color = Color::Rgb(0x1a, 0x66, 0x40);

/// Screen-filling dial: (center x, center y, x-radius, y-radius). The x radius
/// runs wider than 2:1 cell aspect would suggest so the beam reaches the
/// corners of wide terminals.
fn dial(ctx: &FrameCtx) -> Option<(f64, f64, f64, f64)> {
    let s = ctx.screen;
    if s.width < 20 || s.height < 10 {
        return None;
    }
    let cx = s.x as f64 + s.width as f64 / 2.0;
    let cy = s.y as f64 + s.height as f64 / 2.0;
    Some((
        cx,
        cy,
        s.width as f64 / 2.0 - 1.0,
        s.height as f64 / 2.0 - 1.0,
    ))
}

fn sweep_angle(frame: u64, speed: f32) -> f64 {
    (frame as f64 * (0.008 + speed as f64 * 0.03)).rem_euclid(std::f64::consts::TAU)
}

/// Bearing of a point relative to the dial center (cell-aspect corrected).
fn bearing(cx: f64, cy: f64, rx: f64, ry: f64, x: f64, y: f64) -> f64 {
    ((y - cy) / ry)
        .atan2((x - cx) / rx)
        .rem_euclid(std::f64::consts::TAU)
}

pub struct Radar;

impl ThemeEffect for Radar {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Speed,
            Knob::Persistence,
            Knob::Density,
            Knob::BeatSync,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            speed: 0.5,
            persistence: 0.6,
            density: 0.5,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        crate::color::qscale(base, 0.84)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let Some((cx, cy, rx, ry)) = dial(ctx) else {
            return;
        };
        // Contacts drift very slowly; steer nothing, just spawn. A band firing
        // hard drops one at the current bearing: bass close in, treble at the
        // rim. Thinned so one hit is one contact, not an arc smear.
        if !ctx.frame.is_multiple_of(4) {
            return;
        }
        let a = sweep_angle(ctx.frame, ctx.tuning.speed);
        let life = 240 + (ctx.tuning.persistence * 600.0) as u16;
        for (bi, frac) in [(0usize, 0.30), (1, 0.58), (2, 0.85)] {
            let pulse = ctx.beat_bands[bi] * ctx.tuning.beat_sync;
            let gate = 0.55 - ctx.tuning.density * 0.2;
            if pulse < gate {
                continue;
            }
            let s = noise(ctx.frame as u32, bi as u32 * 53);
            let jitter = (s % 20) as f64 * 0.01 - 0.1;
            let r = frac * (1.0 + jitter);
            sim.push(Spark {
                x: (cx + a.cos() * rx * r) as f32,
                y: (cy + a.sin() * ry * r) as f32,
                vx: ((s % 5) as f32 - 2.0) * 0.008,
                vy: ((s / 7 % 5) as f32 - 2.0) * 0.004,
                age: 0,
                life,
                seed: s,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let Some((cx, cy, rx, ry)) = dial(ctx) else {
            return;
        };
        let area = ctx.screen;
        let inside = |x: i32, y: i32| {
            x >= area.x as i32
                && x < area.right() as i32
                && y >= area.y as i32
                && y < area.bottom() as i32
        };
        let buf = f.buffer_mut();

        // Range rings + bearing ticks: static furniture on blank cells only.
        for ring in [0.35f64, 0.65, 0.95] {
            let steps = (rx.max(ry) * ring * 10.0) as u32;
            for i in 0..steps {
                let a = i as f64 / steps as f64 * std::f64::consts::TAU;
                let (x, y) = (
                    (cx + a.cos() * rx * ring).round() as i32,
                    (cy + a.sin() * ry * ring).round() as i32,
                );
                if inside(x, y) {
                    let cell = &mut buf[(x as u16, y as u16)];
                    if cell.symbol() == " " {
                        cell.set_char('·');
                        cell.set_fg(scale(RIM, if ring > 0.9 { 1.0 } else { 0.7 }));
                    }
                }
            }
        }

        // The beam: leading edge plus a trailing wake, tinting the glyphs it
        // crosses toward phosphor green (texture dots on blanks so the beam
        // is visible over empty space). Motion is a thin rotating line — the
        // rest of the screen is untouched each frame.
        let a0 = sweep_angle(ctx.frame, ctx.tuning.speed);
        for (k, glow) in [(0.0f64, 1.0f64), (0.05, 0.6), (0.12, 0.35), (0.22, 0.18)] {
            let a = a0 - k;
            let (dx, dy) = (a.cos(), a.sin());
            let steps = (rx.max(ry) * 2.2) as u32;
            for i in 1..steps {
                let t = i as f64 / steps as f64;
                let (x, y) = (
                    (cx + dx * rx * t).round() as i32,
                    (cy + dy * ry * t).round() as i32,
                );
                if !inside(x, y) || ctx.in_thumb(x as u16, y as u16) {
                    continue;
                }
                let cell = &mut buf[(x as u16, y as u16)];
                if cell.symbol() == " " {
                    if glow > 0.3 {
                        cell.set_char('·');
                        cell.set_fg(scale(PHOSPHOR, glow * 0.55));
                    }
                } else {
                    let (r, g, b) = rgb_of(cell.fg);
                    let (pr, pg, pb) = rgb_of(PHOSPHOR);
                    let m = glow * 0.7;
                    cell.set_fg(Color::Rgb(
                        (r + (pr - r) * m) as u8,
                        (g + (pg - g) * m) as u8,
                        (b + (pb - b) * m) as u8,
                    ));
                }
            }
        }

        // Contacts: each glows hardest right after the beam crosses its
        // bearing and decays over the rest of the rotation — brightness is a
        // function of angular lag, so it changes gradually, never in lockstep.
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if !inside(x, y) {
                continue;
            }
            let b = bearing(cx, cy, rx, ry, p.x as f64, p.y as f64);
            let lag = (a0 - b).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
            let sweep_glow = (1.0 - lag).powi(2);
            let age_fade = 1.0 - p.age as f64 / p.life.max(1) as f64;
            let lum = quant((0.25 + sweep_glow * 0.75) * (0.3 + age_fade * 0.7), 10.0);
            let cell = &mut buf[(x as u16, y as u16)];
            cell.set_char(if lum > 0.6 {
                '●'
            } else if lum > 0.35 {
                '◉'
            } else {
                '○'
            });
            cell.set_fg(scale(PHOSPHOR, 0.2 + lum * 0.8));
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
