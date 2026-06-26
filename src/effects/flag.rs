//! A small stars-and-stripes in the top-right of the now-playing panel, plus
//! red/white/blue click-fireworks. Border stays static.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, ThemeEffect, render_sparks};
use crate::particles::ParticleSim;

pub struct Flag;

impl ThemeEffect for Flag {
    fn on_click(&self, sim: &mut ParticleSim, ctx: &FrameCtx, col: u16, row: u16) {
        sim.burst(ctx.frame, col, row, 26, 0.7, 14);
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        draw_flag(f, ctx.np_rect);
        render_sparks(f, sim, ctx.screen, ctx.frame as u32, firework_glyph);
    }
}

fn draw_flag(f: &mut Frame, panel: Rect) {
    let red = Color::Rgb(0xb2, 0x22, 0x34);
    let white = Color::Rgb(0xf2, 0xf2, 0xf2);
    let blue = Color::Rgb(0x3c, 0x3b, 0x6e);
    if panel.width < 12 || panel.height < 4 {
        return;
    }
    let inner_w = panel.width - 2;
    let h = panel.height - 2;
    // Small but detailed: ~30% of the inner width, top-right corner.
    let fw = ((inner_w as u32 * 12 / 40) as u16).clamp(9, inner_w);
    let fx = panel.x + 1 + (inner_w - fw);
    let y0 = panel.y + 1;
    let canton_w = (fw / 2).max(5);
    // Taller blue field so it holds at least two rows of stars.
    let canton_h = (h * 2 / 3).max(2).min(h);
    // Half-block stripes: 2 stripes per cell row -> ~13-stripe look in a few
    // rows. Stripe 0 (top) is red, alternating down.
    let stripe = |sub: i32| if sub.rem_euclid(2) == 0 { red } else { white };
    let buf = f.buffer_mut();
    for r in 0..h {
        for c in 0..fw {
            let cell = &mut buf[(fx + c, y0 + r)];
            if r < canton_h && c < canton_w {
                // Solid blue canton with a sparse, row-offset star field.
                cell.set_bg(blue);
                if (c + r) % 2 == 0 {
                    cell.set_char('★');
                    cell.set_fg(white);
                } else {
                    cell.set_char(' ');
                }
            } else {
                cell.set_char('▀');
                cell.set_fg(stripe(2 * r as i32));
                cell.set_bg(stripe(2 * r as i32 + 1));
            }
        }
    }
}

/// Firework spark glyph + a red/white/blue color per particle.
fn firework_glyph(t: f64, seed: u32) -> (char, Color) {
    let pick = |arr: &[char]| arr[seed as usize % arr.len()];
    let col = match (seed / 5) % 3 {
        0 => Color::Rgb(0xe0, 0x3b, 0x4a),
        1 => Color::Rgb(0xff, 0xff, 0xff),
        _ => Color::Rgb(0x5a, 0x7b, 0xff),
    };
    let ch = if t < 0.5 {
        pick(&['✦', '✶', '*', '+'])
    } else {
        pick(&['·', '˙', '✧'])
    };
    (ch, col)
}
