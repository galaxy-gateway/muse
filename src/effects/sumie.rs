//! Sumi-e: a meditative ink-wash. Monochrome — sparse ink specks settle, clicks
//! bloom an expanding ink splash, and each beat sends a soft ripple ring from the
//! center. One red hanko seal stamps the corner. The calm, minimal counterpart to
//! the busy themes.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const INK: Color = Color::Rgb(0x20, 0x20, 0x26);
const WASH: Color = Color::Rgb(0x5a, 0x5a, 0x64);
const MIST: Color = Color::Rgb(0x9a, 0x9a, 0xa4);
const SEAL: Color = Color::Rgb(0xc4, 0x28, 0x28);

pub struct Sumie;

impl ThemeEffect for Sumie {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.5,
            beat_sync: 0.4,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Dry brush gray; a faint red bleed on a strong beat.
        if beat > 0.55 {
            SEAL
        } else {
            let n = noise(frame as u32 / 4, (offset * 70.0) as u32) % 100;
            scale(base, 0.4 + 0.3 * (n as f64 / 100.0))
        }
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        // Ink splash: a slow, wide bloom.
        sim.burst(ctx.frame, col, row, 22, 0.5, 40);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        // Very sparse settling specks.
        if ctx.frame % 3 == 0 {
            let seed = noise(ctx.frame as u32, 0x1E);
            let count = 1 + (ctx.tuning.intensity * 2.0) as u32;
            for i in 0..count {
                let sd = noise(seed + i * 71, 0x1F);
                sim.push(Spark {
                    x: (s.x as u32 + sd % s.width as u32) as f32,
                    y: s.y as f32,
                    vx: ((sd % 5) as f32 - 2.0) * 0.03,
                    vy: 0.06 + (sd % 3) as f32 * 0.03,
                    age: 0,
                    life: 220,
                    seed: sd,
                });
            }
        }
        // Beat ripple: a gentle ring of ink from the center.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        if beat > 0.4 {
            let (cx, cy) = (s.x + s.width / 2, s.y + s.height / 2);
            sim.burst(ctx.frame, cx, cy, (beat * 16.0) as u32, 0.7, 24);
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        render_sparks(f, sim, area, ctx.frame as u32, ink_glyph);
        // Red hanko seal in the bottom-right (single width-1 glyph — no CJK).
        if area.width > 3 && area.height > 2 {
            let cell = &mut f.buffer_mut()[(area.right() - 2, area.bottom() - 2)];
            cell.set_char('◉');
            cell.set_fg(SEAL);
        }
    }
}

/// Ink glyph by age: wet/dark → dry/misty.
fn ink_glyph(t: f64, seed: u32) -> (char, Color) {
    let pick = |arr: &[char]| arr[seed as usize % arr.len()];
    if t < 0.3 {
        (pick(&['●', '◍', '◉']), INK)
    } else if t < 0.65 {
        (pick(&['◌', '○', '∘']), WASH)
    } else {
        (pick(&['·', '˙', '°']), MIST)
    }
}
