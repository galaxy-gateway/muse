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
        sim.burst(ctx.frame, col, row, 30, 1.1, 16);
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as f32, area.height as f32);
        if w < 12.0 || h < 8.0 {
            return;
        }
        render_sparks(f, sim, area, ctx.frame as u32, ejecta_glyph);

        // A full-screen swirl behind the panes: everything is drawn only on empty
        // cells, so the UI text/borders sit on top and stay perfectly readable —
        // the accretion disk peeks through the gaps around the boxes.
        let frame = ctx.frame as f32;
        let speed = ctx.tuning.speed;
        let stars = 40 + (ctx.tuning.density * 160.0) as u32;
        let cx = area.x as f32 + w / 2.0;
        let cy = area.y as f32 + h / 2.0;
        let max_r = (h / 2.0).max(w / 4.0);
        let beat = ctx.beat * ctx.tuning.beat_sync;
        let buf = f.buffer_mut();
        let mut plot = |x: f32, y: f32, ch: char, col: Color| {
            let (xi, yi) = (x.round() as i32, y.round() as i32);
            if xi >= area.x as i32
                && xi < area.right() as i32
                && yi >= area.y as i32
                && yi < area.bottom() as i32
            {
                let cell = &mut buf[(xi as u16, yi as u16)];
                if cell.symbol() == " " {
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        };

        for k in 0..stars {
            let period = 240.0;
            // `speed` floor is very low so it can crawl almost imperceptibly.
            let prog = ((frame * (0.03 + speed * 1.2) + k as f32 * 31.0) % period) / period;
            let r = max_r * (1.0 - prog);
            let ang = k as f32 * 2.399_963 + prog * prog * 26.0; // winds up near center
            let x = cx + r * ang.cos() * 2.0; // ×2 for cell aspect
            let y = cy + r * ang.sin();
            let (ch, col) = if prog > 0.82 {
                ('✦', WHITE)
            } else if prog > 0.5 {
                ('•', mix(BLUE, HOT, ((prog - 0.5) / 0.32) as f64))
            } else {
                ('·', mix(PURPLE, BLUE, (prog / 0.5) as f64))
            };
            plot(x, y, ch, col);
        }

        // Accretion ring, brightening on the beat.
        let ring_r = (max_r * 0.12 * (1.0 + beat * 0.5)).max(1.0);
        for i in 0..48 {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            plot(
                cx + ring_r * a.cos() * 2.0,
                cy + ring_r * a.sin(),
                '●',
                mix(HOT, WHITE, beat as f64),
            );
        }
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
