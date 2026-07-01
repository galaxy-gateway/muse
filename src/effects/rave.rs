//! RAVE: confetti, strobe bands, fireworks on the beat, mega-explosions on click.
//! Rainbow border.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, render_sparks};
use crate::color::hue;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

pub struct Rave;

impl ThemeEffect for Rave {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Strobe, Knob::BeatSync, Knob::Speed]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.7,
            strobe: 0.6,
            beat_sync: 0.8,
            speed: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Rainbow spin with a hue kick on the beat.
        hue(frame as f64 * 0.05 + offset + beat as f64 * 0.25)
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 80, 1.6, 22); // mega-explosion
        sim.burst(ctx.frame, col, row, 40, 0.7, 30);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let s = ctx.screen;
        if s.width < 4 || s.height < 4 {
            return;
        }
        let (f, w) = (ctx.frame as u32, s.width as u32);
        // Confetti volume scales with intensity; motion with speed.
        let confetti = 1 + (ctx.tuning.intensity * 6.0) as u32;
        let spd = 0.05 + ctx.tuning.speed * 0.12;
        for i in 0..confetti {
            let seed = noise(f + i, 0xACE);
            sim.push(Spark {
                x: (s.x as u32 + seed % w) as f32,
                y: (s.y as u32 + (seed / 7) % s.height as u32) as f32,
                vx: ((seed % 9) as f32 - 4.0) * spd,
                vy: ((seed / 9 % 9) as f32 - 4.0) * spd,
                age: 0,
                life: 28,
                seed,
            });
        }
        // Fireworks on the bass beat (scaled by beat-sync).
        let bass = ctx.beat_bands[0] * ctx.tuning.beat_sync;
        if bass > 0.25 {
            let shots = 1 + (bass * 3.0) as u32;
            for j in 0..shots {
                let seed = noise(f.wrapping_add(j * 149), 0xF12E);
                let cx = s.x + (seed % w) as u16;
                let cy = s.y + (seed / 11 % (s.height as u32 / 2).max(1)) as u16;
                sim.burst(ctx.frame, cx, cy, 30, 1.0, 20);
            }
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, confetti_glyph);
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 2 || h < 2 {
            return;
        }
        let frame = ctx.frame as u32;
        // Strobing color bands: count from the strobe knob, plus a burst of extra
        // bands on the beat. `strobe = 0` disables them for a calmer rave.
        let beat = ctx.beat * ctx.tuning.beat_sync;
        let bands = (ctx.tuning.strobe * 3.0) as u32 + (beat * 4.0) as u32;
        if bands == 0 {
            return;
        }
        let buf = f.buffer_mut();
        for b in 0..bands {
            let row = noise(frame / 3 + b * 41, b * 7) % h;
            let col = hue((frame as f64 * 0.07) + b as f64 * 0.5);
            let y = area.y + row as u16;
            for c in 0..area.width {
                let cell = &mut buf[(area.x + c, y)];
                cell.set_bg(col);
            }
        }
    }
}

fn confetti_glyph(t: f64, seed: u32) -> (char, Color) {
    const G: [char; 6] = ['▪', '◆', '●', '★', '✦', '▰'];
    let ch = G[seed as usize % G.len()];
    // Vivid rainbow that also shifts as it ages.
    let col = hue((seed % 360) as f64 / 360.0 + t * 0.5);
    (ch, col)
}
