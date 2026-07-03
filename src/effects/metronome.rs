//! Metronome: the calmest animated theme. One accent tick swings across the
//! transport bar, phase-locked to the detected tempo (a full left-right-left
//! swing spans two beats, so it strikes an end on every beat), with a short
//! motion trail and a bright strike flash on the hit. When locked, the BPM
//! reads out quietly at the bar's left edge. Without a lock it idles in a
//! slow frame-driven sway. No particles anywhere.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, scale};
use crate::particles::ParticleSim;

pub struct Metronome;

impl ThemeEffect for Metronome {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Speed, Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            speed: 0.4,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        // Breathe gently, brightening on the hit — never strobing. Stepped +
        // quantized so borders re-color a few times a second at most.
        let t = (frame / 6) as f64 * 0.072 + offset;
        let breathe = 0.86 + 0.08 * (t * std::f64::consts::TAU).sin();
        let beat = crate::color::quant(beat as f64, 4.0);
        crate::color::qscale(base, (breathe + beat * 0.18).min(1.05))
    }

    fn wave_tint(&self, base: Color, ctx: &FrameCtx, _frac: f64, played: bool) -> Color {
        // Anticipation: the whole waveform brightens as the next beat
        // approaches (phase rising toward 1) and relaxes right after it.
        let glow = match ctx.tempo_phase {
            Some(p) => {
                crate::color::quant((p as f64).powi(3), 8.0) * 0.35 * ctx.tuning.beat_sync as f64
            }
            None => 0.0,
        };
        let lit = scale(base, 0.85 + glow);
        if played { lit } else { scale(lit, 0.75) }
    }

    fn overlay(&self, f: &mut Frame, _sim: &ParticleSim, ctx: &FrameCtx) {
        let r = ctx.transport_rect;
        if r.width < 12 || r.height < 3 {
            return;
        }
        let y = r.y + r.height - 1; // bottom border row: a free track
        let x0 = r.x + 2;
        let span = (r.width - 5) as f64;

        // Pendulum angle: the two-beat phase when locked; a slow frame sway
        // otherwise (speed knob sets the idle rate).
        let angle = match ctx.tempo_phase2 {
            Some(p2) => p2 as f64 * std::f64::consts::TAU,
            None => {
                ctx.frame as f64 * (0.006 + ctx.tuning.speed as f64 * 0.012) * std::f64::consts::TAU
            }
        };
        let pos = (0.5 + 0.5 * angle.sin()) * span;
        // Travel direction (sign of the derivative) — the trail hangs behind.
        let dir = if angle.cos() >= 0.0 { 1.0f64 } else { -1.0 };

        // Strike flash: right at a beat (phase near 0), the tick flares.
        let strike = match ctx.tempo_phase {
            Some(p) => (p < 0.10 || p > 0.94) as u8 as f32 * ctx.tuning.beat_sync,
            None => ctx.beat * ctx.tuning.beat_sync,
        };

        let accent = Color::Rgb(0xe8, 0xc8, 0x7a);
        let flash = Color::Rgb(0xff, 0xf2, 0xc8);
        let dim = Color::Rgb(0x6a, 0x5c, 0x40);
        let intensity = ctx.tuning.intensity as f64;
        let buf = f.buffer_mut();

        // Motion trail: two ghosts hanging behind the tick's travel direction.
        for (i, k) in [(2i32, 0.25), (1, 0.5)] {
            let gx = (x0 as f64 + pos - i as f64 * dir) as i32;
            if gx >= x0 as i32 && gx < (x0 as f64 + span) as i32 + 1 {
                let cell = &mut buf[(gx as u16, y)];
                cell.set_char('─');
                cell.set_fg(scale(dim, 0.5 + k * intensity));
            }
        }

        // The tick itself.
        let tx = (x0 as f64 + pos) as u16;
        let cell = &mut buf[(tx, y)];
        cell.set_char(if strike > 0.3 { '◆' } else { '▼' });
        cell.set_fg(mix(accent, flash, strike as f64));

        // Quiet BPM readout at the left of the bottom border when locked.
        if let Some(bpm) = ctx.bpm {
            let label = format!(" {} bpm ", bpm.round() as u32);
            let lx = r.x + 2;
            if lx + label.len() as u16 + 2 < tx.max(r.x + 20) || tx < lx {
                for (i, ch) in label.chars().enumerate() {
                    let cell = &mut buf[(lx + i as u16, y)];
                    cell.set_char(ch);
                    cell.set_fg(dim);
                }
            }
        }
    }
}
