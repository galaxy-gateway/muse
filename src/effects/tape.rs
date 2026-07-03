//! Cassette deck: the waveform panel wobbles with wow & flutter (slow pitch
//! drift + fast jitter as small horizontal row shifts), blank cells carry a
//! faint tape-hiss dither, and switching tracks smears a fast horizontal
//! rewind streak across the browser. Warm oxide-brown palette; the border
//! drifts in brightness like a motor that never quite holds speed.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::scale;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

const HISS: Color = Color::Rgb(0x5a, 0x4a, 0x38);
const SMEAR: Color = Color::Rgb(0xe8, 0xd0, 0xa8);

pub struct Tape;

impl ThemeEffect for Tape {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Intensity,
            Knob::Speed,
            Knob::Disruption,
            Knob::BeatSync,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.5,
            speed: 0.5,
            disruption: 0.45,
            beat_sync: 0.4,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        // Steady frame — animated border brightness re-colors every panel
        // edge in lockstep and reads as the palette flashing. The tape
        // character lives in the overlay (wobble, hiss, smear).
        crate::color::qscale(base, 0.9)
    }

    fn on_track(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        // Rewind smear: fast horizontal streaks shooting across the browser,
        // alternating direction per row so it reads as tape flying past a head.
        let r = ctx.tree_rect;
        if r.width < 6 || r.height < 4 {
            return;
        }
        let rows = (r.height - 2).min(14);
        for i in 0..rows {
            let seed = noise(ctx.frame as u32, i as u32 * 31);
            let y = r.y + 1 + (seed % (r.height - 2) as u32) as u16;
            let leftward = seed & 1 == 0;
            let (x0, vx) = if leftward {
                ((r.right() - 2) as f32, -(3.0 + (seed % 4) as f32))
            } else {
                ((r.x + 1) as f32, 3.0 + (seed % 4) as f32)
            };
            sim.push(Spark {
                x: x0,
                y: y as f32,
                vx,
                vy: 0.0,
                age: 0,
                life: 10 + (seed % 6) as u16,
                seed,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 4 || area.height < 4 {
            return;
        }
        let intensity = ctx.tuning.intensity as f64;
        let speed = ctx.tuning.speed as f64;
        let disruption = ctx.tuning.disruption as f64;
        let beat = (ctx.beat * ctx.tuning.beat_sync) as f64;
        let buf = f.buffer_mut();

        // --- Wow & flutter: shift each waveform row horizontally by a slow
        // wander plus a fast jitter; a beat kicks the flutter briefly. ---
        let wr = ctx.wave_rect;
        if wr.width > 6 && wr.height > 3 {
            let t = ctx.frame as f64 * (0.02 + speed * 0.06);
            let amp = disruption * 1.9 + beat * 1.2;
            for y in (wr.y + 1)..(wr.bottom() - 1) {
                let wow = (t + y as f64 * 0.35).sin();
                let flutter = 0.4 * (t * 4.7 + y as f64 * 1.3).sin();
                let shift = ((wow + flutter) * amp).round() as i32;
                if shift != 0 {
                    shift_row(buf, wr, y, shift);
                }
            }
        }

        // --- Tape hiss: sparse dim specks on blank cells only, re-rolled a
        // few times a second (per-frame rerolls read as harsh noise). Never
        // overwrites real glyphs or the cover image. ---
        let (w, h) = (area.width as u32, area.height as u32);
        let specks = (w * h / 90) * (1 + (intensity * 3.0) as u32);
        for k in 0..specks {
            let s = noise(ctx.frame as u32 / 8, k.wrapping_mul(0x9E37_79B9));
            let x = area.x + (s % w) as u16;
            let y = area.y + (s / 7 % h) as u16;
            if ctx.in_thumb(x, y) {
                continue;
            }
            let cell = &mut buf[(x, y)];
            if cell.symbol() == " " {
                cell.set_char(if s & 8 == 0 { '·' } else { '.' });
                cell.set_fg(scale(HISS, 0.5 + (s % 5) as f64 * 0.12));
            }
        }

        // --- Rewind streaks: long dashes with a fading tail. ---
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= area.bottom() as i32
            {
                continue;
            }
            let fade = crate::color::quant(1.0 - p.age as f64 / p.life.max(1) as f64, 8.0);
            let cell = &mut buf[(x as u16, y as u16)];
            cell.set_char(if p.age % 3 == 0 { '╌' } else { '─' });
            cell.set_fg(scale(SMEAR, 0.35 + fade * 0.65));
        }
    }
}

/// Shift one row's glyphs+colors horizontally by `shift` cells (positive =
/// right), clearing the vacated edge. Confined to `r`'s inner columns.
fn shift_row(buf: &mut Buffer, r: Rect, y: u16, shift: i32) {
    let x0 = r.x + 1;
    let w = (r.width - 2) as i32;
    let row: Vec<(String, Color)> = (0..w)
        .map(|i| {
            let c = &buf[(x0 + i as u16, y)];
            (c.symbol().to_string(), c.fg)
        })
        .collect();
    for i in 0..w {
        let cell = &mut buf[(x0 + i as u16, y)];
        let src = i - shift;
        if src >= 0 && src < w {
            let (sym, fg) = &row[src as usize];
            cell.set_symbol(sym);
            cell.set_fg(*fg);
        } else {
            cell.set_char(' ');
        }
    }
}
