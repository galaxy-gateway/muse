//! Amber-phosphor CRT: every artifact is horizontal, nothing confetti-like.
//! A slow scanline sweep rolls down the screen, odd rows sit slightly dimmed
//! (shadow-mask lines), the corners fall off like tube glass, and bass hits
//! flicker the whole picture the way a failing tube would. Navigation leaves a
//! phosphor afterglow trail on the rows the cursor passed — drawn by tinting
//! the existing glyphs, never by painting new ones.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::rgb_of;
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

/// Phosphor tint the afterglow pulls glyphs toward.
const PHOSPHOR: (f64, f64, f64) = (255.0, 176.0, 0.0);

pub struct Crt;

impl ThemeEffect for Crt {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Intensity,
            Knob::Speed,
            Knob::Persistence,
            Knob::BeatSync,
            Knob::BeatSource,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.5,
            speed: 0.4,
            persistence: 0.5,
            beat_sync: 0.5,
            beat_source: 1.0 / 3.0, // bass drives the tube
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // A slow mains-hum breathing, brightening on bass hits. Rare dropouts
        // sell the failing-tube feel. Phase steps ~10Hz and the factor is
        // quantized so the border isn't rewritten (and shimmering) every frame.
        let t = (frame / 6) as f64 * 0.18 + offset * 2.1;
        let hum = 0.82 + 0.10 * (t * std::f64::consts::TAU).sin();
        let drop = if noise(frame as u32 / 6, (offset * 97.0) as u32).is_multiple_of(40) {
            0.55
        } else {
            1.0
        };
        let beat = crate::color::quant(beat as f64, 4.0);
        crate::color::qscale(base, (hum + beat * 0.25).min(1.15) * drop)
    }

    fn wave_tint(&self, base: Color, ctx: &FrameCtx, frac: f64, played: bool) -> Color {
        // Heat radiates from the playhead on a hit — the waveform glows like
        // the beam is burning it in. The played side stays warm phosphor.
        let pulse = crate::color::quant((ctx.routed_beat() * ctx.tuning.beat_sync) as f64, 4.0);
        let hot = if let Some(p) = ctx.play_frac {
            let d = (frac - p).abs();
            (pulse * (1.0 - d * 6.0)).max(0.0)
        } else {
            0.0
        };
        let warm = crate::color::mix(base, Color::Rgb(0xff, 0xd7, 0x66), hot * 0.8);
        if played {
            warm
        } else {
            crate::color::scale(warm, 0.7)
        }
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, _dir: f32) {
        // Phosphor afterglow: mark the cursor row; overlay tints those cells
        // toward amber as they age out. Zero velocity — afterglow stays put.
        let r = ctx.tree_rect;
        if r.width < 3 || r.height < 3 {
            return;
        }
        let vis = ctx.cursor_row.unwrap_or(0);
        let y = (r.y + 1 + vis).min(r.y + r.height - 2) as f32;
        let life = 8 + (ctx.tuning.persistence * 28.0) as u16;
        let span = r.width.saturating_sub(2);
        for i in 0..span {
            sim.push(Spark {
                x: (r.x + 1 + i) as f32,
                y,
                vx: 0.0,
                vy: 0.0,
                age: 0,
                life,
                seed: noise(ctx.frame as u32, i as u32),
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        let (w, h) = (area.width as u32, area.height as u32);
        if w < 4 || h < 4 {
            return;
        }
        let intensity = ctx.tuning.intensity as f64;
        let speed = ctx.tuning.speed as f64;
        // The routed band drives the tube (bass by default): brighten on a
        // kick. Quantized to coarse steps so the full screen is only
        // rewritten on real level changes, not on every frame of the pulse's
        // decay (that read as whole-screen flicker).
        let bass = crate::color::quant(
            (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64,
            4.0,
        );
        let global = 1.0 + bass * 0.08;

        // Rolling sweep line: a brightened band that takes a few seconds per
        // pass, faster with `speed`.
        let period = (h + 8) as f64;
        let sweep_y = (ctx.frame as f64 * (0.08 + speed * 0.45)).rem_euclid(period);

        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let buf = f.buffer_mut();
        for y in 0..h {
            // Shadow-mask: odd rows slightly darker.
            let mask = if y % 2 == 1 {
                1.0 - 0.10 * intensity
            } else {
                1.0
            };
            // Sweep proximity brightens over a 2-row falloff.
            let d = (y as f64 - sweep_y).abs();
            let sweep = if d < 2.0 {
                1.0 + (0.22 * intensity) * (1.0 - d / 2.0)
            } else {
                1.0
            };
            for x in 0..w {
                if ctx.in_thumb(area.x + x as u16, area.y + y as u16) {
                    continue; // never repaint under the cover image
                }
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                let Color::Rgb(r, g, b) = cell.fg else {
                    continue;
                };
                // Corner falloff: radial distance normalized to the half-
                // diagonal; only the outer ~40% dims, up to ~22% at the corner.
                let dx = (x as f64 + 0.5 - cx) / cx;
                let dy = (y as f64 + 0.5 - cy) / cy;
                let rad = (dx * dx + dy * dy).sqrt();
                let vig = 1.0 - (0.22 * intensity) * ((rad - 0.6).max(0.0) / 0.55).min(1.0);
                let fct = (mask * sweep * vig * global).clamp(0.3, 1.3);
                cell.set_fg(Color::Rgb(
                    ((r as f64 * fct).min(255.0)) as u8,
                    ((g as f64 * fct).min(255.0)) as u8,
                    ((b as f64 * fct).min(255.0)) as u8,
                ));
            }
        }

        // Afterglow trail: pull cell colors toward phosphor amber, strongest
        // when fresh, without touching the glyphs themselves.
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= area.bottom() as i32
            {
                continue;
            }
            let fade = 1.0 - (p.age as f64 / p.life.max(1) as f64);
            let k = fade * 0.55 * (0.4 + intensity * 0.6);
            let cell = &mut buf[(x as u16, y as u16)];
            let (r, g, b) = rgb_of(cell.fg);
            cell.set_fg(Color::Rgb(
                (r + (PHOSPHOR.0 - r) * k) as u8,
                (g + (PHOSPHOR.1 - g) * k) as u8,
                (b + (PHOSPHOR.2 - b) * k) as u8,
            ));
        }
    }
}
