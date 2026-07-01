//! Thunderstorm: driving diagonal rain, and on a bass beat a **full-screen white
//! lightning flash** with a jagged bolt down the screen. The flash is the
//! signature — a visceral, screen-wide reaction to the kick.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::ParticleSim;
use crate::util::noise;

const RAIN: Color = Color::Rgb(0x8a, 0xa6, 0xc8);
const RAIN_DIM: Color = Color::Rgb(0x4a, 0x5e, 0x78);
const FLASH: Color = Color::Rgb(0xf0, 0xf4, 0xff);
const STORM: Color = Color::Rgb(0x1a, 0x22, 0x30);

pub struct Thunderstorm;

impl ThemeEffect for Thunderstorm {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::Intensity, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.6,
            intensity: 0.6, // flash strength
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let n = noise(frame as u32, (offset * 90.0) as u32) % 100;
        if beat > 0.5 {
            FLASH
        } else {
            mix(STORM, RAIN, (n as f64 / 100.0) * 0.6)
        }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 4 || h < 4 {
            return;
        }
        let frame = ctx.frame as u32;
        let bass = (ctx.beat_bands[0] * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let buf = f.buffer_mut();

        // --- Full-screen flash: pale wash over empty cells on a bass beat,
        // strength from intensity. Brief, so it reads as a lightning strobe. ---
        if bass > 0.35 {
            let amt = (bass * ctx.tuning.intensity) as f64;
            for ry in 0..area.height {
                for rx in 0..area.width {
                    let cell = &mut buf[(area.x + rx, area.y + ry)];
                    if cell.symbol() == " " {
                        cell.set_char('░');
                        cell.set_fg(scale(FLASH, (0.25 + amt * 0.6).min(1.0)));
                    }
                }
            }
        }

        // --- Diagonal rain: thin streaks falling down-left, density-scaled. ---
        let drops = (w * (2 + (ctx.tuning.density * 6.0) as u32)).max(8);
        for k in 0..drops {
            let seed = noise(k.wrapping_mul(2_654_435_761), 0x2A1);
            let col0 = seed % w;
            let phase = (seed / 7) % h;
            let speed = 2 + (seed % 3);
            let y = ((frame * speed + phase) % h) as i32;
            // slight down-left slant
            let x = (col0 as i32 - (y / 3)).rem_euclid(w as i32);
            let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
            if cell.symbol() == " " {
                cell.set_char('╱');
                cell.set_fg(if seed % 4 == 0 { RAIN } else { RAIN_DIM });
            }
        }

        // --- Lightning bolt on a strong bass hit: a jagged vertical fork. ---
        if bass > 0.55 {
            let seed = noise(frame / 2, 0xB01);
            let mut x = (area.x + (seed % w) as u16) as i32;
            for y in area.y..area.bottom() {
                let n = noise(seed + y as u32, frame);
                x = (x + (n % 3) as i32 - 1).clamp(area.x as i32, area.right() as i32 - 1);
                let ch = ['│', '╿', '╽'][(n % 3) as usize];
                let cell = &mut buf[(x as u16, y)];
                cell.set_char(ch);
                cell.set_fg(FLASH);
            }
        }
    }
}
