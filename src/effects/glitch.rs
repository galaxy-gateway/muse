//! Retro CRT glitch, purely beat-reactive. Between beats (and on silence) the
//! screen is clean — no sustained fizz. Each beat (`ctx.beat`) above a gate snaps
//! in horizontal datamosh tears, a chromatic scanline, and, on strong hits, a
//! corrupted-frame block and a short noise burst; all decay away within a few
//! frames. The result stutters in time with the hits and goes quiet otherwise.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

const COLORS: [Color; 4] = [
    Color::Rgb(0x33, 0xff, 0x66),
    Color::Rgb(0x00, 0xff, 0xff),
    Color::Rgb(0xff, 0x00, 0xcc),
    Color::Rgb(0x00, 0xff, 0x88),
];
const CYAN: Color = Color::Rgb(0x00, 0xff, 0xff);
const MAGENTA: Color = Color::Rgb(0xff, 0x00, 0xcc);

pub struct Glitch;

impl ThemeEffect for Glitch {
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        // The border has no audio signal here (fixed trait signature), so keep
        // it tasteful: mostly a slowly breathing green with rare RGB micro-jumps
        // so a static screen isn't dead. The beat lives in `overlay`.
        let n = noise(frame as u32 / 9, (offset * 200.0) as u32) % 100;
        if n < 2 {
            CYAN
        } else if n < 4 {
            MAGENTA
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

        let beat = ctx.beat.clamp(0.0, 1.0);
        // Purely beat-driven: between beats (and on silence) the screen is clean.
        // No sustained fizz — the glitch only exists as a reaction to a hit.
        const GATE: f32 = 0.18;
        if beat < GATE {
            return;
        }
        // Remap the active range to 0..1 so intensity ramps from the gate up.
        let hit = ((beat - GATE) / (1.0 - GATE)).clamp(0.0, 1.0);

        let frame = ctx.frame as u32;
        let buf = f.buffer_mut();

        // --- Beat tears: horizontal datamosh. On a hit, grab single rows and
        // shove them sideways with a chromatic tint. Count + displacement scale
        // with the pulse, so they snap in on hits and vanish between them. ---
        let tears = 1 + (hit * 4.0).round() as u32;
        for b in 0..tears {
            let seed = noise(frame ^ b.wrapping_mul(2_654_435_761), b * 31 + 7);
            let y = area.y + (seed % h) as u16;
            let shift = 1 + (seed / 7 % (2 + (hit * 6.0) as u32)) as usize;
            tear_row(buf, area, y, shift, seed & 1 == 0, tint_for(seed));
        }

        // --- Thick tear band: on a strong hit, a chunk of adjacent rows all
        // slide together by the same offset, like a slab of the screen ripping
        // sideways. Stays in the horizontal-glitch language, no scattered noise. ---
        if beat > 0.6 {
            let seed = noise(frame, 0xB347);
            let bh = 2 + (hit * 4.0) as u16;
            let y0 = area.y + (seed % h.saturating_sub(bh as u32).max(1)) as u16;
            let shift = 2 + (seed / 5 % (3 + (hit * 8.0) as u32)) as usize;
            let right = seed & 2 == 0;
            let tint = tint_for(seed);
            for dy in 0..bh {
                let y = y0 + dy;
                if y >= area.bottom() {
                    break;
                }
                tear_row(buf, area, y, shift, right, tint);
            }
        }

        // --- Beat scanline: one bright sweep line on a hit, tinted by the pulse. ---
        if beat > 0.4 {
            let seed = noise(frame, 0x5CA9);
            let y = area.y + (seed % h) as u16;
            let col = if beat > 0.7 { CYAN } else { COLORS[3] };
            for i in 0..area.width {
                let cell = &mut buf[(area.x + i, y)];
                if noise(seed + i as u32, y as u32) % 3 != 0 {
                    cell.set_char('─');
                }
                cell.set_fg(col);
            }
        }
    }
}

/// Pick a tear tint from a seed: mostly green, sometimes an RGB-split channel.
fn tint_for(seed: u32) -> Color {
    match seed % 3 {
        0 => CYAN,
        1 => MAGENTA,
        _ => COLORS[0],
    }
}

/// Slide one whole row of `area` sideways by `shift` cells (right or left),
/// keeping the shifted glyphs but recoloring the row to `tint` — the core
/// horizontal-datamosh move. Snapshots the row first so the writes don't
/// overlap-borrow the read.
fn tear_row(buf: &mut Buffer, area: Rect, y: u16, shift: usize, right: bool, tint: Color) {
    let row: Vec<String> = (0..area.width)
        .map(|i| buf[(area.x + i, y)].symbol().to_string())
        .collect();
    for i in 0..area.width as usize {
        let src = if right {
            i.checked_sub(shift)
        } else {
            (i + shift < row.len()).then_some(i + shift)
        };
        let cell = &mut buf[(area.x + i as u16, y)];
        if let Some(s) = src.and_then(|j| row.get(j)) {
            cell.set_symbol(s);
        }
        cell.set_fg(tint);
    }
}
