//! Vaporwave: an 80s dusk. A scanline sun sinks over a neon perspective grid
//! that scrolls toward you; the sun RGB-glitches and the grid surges on the beat.
//! Everything paints only *empty* cells, so the UI text stays legible on top.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::mix;
use crate::particles::ParticleSim;
use crate::util::noise;

const PINK: Color = Color::Rgb(0xff, 0x6a, 0xd5);
const PURPLE: Color = Color::Rgb(0x8a, 0x2b, 0xe2);
const CYAN: Color = Color::Rgb(0x2d, 0xe6, 0xff);
const GOLD: Color = Color::Rgb(0xff, 0xd0, 0x66);

pub struct Vaporwave;

impl ThemeEffect for Vaporwave {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Speed, Knob::Intensity, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            speed: 0.5,
            intensity: 0.6,
            beat_sync: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = ((frame as f64 * 0.02 + offset).sin() * 0.5 + 0.5 + beat as f64 * 0.3).min(1.0);
        mix(PURPLE, if beat > 0.5 { CYAN } else { PINK }, t)
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as i32, area.height as i32);
        if w < 8 || h < 8 {
            return;
        }
        let frame = ctx.frame as u32;
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let speed = ctx.tuning.speed;
        let horizon = area.y as i32 + (h as f32 * 0.42) as i32;
        let buf = f.buffer_mut();
        let mut put = |x: i32, y: i32, ch: char, col: Color| {
            if x >= area.x as i32
                && x < area.right() as i32
                && y >= area.y as i32
                && y < area.bottom() as i32
            {
                let cell = &mut buf[(x as u16, y as u16)];
                if cell.symbol() == " " {
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        };

        // --- Sun: an ellipse above the horizon, magenta→gold, with scanline gaps
        // in its lower half. A beat jitters it sideways (RGB glitch). ---
        let cx = area.x as i32 + w / 2;
        let cy = area.y as i32 + (h as f32 * 0.24) as i32;
        let rx = (w as f32 * 0.16).max(6.0);
        let ry = (h as f32 * 0.20).max(4.0);
        let jitter = if beat > 0.4 {
            (noise(frame, 0x5) % 5) as i32 - 2
        } else {
            0
        };
        for dy in -(ry as i32)..=(ry as i32) {
            for dx in -(rx as i32)..=(rx as i32) {
                let (nx, ny) = (dx as f32 / rx, dy as f32 / ry);
                if nx * nx + ny * ny > 1.0 {
                    continue;
                }
                let frac = (dy as f32 / ry + 1.0) * 0.5; // 0 top .. 1 bottom
                // Scanline gaps widen toward the bottom of the sun.
                if frac > 0.45 && ((cy + dy) as u32).is_multiple_of(2) {
                    continue;
                }
                put(cx + dx + jitter, cy + dy, '█', mix(PINK, GOLD, frac as f64));
            }
        }

        // --- Floor grid below the horizon: horizontal lines scrolling toward the
        // viewer + vertical rays fanning from the vanishing point. ---
        let scroll = frame as f32 * (0.04 + speed * 0.12) * (1.0 + beat);
        for y in horizon..area.bottom() as i32 {
            let depth = (y - horizon) as f32;
            // Horizontal gridline (spacing widens with depth).
            let phase = depth * depth * 0.03 + scroll;
            if phase.fract() < 0.14 {
                for x in area.x as i32..area.right() as i32 {
                    put(x, y, '─', CYAN);
                }
            }
            // Vertical rays.
            for j in -6i32..=6 {
                let x = cx + (j as f32 * depth * 0.16) as i32;
                let ch = if j < 0 {
                    '\\'
                } else if j > 0 {
                    '/'
                } else {
                    '│'
                };
                put(x, y, ch, PINK);
            }
        }
    }
}
