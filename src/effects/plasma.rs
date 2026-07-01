//! Plasma: a flowing lava-lamp color field. Overlapping sine waves make drifting
//! blobs of magenta/cyan/violet that bloom and speed up on the beat. Paints only
//! empty cells (block glyphs), leaving UI text readable on top.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::hue;
use crate::particles::ParticleSim;

/// Row-major mask of which cells in `area` are blank (a space), captured *before*
/// any painting so neighbour tests reflect the original UI, not our own fills.
/// Shared by the backdrop themes (plasma, aurora).
pub(super) fn blank_mask(buf: &Buffer, area: Rect) -> Vec<bool> {
    (0..area.height)
        .flat_map(|ry| (0..area.width).map(move |rx| (rx, ry)))
        .map(|(rx, ry)| buf[(area.x + rx, area.y + ry)].symbol() == " ")
        .collect()
}

pub struct Plasma;

impl ThemeEffect for Plasma {
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

    fn border(&self, _base: Color, frame: u64, offset: f64, _beat: f32) -> Color {
        hue(frame as f64 * 0.01 + offset + 0.6)
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 4 || area.height < 4 {
            return;
        }
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let t = ctx.frame as f64 * (0.03 + ctx.tuning.speed as f64 * 0.06);
        // Beat blooms the field brighter/faster; intensity sets density of glyphs.
        let gain = 1.0 + beat as f64 * 0.5;
        let sat = 0.45 + ctx.tuning.intensity as f64 * 0.45;
        const GLYPHS: [char; 4] = ['█', '▓', '▒', '░'];
        let (w, h) = (area.width, area.height);
        let buf = f.buffer_mut();
        // Snapshot the *original* blank mask before painting anything, so the
        // neighbour test isn't fooled by our own fills (which caused the
        // every-other-cell checkerboard). A cell fills only if it and its
        // horizontal neighbours were blank — the interior of open fields.
        let blank = blank_mask(buf, area);
        for ry in 0..h {
            for rx in 0..w {
                let idx = ry as usize * w as usize + rx as usize;
                let ok =
                    blank[idx] && (rx == 0 || blank[idx - 1]) && (rx + 1 >= w || blank[idx + 1]);
                if !ok {
                    continue;
                }
                let (x, y) = (rx as f64, ry as f64);
                // Classic multi-sine plasma, 0..1.
                let v = (((x * 0.20 + t).sin()
                    + (y * 0.24 - t * 0.8).sin()
                    + ((x + y) * 0.15 + t * 0.5).sin()
                    + ((x * x + y * y).sqrt() * 0.16 - t).sin())
                    * 0.25
                    * 0.5
                    + 0.5)
                    * gain;
                let v = v.clamp(0.0, 1.0);
                // Below a floor, leave it dark (breathing negative space).
                if v < 0.35 {
                    continue;
                }
                let col = hue(v * 0.35 + 0.72 + t * 0.02); // magenta→violet→cyan band
                let g = GLYPHS[((1.0 - v) * (sat + 0.5) * 3.0) as usize % GLYPHS.len()];
                let cell = &mut buf[(area.x + rx, area.y + ry)];
                cell.set_char(g);
                cell.set_fg(col);
            }
        }
    }
}
