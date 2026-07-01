//! Datamosh: a glitchier sibling of the Glitch theme. Same purely beat-driven,
//! horizontal-tearing philosophy — clean between hits — but with three upgrades:
//!
//! 1. **Frequency-split beats.** Bass, mid, and treble onsets (`ctx.beat_bands`)
//!    drive different artifacts: a kick rips a thick slab and can roll the whole
//!    screen; snares/mids shove individual rows; hats fire fine scanlines.
//! 2. **True RGB chromatic split.** Torn rows are drawn as interleaved
//!    cyan/magenta/green subpixel columns, each pulled from a different offset,
//!    so displaced text fringes apart like a mistracked CRT.
//! 3. **Vertical roll.** A hard bass hit shoves the whole frame up/down a row or
//!    two for one frame, like the picture losing vertical sync.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

const GREEN: Color = Color::Rgb(0x33, 0xff, 0x66);
const CYAN: Color = Color::Rgb(0x00, 0xff, 0xff);
const MAGENTA: Color = Color::Rgb(0xff, 0x00, 0xcc);

/// Below this pulse a band contributes nothing (keeps it clean between hits).
const GATE: f32 = 0.15;

pub struct Datamosh;

impl ThemeEffect for Datamosh {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Disruption]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.7,
            persistence: 0.0,
            disruption: 0.55,
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        // No audio here (fixed signature) — calm breathing green with rare
        // RGB micro-jumps; the beat lives in `overlay`.
        let n = noise(frame as u32 / 9, (offset * 200.0) as u32) % 100;
        if n < 3 {
            CYAN
        } else if n < 6 {
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

        let [bass, mid, treble] = ctx.beat_bands.map(|v| v.clamp(0.0, 1.0));
        // Clean between beats and on silence.
        if bass < GATE && mid < GATE && treble < GATE {
            return;
        }
        // Remap each band's active range to 0..1 so intensity ramps from the gate.
        let ramp = |v: f32| ((v - GATE) / (1.0 - GATE)).clamp(0.0, 1.0);
        let (bass_h, mid_h, treble_h) = (ramp(bass), ramp(mid), ramp(treble));
        // Tuning: `intensity` scales artifact counts; `disruption` scales how far
        // things move and gates the disorienting vertical roll.
        let intensity = ctx.tuning.intensity;
        let disruption = ctx.tuning.disruption;

        let frame = ctx.frame as u32;
        let buf = f.buffer_mut();

        // (3) Vertical roll on a hard bass hit — do it first so the tears below
        // land on the already-rolled picture. Gated on disruption so it can be
        // turned off entirely for a readable screen.
        if bass > 0.6 && disruption > 0.35 {
            let dy = 1 + (bass > 0.85 && disruption > 0.7) as u16;
            let up = noise(frame, 0x701).is_multiple_of(2);
            vertical_roll(buf, area, dy, up);
        }

        // (1a) Bass slab: a chunk of adjacent rows torn together, chromatic.
        if bass > 0.35 && intensity > 0.05 {
            let seed = noise(frame, 0xB0);
            let bh = 2 + (bass_h * 4.0 * intensity) as u16;
            let y0 = area.y + (seed % h.saturating_sub(bh as u32).max(1)) as u16;
            let dir = if seed & 2 == 0 { 1 } else { -1 };
            let shift =
                dir * (2 + (seed / 5 % (1 + (bass_h * 10.0 * disruption) as u32).max(1)) as isize);
            let split = 1 + (bass_h * 3.0 * disruption) as isize;
            for d in 0..bh {
                let y = y0 + d;
                if y >= area.bottom() {
                    break;
                }
                chroma_tear_row(buf, area, y, shift, split);
            }
        }

        // (1b) Mid: individual rows shoved sideways, chromatic. Count scales.
        let tears = (mid_h * 6.0 * intensity).round() as u32;
        for b in 0..tears {
            let seed = noise(frame ^ b.wrapping_mul(2_654_435_761), b * 31 + 7);
            let y = area.y + (seed % h) as u16;
            let dir = if seed & 1 == 0 { 1 } else { -1 };
            let shift =
                dir * (1 + (seed / 7 % (1 + (mid_h * 7.0 * disruption) as u32).max(1)) as isize);
            let split = 1 + (mid_h * 2.0 * disruption) as isize;
            chroma_tear_row(buf, area, y, shift, split);
        }

        // (1c) Treble: fine bright scanline sweeps — one per hit, cheap flicker.
        if treble > 0.25 && intensity > 0.05 {
            let sweeps = 1 + (treble_h * 3.0 * intensity) as u32;
            for b in 0..sweeps {
                let seed = noise(frame.wrapping_add(b * 911), 0x5CA9 + b);
                let y = area.y + (seed % h) as u16;
                let col = if b % 2 == 0 { CYAN } else { GREEN };
                for i in 0..area.width {
                    let cell = &mut buf[(area.x + i, y)];
                    if !noise(seed + i as u32, y as u32).is_multiple_of(3) {
                        cell.set_char('─');
                    }
                    cell.set_fg(col);
                }
            }
        }
    }
}

/// Draw one torn row with RGB chromatic aberration. The row is displaced by
/// `shift` cells (signed = direction), then split into interleaved
/// cyan/magenta/green subpixel columns, each pulled from a slightly different
/// offset (`split`), so the glyphs fringe apart. Snapshots the row first so the
/// writes don't overlap-borrow the read.
fn chroma_tear_row(buf: &mut Buffer, area: Rect, y: u16, shift: isize, split: isize) {
    let width = area.width as isize;
    let row: Vec<String> = (0..area.width)
        .map(|i| buf[(area.x + i, y)].symbol().to_string())
        .collect();
    for i in 0..area.width as isize {
        // Per-channel offset: cyan and magenta pull from opposite sides, green
        // sits centered — classic subpixel split.
        let (coff, col) = match i % 3 {
            0 => (split, CYAN),
            1 => (-split, MAGENTA),
            _ => (0, GREEN),
        };
        let src = i - shift + coff;
        let cell = &mut buf[(area.x + i as u16, y)];
        if (0..width).contains(&src) {
            cell.set_symbol(&row[src as usize]);
        }
        cell.set_fg(col);
    }
}

/// Roll the whole `area` vertically by `dy` rows (wrapping), moving both glyphs
/// and their colors — the picture losing vertical sync.
fn vertical_roll(buf: &mut Buffer, area: Rect, dy: u16, up: bool) {
    let (cols, rows) = (area.width as usize, area.height as usize);
    if rows == 0 || cols == 0 {
        return;
    }
    let dy = (dy as usize) % rows;
    if dy == 0 {
        return;
    }
    let snap: Vec<(String, Color)> = (0..rows)
        .flat_map(|ry| (0..cols).map(move |cx| (ry, cx)))
        .map(|(ry, cx)| {
            let cell = &buf[(area.x + cx as u16, area.y + ry as u16)];
            (cell.symbol().to_string(), cell.fg)
        })
        .collect();
    for ry in 0..rows {
        let sry = if up {
            (ry + dy) % rows
        } else {
            (ry + rows - dy) % rows
        };
        for cx in 0..cols {
            let (sym, fg) = &snap[sry * cols + cx];
            let cell = &mut buf[(area.x + cx as u16, area.y + ry as u16)];
            cell.set_symbol(sym);
            cell.set_fg(*fg);
        }
    }
}
