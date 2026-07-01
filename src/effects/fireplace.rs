//! Fireplace: a cozy hearth. A glowing ember bed flickers along the bottom edge
//! and warm embers rise + fade upward. Warm orange/red — the mellow counterpart
//! to the hacker-green Flame theme. Beats flare the fire up.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const EMBER_HOT: Color = Color::Rgb(0xff, 0xd8, 0x6a);
const EMBER_MID: Color = Color::Rgb(0xff, 0x8a, 0x2b);
const EMBER_LOW: Color = Color::Rgb(0xc4, 0x3a, 0x1e);

pub struct Fireplace;

impl ThemeEffect for Fireplace {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Speed, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            speed: 0.5,
            beat_sync: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let n = noise(frame as u32 / 2, (offset * 120.0) as u32) % 100;
        let b = (0.5 + 0.4 * (n as f64 / 100.0) + beat as f64 * 0.3).min(1.0);
        scale(base, b)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 18, 0.6, 16);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let (f, w) = (ctx.frame as u32, s.width as u32);
        let speed = ctx.tuning.speed;
        let beat = ctx.beat * ctx.tuning.beat_sync;
        let rise = |seed: u32| -(0.25 + speed * 0.4 + (seed % 3) as f32 * 0.1);
        let count = 2 + (ctx.tuning.intensity * 4.0) as u32 + (beat * 10.0) as u32;
        for i in 0..count {
            let seed = noise(f + i * 41, 0xF12E);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: (s.y + s.height - 2) as f32,
                vx: ((seed % 7) as f32 - 3.0) * 0.08,
                vy: rise(seed),
                age: 0,
                life: 12 + (seed % 10) as u16,
                seed,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        render_sparks(f, sim, area, ctx.frame as u32, ember_glyph);
        if area.height < 3 || area.width < 2 {
            return;
        }
        let frame = ctx.frame as u32;
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let buf = f.buffer_mut();
        // Glowing ember bed on the bottom row, flickering (brighter on the beat).
        let y = area.bottom() - 1;
        for x in area.x..area.right() {
            let n = noise(frame / 2 + x as u32, 0xBED) % 100;
            let glow = (n as f64 / 100.0 + beat as f64 * 0.5).min(1.0);
            let ch = if n % 3 == 0 { '▓' } else { '▒' };
            let col = scale(
                if glow > 0.6 { EMBER_HOT } else { EMBER_MID },
                0.5 + glow * 0.5,
            );
            let cell = &mut buf[(x, y)];
            cell.set_char(ch);
            cell.set_fg(col);
        }
    }
}

/// Ember glyph + color by age (0 = fresh/hot, 1 = cooling smoke).
fn ember_glyph(t: f64, seed: u32) -> (char, Color) {
    let pick = |arr: &[char]| arr[seed as usize % arr.len()];
    if t < 0.35 {
        (pick(&['▲', '✦', '●']), EMBER_HOT)
    } else if t < 0.7 {
        (pick(&['◆', '*', '·']), EMBER_MID)
    } else {
        (pick(&['·', '˙', '°']), EMBER_LOW)
    }
}
