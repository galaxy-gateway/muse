//! Deterministic falling-snow overlay drawn over the whole window. Each flake's
//! column, glyph, fall speed and sway derive from its index + the frame counter,
//! so it animates without any RNG or persistent state. Border stays static.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::particles::ParticleSim;
use crate::util::noise;

pub struct Snow;

impl ThemeEffect for Snow {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::Speed, Knob::Wind, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            speed: 0.4,
            wind: 0.3,
            beat_sync: 0.4,
            ..Default::default()
        }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        const GLYPHS: [char; 5] = ['❄', '❅', '*', '·', '✦'];
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w == 0 || h == 0 {
            return;
        }
        let (density, speed, wind) = (ctx.tuning.density, ctx.tuning.speed, ctx.tuning.wind);
        // Beat: a gust that shoves the whole field sideways and speeds the fall
        // briefly on each hit, plus a sparkle boost.
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let gust_dir = if noise(ctx.frame as u32 / 6, 1).is_multiple_of(2) {
            1
        } else {
            -1
        };
        let gust = (beat * 8.0) as i32 * gust_dir;

        let buf = f.buffer_mut();
        let frame = ctx.frame;
        let flakes = ((w as f32) * (0.15 + density * 0.6)) as u32 + 4;
        let sway_amp = 1 + (wind * 3.0) as i64;
        // Faster fall with higher `speed` and on the beat.
        let base_div = (6.0 - speed * 4.0 - beat * 2.0).max(1.0) as u32;
        for k in 0..flakes {
            let col = k.wrapping_mul(2_654_435_761) % w;
            let phase = k.wrapping_mul(40_503) % h;
            let div = base_div + (k % 3);
            let y = ((frame as u32 / div) + phase) % h;
            let sway = ((frame as i64) / 9 + k as i64).rem_euclid(2 * sway_amp + 1) - sway_amp;
            let x = (col as i32 + sway as i32 + gust).clamp(0, w as i32 - 1) as u32;
            // On a strong beat some flakes flash bright/white regardless of shade.
            let bright = beat > 0.5 && k % 4 == 0;
            let shade = if bright {
                Color::Rgb(0xff, 0xff, 0xff)
            } else {
                match k % 3 {
                    0 => Color::Rgb(0xff, 0xff, 0xff),
                    1 => Color::Rgb(0xcf, 0xe4, 0xff),
                    _ => Color::Rgb(0x9e, 0xb8, 0xd8),
                }
            };
            let glyph = if bright {
                '✦'
            } else {
                GLYPHS[k as usize % GLYPHS.len()]
            };
            let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
            cell.set_char(glyph);
            cell.set_fg(shade);
        }
    }
}
