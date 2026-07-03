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

/// Tube gas color per colorway.
const TUBES: [Color; 3] = [
    Color::Rgb(0xff, 0x5a, 0xd8), // magenta
    Color::Rgb(0x3a, 0xe8, 0xff), // cyan
    Color::Rgb(0xff, 0xc8, 0x3a), // gold
];

fn tube(t: &Tuning) -> Color {
    TUBES[t.choice(Knob::Palette).min(2)]
}

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

    fn border(&self, base: Color, frame: u64, offset: f64, _beat: f32) -> Color {
        // Steady frame with a RARE brief stutter dip (identity accent, ~every
        // few seconds per panel) — never a continuous flicker: that re-colors
        // every panel edge in lockstep and reads as palette flashing.
        let dip = if noise(frame as u32 / 9, (offset * 71.0) as u32).is_multiple_of(37) {
            0.55
        } else {
            1.0
        };
        crate::color::qscale(base, 0.88 * dip)
    }

    fn wave_tint(&self, base: Color, ctx: &FrameCtx, frac: f64, played: bool) -> Color {
        // The waveform is a neon strip: per-segment gas noise (re-rolled a few
        // times a second), buzzing toward the tube color on a hit.
        let seg = (frac * 48.0) as u32;
        let n = (noise(ctx.frame as u32 / 10, seg * 17) % 100) as f64 / 100.0;
        let buzz = crate::color::quant((ctx.routed_beat() * ctx.tuning.beat_sync) as f64, 4.0);
        let lum = 0.65 + 0.35 * n + buzz * 0.25;
        let lit = mix(scale(base, lum.min(1.0)), tube(&ctx.tuning), buzz * 0.4);
        if played { lit } else { scale(lit, 0.7) }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let r = ctx.np_rect;
        if r.width < 6 || r.height < 3 {
            return;
        }
        let intensity = ctx.tuning.intensity as f64;
        // Quantized so a decaying pulse doesn't re-color the sign every frame.
        let buzz = crate::color::quant(
            (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64,
            4.0,
        );
        // Flicker clock: `speed` sets how fast the gas noise re-rolls
        // (~4-12Hz — per-frame rerolls read as harsh strobing).
        let div = (14.0 - ctx.tuning.speed * 9.0).max(5.0) as u32;
        let tick = ctx.frame as u32 / div;

        // Whole-sign dropout: a rare few-frame blackout of the panel.
        let seg = ctx.frame as u32 / 6;
        let sign_out = noise(seg, 0x0E0).is_multiple_of(41);

        let buf = f.buffer_mut();
        for y in (r.y + 1)..(r.bottom() - 1) {
            for x in (r.x + 1)..(r.right() - 1) {
                if ctx.in_thumb(x, y) {
                    continue; // the cover image is not part of the sign
                }
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
                    mix(base, tube(&ctx.tuning), glow)
                } else {
                    base
                });
            }
        }
    }
}
