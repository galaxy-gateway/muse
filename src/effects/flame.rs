//! Hacker-green flames: particles on navigation, a burning playhead, scope
//! shoot-offs, erratic green border flicker, and a Calcifer fireball sprite.

use ratatui::Frame;
use ratatui::style::{Color, Style};

use super::{FrameCtx, ThemeEffect, nav_sparks, render_sparks, scope_sparks};
use crate::color::scale;
use crate::particles::ParticleSim;
use crate::util::noise;

pub struct Flame;

impl ThemeEffect for Flame {
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        // Erratic green flicker on the borders.
        let n = noise(frame as u32 / 2, (offset * 131.0) as u32) % 100;
        scale(base, 0.55 + 0.45 * (n as f64 / 100.0))
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
        let frame = ctx.frame as u32;
        render_sparks(f, sim, area, frame, flame_glyph);

        let wr = ctx.wave_rect;
        let fphase = ctx.frame as f64;
        let buf = f.buffer_mut();

        // Burning playhead — a small flame cluster pinned to the play column that
        // oscillates up and down the seek line as the song plays.
        if let Some(frac) = ctx.play_frac {
            if wr.width > 2 && wr.height > 3 {
                let cx = wr.x + 1 + (frac * (wr.width - 2) as f64) as u16;
                let top = (wr.y + 1) as f64;
                let bottom = (wr.y + wr.height - 2) as f64;
                let mid = (top + bottom) / 2.0;
                let amp = (bottom - top) / 2.0;
                let yc = mid + (fphase * 0.14).sin() * amp;
                for dy in -2i32..=2 {
                    let y = (yc.round() as i32 + dy).clamp(top as i32, bottom as i32) as u16;
                    let n = noise(frame + dy as u32 * 17, cx as u32);
                    let (ch, col) = flame_glyph(dy.unsigned_abs() as f64 / 2.0, n);
                    let cell = &mut buf[(cx, y)];
                    cell.set_char(ch);
                    cell.set_fg(col);
                }
            }
        }

        // Calcifer, top-right.
        if area.width > 12 && area.height > 4 {
            let bx = area.right().saturating_sub(8);
            for (i, (text, color)) in calcifer_rows(ctx.frame).iter().enumerate() {
                let y = area.y + i as u16;
                if y < area.bottom() {
                    buf.set_string(bx, y, text, Style::default().fg(*color));
                }
            }
        }
    }
}

/// Pick a flame glyph + color by intensity `t` (0 = hot/young, 1 = cool/old).
fn flame_glyph(t: f64, seed: u32) -> (char, Color) {
    let pick = |arr: &[char]| arr[seed as usize % arr.len()];
    if t < 0.34 {
        (
            pick(&['✦', '▲', '✸', '♦', '✺']),
            Color::Rgb(0xd8, 0xff, 0x66),
        )
    } else if t < 0.67 {
        (pick(&['◆', '✧', '*', '♢']), Color::Rgb(0x39, 0xff, 0x14))
    } else {
        (pick(&['·', '˙', '°', '‧']), Color::Rgb(0x1f, 0x9d, 0x3a))
    }
}

/// Animated three-row Calcifer sprite (flame tip, blinking eyes, wiggling mouth).
fn calcifer_rows(frame: u64) -> [(String, Color); 3] {
    let tip = match (frame / 5) % 4 {
        0 => "(^^^)",
        1 => "(~^~)",
        2 => "(^~^)",
        _ => "(v^v)",
    };
    let eyes = if frame % 64 < 4 { "‐ ‐" } else { "◉ ◉" };
    let mouth = match (frame / 11) % 3 {
        0 => "\\▽/",
        1 => "\\○/",
        _ => "\\◡/",
    };
    [
        (format!(" {tip} "), Color::Rgb(0xaa, 0xff, 0x33)),
        (format!(" ({eyes}) "), Color::Rgb(0xd8, 0xff, 0x66)),
        (format!("  {mouth}  "), Color::Rgb(0x39, 0xff, 0x14)),
    ]
}
