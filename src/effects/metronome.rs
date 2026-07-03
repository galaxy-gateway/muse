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

/// (tick accent, strike flash, trail/label dim) per colorway.
const COLORWAYS: [(Color, Color, Color); 3] = [
    (
        Color::Rgb(0xe8, 0xc8, 0x7a),
        Color::Rgb(0xff, 0xf2, 0xc8),
        Color::Rgb(0x6a, 0x5c, 0x40),
    ), // brass
    (
        Color::Rgb(0xd0, 0xd8, 0xe0),
        Color::Rgb(0xff, 0xff, 0xff),
        Color::Rgb(0x5a, 0x60, 0x6a),
    ), // silver
    (
        Color::Rgb(0xd8, 0x8a, 0x6a),
        Color::Rgb(0xff, 0xd8, 0xc0),
        Color::Rgb(0x6a, 0x44, 0x38),
    ), // rosewood
];

fn colorway(t: &Tuning) -> (Color, Color, Color) {
    COLORWAYS[t.choice(Knob::Palette).min(2)]
}

pub struct Metronome;

impl ThemeEffect for Metronome {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Palette, Knob::Intensity, Knob::Speed, Knob::BeatSync]
    }

    fn palette_names(&self) -> &'static [&'static str] {
        &["brass", "silver", "rosewood"]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            speed: 0.4,
            beat_sync: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        // Steady frame — animated border brightness re-colors every panel
        // edge in lockstep and reads as the palette flashing; this theme's
        // character lives in its overlay.
        crate::color::qscale(base, 0.88)
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

        let (accent, flash, dim) = colorway(&ctx.tuning);
        let intensity = ctx.tuning.intensity as f64;
        let buf = f.buffer_mut();

        // --- The arm: a full-height pendulum hung from the top of the
        // screen, its bob sweeping just above the transport bar. Drawn by
        // TINTING the glyphs it crosses (texture dots on blanks) so the file
        // tree stays readable — one thin moving line, everything else
        // untouched. The transport tick below is its echo. ---
        let s = ctx.screen;
        if s.height > 14 {
            let px = s.x as f64 + s.width as f64 / 2.0;
            let py = s.y as f64 + 1.0;
            let len = ctx.transport_rect.y.saturating_sub(s.y + 3) as f64;
            let theta = 0.55 * angle.sin(); // radians off vertical
            let (adx, ady) = (theta.sin() * 2.0, theta.cos()); // cell aspect
            let steps = (len as u32 * 2).max(8);
            for i in 4..steps {
                let t = i as f64 / steps as f64;
                let x = (px + adx * len * t).round() as i32;
                let yy = (py + ady * len * t).round() as i32;
                if x < s.x as i32
                    || x >= s.right() as i32
                    || yy < s.y as i32
                    || yy >= ctx.transport_rect.y as i32
                    || ctx.in_thumb(x as u16, yy as u16)
                {
                    continue;
                }
                let cell = &mut buf[(x as u16, yy as u16)];
                if cell.symbol() == " " {
                    cell.set_char('·');
                    cell.set_fg(scale(accent, (0.30 + t * 0.45) * (0.5 + intensity * 0.5)));
                } else {
                    let (r, g, b) = crate::color::rgb_of(cell.fg);
                    let (ar, ag, ab) = crate::color::rgb_of(accent);
                    let m = (0.30 + t * 0.30) * (0.5 + intensity * 0.5);
                    cell.set_fg(Color::Rgb(
                        (r + (ar - r) * m) as u8,
                        (g + (ag - g) * m) as u8,
                        (b + (ab - b) * m) as u8,
                    ));
                }
            }
            // The bob, flaring on the strike; the pivot above it all.
            let bx = (px + adx * len).round() as i32;
            let by = (py + ady * len).round() as i32;
            if bx >= s.x as i32
                && bx < s.right() as i32
                && by >= s.y as i32
                && by < ctx.transport_rect.y as i32
            {
                let cell = &mut buf[(bx as u16, by as u16)];
                cell.set_char(if strike > 0.3 { '◉' } else { '●' });
                cell.set_fg(mix(accent, flash, strike as f64));
            }
            let cell = &mut buf[(px.round() as u16, py as u16)];
            cell.set_char('┰');
            cell.set_fg(dim);
        }

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
