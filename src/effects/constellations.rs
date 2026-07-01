//! Constellations: a quiet star field that twinkles, and on each beat **draws a
//! constellation** — lines connecting a handful of stars — that fades between
//! hits. Stars sit at fixed seeded positions; the current figure is chosen from
//! the frame so it needs no persistent state.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

const STAR: Color = Color::Rgb(0xcf, 0xd6, 0xff);
const LINE: Color = Color::Rgb(0x6a, 0x8c, 0xd0);
const BRIGHT: Color = Color::Rgb(0xff, 0xff, 0xff);

pub struct Constellations;

impl ThemeEffect for Constellations {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Density, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.5,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let n = noise(frame as u32 / 3, (offset * 80.0) as u32) % 100;
        scale(
            base,
            (0.5 + 0.3 * (n as f64 / 100.0) + beat as f64 * 0.4).min(1.0),
        )
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 8 || h < 8 {
            return;
        }
        let frame = ctx.frame as u32;
        let beat = (ctx.beat * ctx.tuning.beat_sync).clamp(0.0, 1.0);
        let n_stars = 16 + (ctx.tuning.density * 60.0) as u32;
        // Fixed star position for index k.
        let star_pos = |k: u32| -> (u16, u16) {
            let s = noise(k.wrapping_mul(2_654_435_761), 0x57A2);
            (area.x + (s % w) as u16, area.y + (s / 7 % h) as u16)
        };
        let buf = f.buffer_mut();

        // Twinkling stars.
        for k in 0..n_stars {
            let (x, y) = star_pos(k);
            let tw = ((frame as f32 * 0.1 + k as f32).sin() * 0.5 + 0.5) as f64;
            let (ch, col) = if tw > 0.85 {
                ('✦', BRIGHT)
            } else if tw > 0.5 {
                ('+', STAR)
            } else {
                ('·', scale(STAR, 0.5 + tw * 0.4))
            };
            let cell = &mut buf[(x, y)];
            if cell.symbol() == " " {
                cell.set_char(ch);
                cell.set_fg(col);
            }
        }

        // On a beat, connect a small cluster of stars into a figure. The lines
        // fade with the pulse; the figure re-picks a few times during the hit.
        if beat > 0.3 {
            let fig = noise(frame / 5, 0xF16);
            let pts = 3 + fig % 4; // 3..6 stars
            let col = scale(LINE, beat as f64);
            let mut prev: Option<(i32, i32)> = None;
            for p in 0..pts {
                let k = noise(fig + p * 131, 0xC057) % n_stars;
                let (sx, sy) = star_pos(k);
                let cur = (sx as i32, sy as i32);
                if let Some((px, py)) = prev {
                    draw_line(
                        buf,
                        area.x,
                        area.y,
                        area.right(),
                        area.bottom(),
                        px,
                        py,
                        cur.0,
                        cur.1,
                        col,
                    );
                }
                prev = Some(cur);
            }
        }
    }
}

/// Draw a faint line between two points (DDA), on empty cells only.
#[allow(clippy::too_many_arguments)]
fn draw_line(
    buf: &mut ratatui::buffer::Buffer,
    minx: u16,
    miny: u16,
    maxx: u16,
    maxy: u16,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    col: Color,
) {
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    let steps = dx.max(dy).max(1);
    for i in 0..=steps {
        let x = x0 + (x1 - x0) * i / steps;
        let y = y0 + (y1 - y0) * i / steps;
        if x < minx as i32 || x >= maxx as i32 || y < miny as i32 || y >= maxy as i32 {
            continue;
        }
        let cell = &mut buf[(x as u16, y as u16)];
        if cell.symbol() == " " {
            let ch = if dx > dy * 2 {
                '─'
            } else if dy > dx * 2 {
                '│'
            } else if (x1 - x0).signum() == (y1 - y0).signum() {
                '╲'
            } else {
                '╱'
            };
            cell.set_char(ch);
            cell.set_fg(col);
        }
    }
}
