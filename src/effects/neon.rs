//! Neon sign: the now-playing panel is a storefront sign that never got
//! fixed. Every glyph carries its own per-frame brightness noise, single
//! letters die for a moment like a bad tube, the whole sign occasionally
//! drops out, and bass makes the gas buzz brighter and jumpier. Bright
//! flashes pull glyph color toward the tube glow. Everything outside the
//! panel stays calm — the sign is the show.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::ParticleSim;
use crate::util::noise;

const TUBE: Color = Color::Rgb(0xff, 0x5a, 0xd8);

pub struct Neon;

impl ThemeEffect for Neon {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Intensity,
            Knob::Speed,
            Knob::BeatSync,
            Knob::BeatSource,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.55,
            speed: 0.5,
            beat_sync: 0.6,
            beat_source: 1.0 / 3.0, // bass buzzes the gas
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // The sign's frame hums along, with its own occasional stutter.
        let n = noise(frame as u32 / 3, (offset * 71.0) as u32) % 100;
        let stutter = if n < 4 {
            0.5
        } else {
            0.85 + (n % 12) as f64 * 0.012
        };
        scale(base, (stutter + beat as f64 * 0.2).min(1.1))
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let r = ctx.np_rect;
        if r.width < 6 || r.height < 3 {
            return;
        }
        let intensity = ctx.tuning.intensity as f64;
        let buzz = (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64;
        // Flicker clock: `speed` sets how fast the noise re-rolls.
        let div = (6.0 - ctx.tuning.speed * 4.0).max(1.0) as u32;
        let tick = ctx.frame as u32 / div;

        // Whole-sign dropout: a rare few-frame blackout of the panel.
        let seg = ctx.frame as u32 / 6;
        let sign_out = noise(seg, 0x0E0).is_multiple_of(41);

        let buf = f.buffer_mut();
        for y in (r.y + 1)..(r.bottom() - 1) {
            for x in (r.x + 1)..(r.right() - 1) {
                let cell = &mut buf[(x, y)];
                if cell.symbol() == " " {
                    continue;
                }
                let key = x as u32 * 13 + y as u32 * 131;
                // Dead tube: one letter stays dark for ~three quarters of a
                // second, picked on a slow clock so it visibly "goes out".
                let dead = noise(ctx.frame as u32 / 44, key).is_multiple_of(29);
                // Per-glyph gas noise, deeper with intensity, jumpier on buzz.
                let n = (noise(tick, key) % 100) as f64 / 100.0;
                let depth = 0.25 + intensity * 0.45 + buzz * 0.15;
                let mut lum = (1.0 - depth) + depth * n + buzz * 0.35;
                if dead {
                    lum = 0.12;
                }
                if sign_out {
                    lum *= 0.15;
                }
                let lum = lum.clamp(0.08, 1.25);
                // Hot glyphs shift toward the tube glow color.
                let glow = ((lum - 0.9) / 0.35).clamp(0.0, 0.6);
                let base = scale(cell.fg, lum.min(1.0));
                cell.set_fg(if glow > 0.0 {
                    mix(base, TUBE, glow)
                } else {
                    base
                });
            }
        }
    }
}
