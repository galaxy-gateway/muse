//! Seismograph: a three-channel strip-chart recorder. Bass, mid, and treble
//! each drive their own pen (red, amber, slate inks) on stacked baselines,
//! tracing onto paper that scrolls right-to-left across the whole screen; the
//! particle sim is the drum memory. A time grid scrolls with the paper,
//! channel labels sit at the left margin, switching tracks stamps a dashed
//! event line across all three channels, and navigation knocks every pen —
//! hardest at the top — ringing down over the next few marks.

use ratatui::Frame;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{quant, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

/// Channel labels (fixed) and per-colorway ink sets: three (wet, dry) pairs
/// plus the paper grid color.
const LABELS: [&str; 3] = ["bass", "mid", "treble"];
const COLORWAYS: [([(Color, Color); 3], Color); 3] = [
    (
        [
            (Color::Rgb(0xd8, 0x3a, 0x2a), Color::Rgb(0x8a, 0x2e, 0x24)),
            (Color::Rgb(0xd8, 0xa0, 0x2a), Color::Rgb(0x8a, 0x6a, 0x24)),
            (Color::Rgb(0x8a, 0xa0, 0xb8), Color::Rgb(0x5a, 0x6a, 0x7a)),
        ],
        Color::Rgb(0x3a, 0x34, 0x2c),
    ), // classic
    (
        [
            (Color::Rgb(0xe8, 0xf0, 0xff), Color::Rgb(0x9a, 0xa8, 0xc0)),
            (Color::Rgb(0x5a, 0xc8, 0xff), Color::Rgb(0x3a, 0x80, 0xa8)),
            (Color::Rgb(0xb0, 0xc8, 0xe0), Color::Rgb(0x6a, 0x80, 0x9a)),
        ],
        Color::Rgb(0x2a, 0x3c, 0x55),
    ), // blueprint
    (
        [
            (Color::Rgb(0xff, 0x4a, 0x2a), Color::Rgb(0xa0, 0x32, 0x1e)),
            (Color::Rgb(0xff, 0x8a, 0x2a), Color::Rgb(0xa8, 0x5c, 0x1e)),
            (Color::Rgb(0xff, 0xd0, 0x4a), Color::Rgb(0xa8, 0x8a, 0x30)),
        ],
        Color::Rgb(0x40, 0x30, 0x20),
    ), // thermal
];

fn colorway(t: &Tuning) -> &'static ([(Color, Color); 3], Color) {
    &COLORWAYS[t.choice(Knob::Palette).min(2)]
}

/// Spark seed layout: bits 0..7 deflection (+32), bits 8..9 channel,
/// bit 10 marks a track-change event stamp.
fn encode(defl: i32, ch: u32) -> u32 {
    ((defl + 32).clamp(0, 64) as u32) | (ch << 8)
}

fn decode(seed: u32) -> (i32, usize) {
    ((seed & 0x7f) as i32 - 32, (seed >> 8 & 0x3) as usize)
}

const EVENT: u32 = 1 << 10;

fn paper_speed(t: &Tuning) -> f32 {
    0.25 + t.speed * 0.9
}

/// Baseline row for channel `ch`: stacked at 28% / 52% / 76% of the screen.
fn base_y(ctx: &FrameCtx, ch: usize) -> u16 {
    let s = ctx.screen;
    s.y + (s.height as u32 * (28 + ch as u32 * 24) / 100) as u16
}

pub struct Seismograph;

impl ThemeEffect for Seismograph {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::Intensity, Knob::Speed, Knob::Persistence]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            intensity: 0.6,
            speed: 0.45,
            persistence: 0.6,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        crate::color::qscale(base, 0.9)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 20 || area.height < 14 {
            return;
        }
        let paper = paper_speed(&ctx.tuning);
        let life = ((area.width as f32 - 3.0) / paper) as u16;
        // Keep three full-width traces under the global particle cap: stretch
        // the mark stride when the paper is slow (long lives) so the drum
        // never eats its own tail.
        let stride = (1 + (3 * life as u32) / 600).min(4) as u64;
        if !ctx.frame.is_multiple_of(stride) {
            return;
        }
        let gain = 2.0 + ctx.tuning.intensity * 6.0;
        for ch in 0..3usize {
            // The pen wobbles organically: two incommensurate sines per
            // channel carry the deflection, scaled by that band's live pulse
            // (plus raw level for bass so quiet passages still breathe).
            let level = ctx.beat_bands[ch] + if ch == 0 { ctx.scope_peak * 0.6 } else { 0.0 };
            let t = ctx.frame as f64 * (0.11 + ch as f64 * 0.03);
            let wave = (t.sin() + 0.5 * (t * 1.73 + ch as f64).sin()) / 1.5;
            let jitter = ((noise(ctx.frame as u32, ch as u32 * 91) % 100) as f64 - 50.0) * 0.004;
            let defl = ((level * gain) as f64 * (wave + jitter) * 1.2).round() as i32;
            sim.push(Spark {
                x: (area.right() - 2) as f32,
                y: base_y(ctx, ch) as f32,
                vx: -paper,
                vy: 0.0,
                age: 0,
                life: life.max(10),
                seed: encode(defl.clamp(-6, 6), ch as u32),
            });
        }
        sim.cap();
    }

    fn on_track(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        // Event stamp: a dashed line across all channels, inked into the drum.
        let area = ctx.screen;
        if area.width < 20 || area.height < 14 {
            return;
        }
        let paper = paper_speed(&ctx.tuning);
        sim.push(Spark {
            x: (area.right() - 2) as f32,
            y: base_y(ctx, 1) as f32,
            vx: -paper,
            vy: 0.0,
            age: 0,
            life: (((area.width as f32 - 3.0) / paper) as u16).max(10),
            seed: EVENT,
        });
        sim.cap();
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {
        // The table takes a knock: every pen spikes, hardest on the bass
        // channel, ringing down over the next few marks.
        let area = ctx.screen;
        if area.width < 20 || area.height < 14 {
            return;
        }
        let paper = paper_speed(&ctx.tuning);
        let life = (((area.width as f32 - 3.0) / paper) as u16).max(10);
        for ch in 0..3usize {
            let scale_ch = [1.0f32, 0.6, 0.35][ch];
            for (i, mag) in [6i32, -4, 2, -1].into_iter().enumerate() {
                let defl = ((if dir >= 0.0 { mag } else { -mag }) as f32 * scale_ch) as i32;
                if defl == 0 {
                    continue;
                }
                sim.push(Spark {
                    x: (area.right() - 2 - i as u16) as f32,
                    y: base_y(ctx, ch) as f32,
                    vx: -paper,
                    vy: 0.0,
                    age: 0,
                    life,
                    seed: encode(defl, ch as u32),
                });
            }
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 20 || area.height < 14 {
            return;
        }
        let fade_floor = 0.25 + ctx.tuning.persistence as f64 * 0.65;
        let paper = paper_speed(&ctx.tuning) as f64;
        let (inks, grid) = colorway(&ctx.tuning);
        let buf = f.buffer_mut();

        // Time grid scrolling with the paper (whole-cell steps): a dim tick
        // column every 14 cells, drawn on blank cells across the drum band.
        let band_top = base_y(ctx, 0).saturating_sub(7);
        let band_bot = (base_y(ctx, 2) + 7).min(area.bottom() - 2);
        let offset = (ctx.frame as f64 * paper) as i32;
        for x in (area.x + 1)..(area.right() - 1) {
            if (x as i32 + offset) % 14 == 0 {
                for y in band_top..=band_bot {
                    let cell = &mut buf[(x, y)];
                    if cell.symbol() == " " {
                        cell.set_char('┊');
                        cell.set_fg(scale(*grid, 0.8));
                    }
                }
            }
        }

        // Baselines + channel labels at the left margin.
        for (ch, &(ink, _)) in inks.iter().enumerate() {
            let y = base_y(ctx, ch);
            for x in (area.x + 1)..(area.right() - 1) {
                let cell = &mut buf[(x, y)];
                if cell.symbol() == " " {
                    cell.set_char('┄');
                    cell.set_fg(*grid);
                }
            }
            for (i, c) in LABELS[ch].chars().enumerate() {
                let x = area.x + 2 + i as u16;
                let cell = &mut buf[(x, y.saturating_sub(1))];
                if cell.symbol() == " " {
                    cell.set_char(c);
                    cell.set_fg(scale(ink, 0.55));
                }
            }
        }

        // The ink.
        for p in &sim.sparks {
            let x = p.x.round() as i32;
            if x <= area.x as i32 || x >= area.right() as i32 - 1 {
                continue;
            }
            if p.seed & EVENT != 0 {
                // Track-change stamp: dashed vertical through all channels —
                // drawn on blank cells, tinting any text it crosses instead.
                for y in band_top..=band_bot {
                    if (y - band_top).is_multiple_of(2) {
                        ink_cell(
                            buf,
                            x as u16,
                            y,
                            '╎',
                            scale(Color::Rgb(0xe0, 0xd8, 0xc4), 0.5),
                        );
                    }
                }
                continue;
            }
            let (defl, ch) = decode(p.seed);
            let (wet_ink, dry_ink) = inks[ch.min(2)];
            let base = base_y(ctx, ch.min(2));
            let age_f = p.age as f64 / p.life.max(1) as f64;
            let wet = p.age < 3;
            let fade = quant((1.0 - age_f * (1.0 - fade_floor)).clamp(0.2, 1.0), 8.0);
            let col = if wet { wet_ink } else { scale(dry_ink, fade) };
            let tip = (base as i32 - defl).clamp(area.y as i32 + 1, area.bottom() as i32 - 2);
            let (lo, hi) = (tip.min(base as i32), tip.max(base as i32));
            for y in lo..=hi {
                let g = if y == tip {
                    if wet { '●' } else { '•' }
                } else {
                    '│'
                };
                ink_cell(buf, x as u16, y as u16, g, col);
            }
        }
    }
}

/// Ink one cell: draw the stroke glyph on blank cells (and over our own paper
/// furniture), but only TINT the foreground of real text — the drum crosses
/// the file tree, and replacing glyphs there made filenames unreadable.
fn ink_cell(buf: &mut ratatui::buffer::Buffer, x: u16, y: u16, g: char, col: Color) {
    let cell = &mut buf[(x, y)];
    match cell.symbol() {
        " " | "┄" | "┊" | "·" | "│" | "•" | "●" | "╎" => {
            cell.set_char(g);
            cell.set_fg(col);
        }
        _ => {
            let (r, gr, b) = crate::color::rgb_of(cell.fg);
            let (ir, ig, ib) = crate::color::rgb_of(col);
            cell.set_fg(Color::Rgb(
                (r + (ir - r) * 0.6) as u8,
                (gr + (ig - gr) * 0.6) as u8,
                (b + (ib - b) * 0.6) as u8,
            ));
        }
    }
}
