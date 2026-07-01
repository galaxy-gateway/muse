//! Electric: spark particles on nav/click/scope, crackling cyan/white border
//! flicker, and lightning bolts that strike inside the oscilloscope on loud peaks.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning, nav_sparks, render_sparks, scope_sparks};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

pub struct Electric;

impl ThemeEffect for Electric {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::BeatSync, Knob::Speed]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            beat_sync: 0.7,
            speed: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Crackling cyan/white flicker; a beat forces a bright white arc.
        let n = noise(frame as u32, (offset * 173.0) as u32) % 100;
        if beat > 0.4 || n < 6 {
            Color::Rgb(0xff, 0xff, 0xff)
        } else if n < 14 {
            Color::Rgb(0x9d, 0xf0, 0xff)
        } else {
            scale(base, 0.5 + 0.5 * (n as f64 / 100.0))
        }
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {
        nav_sparks(sim, ctx, dir);
    }

    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 22, 0.7, 12);
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        scope_sparks(sim, ctx);
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 4 || area.height < 4 {
            return;
        }
        let frame = ctx.frame as u32;
        let sr = ctx.scope_rect;
        // Bass beat (scaled by beat-sync) triggers the strikes; intensity adds bolts.
        let bass = ctx.beat_bands[0] * ctx.tuning.beat_sync;
        render_sparks(f, sim, area, frame, spark_glyph);

        // Lightning bolts strike inside the oscilloscope on a bass beat.
        if sr.width > 3 && sr.height > 3 && bass > 0.25 {
            let buf = f.buffer_mut();
            let bolts = 1 + (bass * (1.0 + ctx.tuning.intensity * 3.0)) as u32;
            let lo = (sr.x + 1) as i32;
            let hi = (sr.x + sr.width - 2) as i32;
            for b in 0..bolts {
                let seed = noise(frame / 2, b * 53 + 11);
                let mut x = lo + (seed % (sr.width - 2).max(1) as u32) as i32;
                for y in (sr.y + 1)..(sr.y + sr.height - 1) {
                    let n = noise(seed + y as u32, frame);
                    let dx = (n % 3) as i32 - 1;
                    x = (x + dx).clamp(lo, hi);
                    let ch = match dx {
                        d if d < 0 => '╲',
                        d if d > 0 => '╱',
                        _ => '│',
                    };
                    let col = if n % 4 == 0 {
                        Color::Rgb(0xff, 0xff, 0xff)
                    } else {
                        Color::Rgb(0x9d, 0xf0, 0xff)
                    };
                    let cell = &mut buf[(x as u16, y)];
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        }
    }
}

/// Electric spark glyph + color by age (0 = hot/white bolt, 1 = fading blue).
fn spark_glyph(t: f64, seed: u32) -> (char, Color) {
    let pick = |arr: &[char]| arr[seed as usize % arr.len()];
    if t < 0.4 {
        (pick(&['↯', '⚡', '✦', '+']), Color::Rgb(0xff, 0xff, 0xff))
    } else if t < 0.7 {
        (pick(&['✦', '×', '+', '·']), Color::Rgb(0x4d, 0xd2, 0xff))
    } else {
        (pick(&['·', '˙', '+']), Color::Rgb(0x2b, 0x8c, 0xff))
    }
}
