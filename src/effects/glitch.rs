//! Retro CRT glitch: artifact bands, scattered noise glyphs, and a green border
//! with occasional RGB-split jumps. No particles.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

const GLYPHS: [char; 14] = [
    '▓', '▒', '░', '█', '▚', '▞', '▙', '▟', '╳', '┊', '/', '\\', '¦', '╎',
];
const COLORS: [Color; 4] = [
    Color::Rgb(0x33, 0xff, 0x66),
    Color::Rgb(0x00, 0xff, 0xff),
    Color::Rgb(0xff, 0x00, 0xcc),
    Color::Rgb(0x00, 0xff, 0x88),
];

pub struct Glitch;

impl ThemeEffect for Glitch {
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        // Mostly green, with occasional brief RGB-split jumps (calmer).
        let n = noise(frame as u32 / 9, (offset * 200.0) as u32) % 100;
        if n < 4 {
            Color::Rgb(0x00, 0xff, 0xff)
        } else if n < 8 {
            Color::Rgb(0xff, 0x00, 0xcc)
        } else {
            scale(base, 0.7 + 0.3 * ((n % 40) as f64 / 40.0))
        }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 4 || h < 4 {
            return;
        }
        // Slow the whole effect down: only update the glitch state every ~10 frames.
        let slot = ctx.frame as u32 / 10;
        let buf = f.buffer_mut();

        // A couple of horizontal artifact bands that occasionally appear.
        for b in 0..2u32 {
            let on = noise(slot + b * 97, b * 13 + 5);
            if on % 7 != 0 {
                continue; // band idle this slot (rarer)
            }
            let y = (noise(slot, b * 31) % h) as u16;
            let x0 = (noise(slot + 1, b * 17) % w) as u16;
            let len = 3 + (noise(slot, b * 7) % (w / 3).max(1)) as u16;
            for i in 0..len {
                let x = area.x + x0 + i;
                if x >= area.right() {
                    break;
                }
                let s = noise(slot + i as u32, b * 101 + y as u32);
                let cell = &mut buf[(x, area.y + y)];
                cell.set_char(GLYPHS[s as usize % GLYPHS.len()]);
                cell.set_fg(COLORS[(s as usize / 7) % COLORS.len()]);
            }
        }

        // Sparse scattered sparkle artifacts (calm).
        let scatter = (w * h / 200).max(4);
        for k in 0..scatter {
            let s = noise(slot.wrapping_add(k.wrapping_mul(40_503)), k ^ slot);
            if s % 11 != 0 {
                continue;
            }
            let x = area.x + (s % w) as u16;
            let y = area.y + ((s / w) % h) as u16;
            let cell = &mut buf[(x, y)];
            cell.set_char(GLYPHS[(s as usize / 3) % GLYPHS.len()]);
            cell.set_fg(COLORS[s as usize % COLORS.len()]);
        }
    }
}
