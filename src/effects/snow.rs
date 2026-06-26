//! Deterministic falling-snow overlay drawn over the whole window. Each flake's
//! column, glyph, fall speed and sway derive from its index + the frame counter,
//! so it animates without any RNG or persistent state. Border stays static.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect};
use crate::particles::ParticleSim;

pub struct Snow;

impl ThemeEffect for Snow {
    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        const GLYPHS: [char; 5] = ['❄', '❅', '*', '·', '✦'];
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w == 0 || h == 0 {
            return;
        }
        let buf = f.buffer_mut();
        let frame = ctx.frame;
        let flakes = (w / 2).max(8);
        for k in 0..flakes {
            let col = k.wrapping_mul(2_654_435_761) % w;
            let phase = k.wrapping_mul(40_503) % h;
            let div = 3 + (k % 4); // fall speed: one cell every 3..6 frames
            let y = ((frame as u32 / div) + phase) % h;
            let sway = ((frame as i64) / 9 + k as i64).rem_euclid(3) as i32 - 1;
            let x = (col as i32 + sway).clamp(0, w as i32 - 1) as u32;
            let shade = match k % 3 {
                0 => Color::Rgb(0xff, 0xff, 0xff),
                1 => Color::Rgb(0xcf, 0xe4, 0xff),
                _ => Color::Rgb(0x9e, 0xb8, 0xd8),
            };
            let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
            cell.set_char(GLYPHS[k as usize % GLYPHS.len()]);
            cell.set_fg(shade);
        }
    }
}
