//! Aurora borealis: shimmering vertical curtains of green/violet light waving
//! across the top of the screen. The curtains ripple and surge brighter on the
//! beat. Paints only empty cells so text stays readable.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::ParticleSim;

const GREEN: Color = Color::Rgb(0x3d, 0xff, 0xa6);
const VIOLET: Color = Color::Rgb(0x9a, 0x4d, 0xff);
const TEAL: Color = Color::Rgb(0x2d, 0xe6, 0xff);

pub struct Aurora;

impl ThemeEffect for Aurora {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Speed, Knob::Intensity, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            speed: 0.4,
            intensity: 0.6,
            beat_sync: 0.4,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = (frame as f64 * 0.015 + offset).sin() * 0.5 + 0.5;
        scale(mix(GREEN, VIOLET, t), (0.6 + beat as f64 * 0.4).min(1.0))
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width, area.height);
        if w < 4 || h < 6 {
            return;
        }
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let t = ctx.frame as f32 * (0.02 + ctx.tuning.speed * 0.05);
        let reach = 0.35 + ctx.tuning.intensity * 0.35; // fraction of height the curtains fall
        const GLYPHS: [char; 4] = ['░', '▒', '▓', '█'];
        let buf = f.buffer_mut();
        for rx in 0..w {
            let x = rx as f32;
            // Each column's curtain: a waving base height + a slow horizontal drift.
            let wave = (x * 0.18 + t).sin() * 0.5 + (x * 0.07 - t * 1.3).sin() * 0.5;
            let bottom =
                ((0.15 + reach) * (0.6 + 0.4 * (wave * 0.5 + 0.5)) * h as f32 * (1.0 + beat * 0.3))
                    as u16;
            for ry in 0..bottom.min(h) {
                let cell = &mut buf[(area.x + rx, area.y + ry)];
                if cell.symbol() != " " {
                    continue;
                }
                let depth = ry as f32 / bottom.max(1) as f32; // 0 top .. 1 bottom of curtain
                // Green low, violet high, teal shimmer at the leading edge.
                let hue_t = (depth + wave * 0.2).clamp(0.0, 1.0);
                let mut col = mix(VIOLET, GREEN, hue_t as f64);
                if depth > 0.8 {
                    col = mix(col, TEAL, 0.5);
                }
                let fade = (1.0 - depth) * (0.5 + beat * 0.5);
                let g = GLYPHS[((fade * 3.9) as usize).min(3)];
                cell.set_char(g);
                cell.set_fg(scale(col, (0.35 + fade as f64).min(1.0)));
            }
        }
    }
}
