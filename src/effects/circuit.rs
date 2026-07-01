//! Circuit: a neon cyberpunk board. Faint traces run across the screen; bright
//! **data pulses** race along them, forking at junctions, and fire faster + more
//! often on the beat. Traces + pulses paint only empty cells so the UI stays
//! readable, like glowing wiring behind the panels.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::ParticleSim;
use crate::util::noise;

const TRACE: Color = Color::Rgb(0x14, 0x40, 0x38);
const NEON: Color = Color::Rgb(0x2d, 0xff, 0xd0);
const NEON2: Color = Color::Rgb(0x2d, 0xa6, 0xff);
const HOT: Color = Color::Rgb(0xe6, 0xff, 0xff);

pub struct Circuit;

impl ThemeEffect for Circuit {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::Speed, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            speed: 0.5,
            beat_sync: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = (frame as f64 * 0.03 + offset).sin() * 0.5 + 0.5;
        mix(
            scale(NEON, 0.5),
            HOT,
            (t * 0.4 + beat as f64 * 0.6).min(1.0),
        )
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width, area.height);
        if w < 8 || h < 8 {
            return;
        }
        let frame = ctx.frame as u32;
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let speed = 1.0 + ctx.tuning.speed + beat * 2.0;
        let lines = 3 + (ctx.tuning.density * 8.0) as u32;
        let buf = f.buffer_mut();
        let mut put = |x: u16, y: u16, ch: char, col: Color| {
            if x >= area.x && x < area.right() && y >= area.y && y < area.bottom() {
                let cell = &mut buf[(x, y)];
                if cell.symbol() == " " {
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        };

        // Horizontal traces at seeded rows, each with a pulse racing along it.
        for j in 0..lines {
            let seed = noise(j.wrapping_mul(2_654_435_761), 0xC1);
            let ry = area.y + (seed % h as u32) as u16;
            for x in area.x..area.right() {
                put(x, ry, '─', TRACE);
            }
            let head = ((frame as f32 * speed + (seed % 97) as f32) as u32) % w as u32;
            let col = if j % 2 == 0 { NEON } else { NEON2 };
            for tail in 0..4u32 {
                let hx = ((head + w as u32 - tail) % w as u32) as u16;
                let c = if tail == 0 {
                    HOT
                } else {
                    scale(col, 1.0 - tail as f64 * 0.22)
                };
                put(area.x + hx, ry, if tail == 0 { '◆' } else { '─' }, c);
            }
        }
        // Vertical traces at seeded columns, pulses racing down.
        for j in 0..lines {
            let seed = noise(j.wrapping_mul(40_503) + 7, 0xC2);
            let cx = area.x + (seed % w as u32) as u16;
            for y in area.y..area.bottom() {
                put(cx, y, '│', TRACE);
            }
            let head = ((frame as f32 * speed + (seed % 89) as f32) as u32) % h as u32;
            for tail in 0..4u32 {
                let hy = ((head + h as u32 - tail) % h as u32) as u16;
                let c = if tail == 0 {
                    HOT
                } else {
                    scale(NEON2, 1.0 - tail as f64 * 0.22)
                };
                put(cx, area.y + hy, if tail == 0 { '◆' } else { '│' }, c);
            }
        }
    }
}
