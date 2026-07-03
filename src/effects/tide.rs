//! Tide: a water band along the bottom of the screen. Two traveling sine
//! components give the surface a natural cross-chop; bass energy swells the
//! whole band upward. Text below the waterline keeps its glyphs but is tinted
//! toward deep blue (readable underwater); the surface itself is drawn as
//! bright chop. Beat crests throw spray particles that the wind hook drifts
//! sideways as they fall out of the air.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

/// (surface chop, foam, deep tint, underwater speck) per colorway.
struct Water {
    surface: Color,
    foam: Color,
    deep: (f64, f64, f64),
    speck: Color,
}
const COLORWAYS: [Water; 3] = [
    Water {
        surface: Color::Rgb(0x7a, 0xd8, 0xe8),
        foam: Color::Rgb(0xe8, 0xff, 0xff),
        deep: (30.0, 90.0, 140.0),
        speck: Color::Rgb(0x3a, 0x74, 0xa0),
    }, // ocean
    Water {
        surface: Color::Rgb(0x6a, 0xe8, 0xb8),
        foam: Color::Rgb(0xe0, 0xff, 0xf0),
        deep: (16.0, 96.0, 72.0),
        speck: Color::Rgb(0x3a, 0xa0, 0x7a),
    }, // lagoon
    Water {
        surface: Color::Rgb(0x8a, 0x7a, 0xe8),
        foam: Color::Rgb(0xe0, 0xd8, 0xff),
        deep: (44.0, 32.0, 96.0),
        speck: Color::Rgb(0x5c, 0x4a, 0xa8),
    }, // abyss
];

fn water(t: &Tuning) -> &'static Water {
    &COLORWAYS[t.choice(Knob::Palette).min(2)]
}

pub struct Tide;

/// Water surface height (rows above the band floor) for column `x`.
fn surface(x: u16, frame: u64, swell: f64, intensity: f64) -> f64 {
    let t = frame as f64 * 0.05;
    let xf = x as f64;
    let chop = (xf * 0.23 - t).sin() + 0.6 * (xf * 0.11 + t * 0.7).sin();
    1.5 + swell + chop * (0.8 + intensity * 1.6)
}

impl ThemeEffect for Tide {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Intensity,
            Knob::Density,
            Knob::Wind,
            Knob::BeatSync,
            Knob::BeatSource,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.5,
            density: 0.5,
            wind: 0.4,
            beat_sync: 0.6,
            beat_source: 1.0 / 3.0, // bass swells the water
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        // Steady frame — animated border brightness re-colors every panel
        // edge in lockstep and reads as the palette flashing; this theme's
        // character lives in its overlay.
        crate::color::qscale(base, 0.88)
    }

    fn wave_tint(&self, base: Color, ctx: &FrameCtx, frac: f64, played: bool) -> Color {
        // The waveform is water too: a slow brightness swell travels along it,
        // lifting toward foam on a bass hit. Stepped + quantized so columns
        // only re-color on real level changes.
        let t = (ctx.frame / 6) as f64 * 0.18;
        let ripple = crate::color::quant(
            0.8 + 0.2 * ((frac * 9.0 - t) * std::f64::consts::TAU * 0.16).sin(),
            10.0,
        );
        let bass = crate::color::quant((ctx.routed_beat() * ctx.tuning.beat_sync) as f64, 4.0);
        let lifted = mix(scale(base, ripple), water(&ctx.tuning).foam, bass * 0.35);
        if played { lifted } else { scale(lifted, 0.75) }
    }

    fn wind(&self, ctx: &FrameCtx) -> Option<f32> {
        // A slowly shifting onshore/offshore breeze for the spray.
        let t = ctx.frame as f64 * 0.008;
        Some((t.sin() * 1.6 * ctx.tuning.wind as f64) as f32)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 10 {
            return;
        }
        let bass = (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        // Gentle ambient spume plus a burst on a bass crest.
        let base_rate = if ctx.frame.is_multiple_of(6) { 1 } else { 0 };
        let n = base_rate + (bass * 5.0 * ctx.tuning.density) as u32;
        if n == 0 {
            return;
        }
        let floor = ctx.transport_rect.y;
        let swell = bass as f64 * 3.0;
        for i in 0..n {
            let s = noise(ctx.frame as u32, i * 97 + 13);
            let x = area.x + (s % area.width as u32) as u16;
            let h = surface(x, ctx.frame, swell, ctx.tuning.intensity as f64);
            // Launch only from genuinely high columns (crests).
            if h < 2.2 && bass < 0.4 {
                continue;
            }
            sim.push(Spark {
                x: x as f32,
                y: (floor as f64 - h - 1.0).max(area.y as f64 + 1.0) as f32,
                vx: ((s % 5) as f32 - 2.0) * 0.15,
                vy: -(0.25 + (s % 4) as f32 * 0.15) - bass * 0.4,
                age: 0,
                life: 8 + (s % 8) as u16,
                seed: s,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 10 {
            return;
        }
        // Quantized: a decaying pulse must not re-tint the whole band per frame.
        let bass = crate::color::quant(
            (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64,
            4.0,
        );
        let swell = bass * 3.0;
        let intensity = ctx.tuning.intensity as f64;
        let floor = ctx.transport_rect.y; // water sits above the transport bar
        let w = water(&ctx.tuning);
        let buf = f.buffer_mut();

        for x in area.x..area.right() {
            let h = surface(x, ctx.frame, swell, intensity).max(0.5);
            let top = (floor as f64 - h).max(area.y as f64 + 1.0) as u16;
            if ctx.in_thumb(x, top) {
                continue; // leave the cover image untouched
            }
            // Surface chop: bright glyph riding the waterline; glyph choice
            // re-rolls a few times a second, not per frame.
            let cell = &mut buf[(x, top)];
            let s = noise(x as u32, ctx.frame as u32 / 10);
            // Single-width glyphs only: wide chars here would drift columns.
            cell.set_char(match s % 4 {
                0 => '~',
                1 => '≈',
                2 => '∿',
                _ => '~',
            });
            cell.set_fg(mix(w.surface, w.foam, bass * 0.7));
            // Below the waterline: tint existing glyphs toward deep blue, and
            // texture blank cells lightly so the body of water reads as water.
            for y in (top + 1)..floor {
                if ctx.in_thumb(x, y) {
                    continue;
                }
                let depth = (y - top) as f64 / h.max(1.0);
                // Quantized: the surface height moves every frame, but the
                // tint of a given cell should only change on real steps.
                let k = crate::color::quant(0.35 + depth * 0.4, 8.0);
                let cell = &mut buf[(x, y)];
                if cell.symbol() == " " {
                    if noise(x as u32 * 31, y as u32 * 17).is_multiple_of(11) {
                        cell.set_char('·');
                        cell.set_fg(w.speck);
                    }
                } else {
                    let (r, g, b) = crate::color::rgb_of(cell.fg);
                    cell.set_fg(Color::Rgb(
                        (r + (w.deep.0 - r) * k) as u8,
                        (g + (w.deep.1 - g) * k) as u8,
                        (b + (w.deep.2 - b) * k) as u8,
                    ));
                }
            }
        }

        // Spray: bright droplets fading as they fly.
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= floor as i32
            {
                continue;
            }
            let fade = 1.0 - p.age as f64 / p.life.max(1) as f64;
            let cell = &mut buf[(x as u16, y as u16)];
            cell.set_char(if p.age < 3 { '∘' } else { '·' });
            cell.set_fg(scale(w.foam, 0.4 + fade * 0.6));
        }
    }
}
