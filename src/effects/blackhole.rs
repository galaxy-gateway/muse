//! Blackhole: stars spiral *inward* to a glowing accretion disk (the inverse of
//! the starfield's outward warp). On a bass beat the hole belches a mass-ejection
//! burst back out; a click goes supernova. The motion is computed analytically
//! (radius shrinks + angle winds per star index), so no per-particle state.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::mix;
use crate::particles::ParticleSim;

const VOID: Color = Color::Rgb(0x14, 0x08, 0x1e);
const PURPLE: Color = Color::Rgb(0x7a, 0x3d, 0xd6);
const BLUE: Color = Color::Rgb(0x4d, 0x8c, 0xff);
const HOT: Color = Color::Rgb(0xff, 0xc4, 0x6a);
const WHITE: Color = Color::Rgb(0xff, 0xff, 0xff);

pub struct Blackhole;

impl ThemeEffect for Blackhole {
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

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = (frame as f64 * 0.02 + offset).sin() * 0.5 + 0.5;
        mix(mix(VOID, PURPLE, t), HOT, beat as f64 * 0.6)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 40, 1.3, 18);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        // Mass ejection: a bass beat flings matter outward from the center.
        let bass = ctx.beat_bands[0] * ctx.tuning.beat_sync;
        if bass > 0.3 {
            let s = ctx.screen;
            let (cx, cy) = (s.x + s.width / 2, s.y + s.height / 2);
            sim.burst(ctx.frame, cx, cy, (bass * 30.0) as u32, 1.1, 16);
        }
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as f32, area.height as f32);
        if w < 8.0 || h < 8.0 {
            return;
        }
        render_sparks(f, sim, area, ctx.frame as u32, ejecta_glyph);

        let frame = ctx.frame as f32;
        let speed = ctx.tuning.speed;
        let stars = 20 + (ctx.tuning.density * 80.0) as u32;
        let cx = area.x as f32 + w / 2.0;
        let cy = area.y as f32 + h / 2.0;
        let max_r = (h / 2.0).min(w / 4.0); // rows are the tighter axis
        let buf = f.buffer_mut();
        let mut plot = |x: f32, y: f32, ch: char, col: Color| {
            let (xi, yi) = (x.round() as i32, y.round() as i32);
            if xi >= area.x as i32
                && xi < area.right() as i32
                && yi >= area.y as i32
                && yi < area.bottom() as i32
            {
                let cell = &mut buf[(xi as u16, yi as u16)];
                cell.set_char(ch);
                cell.set_fg(col);
            }
        };

        for k in 0..stars {
            // prog 0 (outer) → 1 (swallowed), looping; each star offset in phase.
            let period = 200.0;
            let prog = ((frame * (0.4 + speed) + k as f32 * 37.0) % period) / period;
            let r = max_r * (1.0 - prog);
            let ang = k as f32 * 2.399_963 + prog * prog * 22.0; // winds up near center
            let x = cx + r * ang.cos() * 2.0; // ×2 for cell aspect
            let y = cy + r * ang.sin();
            // Brighter + hotter closer in.
            let (ch, col) = if prog > 0.82 {
                ('✦', WHITE)
            } else if prog > 0.5 {
                ('•', mix(BLUE, HOT, ((prog - 0.5) / 0.32) as f64))
            } else {
                ('·', mix(PURPLE, BLUE, (prog / 0.5) as f64))
            };
            plot(x, y, ch, col);
        }

        // Accretion ring + dark core.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        let ring_r = max_r * 0.14 * (1.0 + beat * 0.4);
        for i in 0..36 {
            let a = i as f32 / 36.0 * std::f32::consts::TAU;
            plot(
                cx + ring_r * a.cos() * 2.0,
                cy + ring_r * a.sin(),
                '●',
                mix(HOT, WHITE, beat as f64),
            );
        }
        plot(cx, cy, ' ', VOID);
    }
}

fn ejecta_glyph(t: f64, _seed: u32) -> (char, Color) {
    if t < 0.4 {
        ('✦', WHITE)
    } else if t < 0.7 {
        ('•', HOT)
    } else {
        ('·', PURPLE)
    }
}
