//! Bamboo moon: a terafox-teal night forest. A crescent hangs in the upper
//! right and lights the panels around it, stars fill the blank sky, mist drifts
//! across the lower third, spirit motes wander the screen, and moonlight
//! travels along the waveform.
//!
//! The magic layers are individually switchable: the ones with a natural
//! magnitude ride existing Range knobs where 0 = off (motes on `Density`, mist
//! on `Wind`), and the binary ones get Toggle knobs (`Glow`, `Shimmer`,
//! `Glint`). The moon itself is always on — it's the theme's identity.
//!
//! Flicker discipline (docs/themes.md): the crescent is identical every frame,
//! the glow uses constant per-cell factors with no time term, and the stars,
//! mist and motes animate on stepped clocks with quantized brightness. Nothing
//! here modulates the border or the whole screen on the beat.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{FrameCtx, Knob, ThemeEffect, Tuning};
use crate::color::{mix, qscale, quant, rgb_of, scale};
use crate::particles::{ParticleSim, Spark};
use crate::util::noise;

/// (moon, mote, warm mote, mist, glint) per colorway.
const COLORWAYS: [(Color, Color, Color, Color, Color); 3] = [
    (
        Color::Rgb(0xdc, 0xe8, 0xee),
        Color::Rgb(0xa1, 0xcd, 0xd8),
        Color::Rgb(0xfd, 0xa4, 0x7f),
        Color::Rgb(0x58, 0x7b, 0x7b),
        Color::Rgb(0xa1, 0xcd, 0xd8),
    ), // moonlit
    (
        Color::Rgb(0xff, 0xe0, 0xc0),
        Color::Rgb(0xfd, 0xa4, 0x7f),
        Color::Rgb(0xff, 0xd7, 0x8a),
        Color::Rgb(0x6d, 0x7f, 0x8b),
        Color::Rgb(0xff, 0x83, 0x49),
    ), // lantern
    (
        Color::Rgb(0xf0, 0xdc, 0xe6),
        Color::Rgb(0xcb, 0x79, 0x85),
        Color::Rgb(0xfd, 0xa4, 0x7f),
        Color::Rgb(0x6d, 0x7f, 0x8b),
        Color::Rgb(0xad, 0x5c, 0x7c),
    ), // plum blossom
];

fn colorway(t: &Tuning) -> (Color, Color, Color, Color, Color) {
    COLORWAYS[t.choice(Knob::Palette).min(2)]
}

/// Terminal cells are about twice as tall as they are wide, so a round moon
/// needs its horizontal distances halved.
const ASPECT: f64 = 2.0;

/// Moon geometry for this screen: (centre x, centre y, vertical radius), or
/// `None` when the screen is too small to hang a moon in.
fn moon_geom(ctx: &FrameCtx) -> Option<(f64, f64, f64)> {
    let a = ctx.screen;
    if a.width < 24 || a.height < 10 {
        return None;
    }
    let rv = (a.height as f64 / 4.0).clamp(3.0, 6.0);
    let cx = a.right() as f64 - rv * ASPECT - 3.0;
    let cy = a.y as f64 + rv + 1.0;
    Some((cx, cy, rv))
}

/// The disc that carves the crescent out of the moon: offset to the right and
/// very slightly smaller, so what survives is a thin lune with sharp horns.
/// The offset is in moon units, hence the `ASPECT` back-conversion for x.
fn occluder(cx: f64, cy: f64, rv: f64) -> (f64, f64, f64) {
    (cx + rv * 0.5 * ASPECT, cy - rv * 0.12, rv * 0.92)
}

/// Distance from (x,y) to a moon-space centre, in vertical-radius units.
fn moon_dist(x: u16, y: u16, cx: f64, cy: f64) -> f64 {
    let nx = (x as f64 - cx) / ASPECT;
    let ny = y as f64 - cy;
    (nx * nx + ny * ny).sqrt()
}

/// Whether a cell is open sky: blank, with blank cells either side. A lone
/// space between two words is a gap in the UI, not a piece of night — dropping
/// a star into one renders "a walk in a park at✦night.mp3".
fn open_sky(buf: &ratatui::buffer::Buffer, area: Rect, x: u16, y: u16) -> bool {
    if buf[(x, y)].symbol() != " " {
        return false;
    }
    let left = x > area.x && buf[(x - 1, y)].symbol() == " ";
    let right = x + 1 < area.right() && buf[(x + 1, y)].symbol() == " ";
    left && right
}

/// Blend a cell's existing foreground toward `col` by `m`, leaving the glyph
/// alone — moonlight and mist fall ON the UI, they don't replace it.
fn tint_fg(cell: &mut ratatui::buffer::Cell, col: Color, m: f64) {
    let (r, g, b) = rgb_of(cell.fg);
    let (tr, tg, tb) = rgb_of(col);
    cell.set_fg(Color::Rgb(
        (r + (tr - r) * m) as u8,
        (g + (tg - g) * m) as u8,
        (b + (tb - b) * m) as u8,
    ));
}

pub struct BambooMoon;

impl ThemeEffect for BambooMoon {
    fn knobs(&self) -> &'static [Knob] {
        &[
            Knob::Density,
            Knob::Speed,
            Knob::Wind,
            Knob::BeatSync,
            Knob::BeatSource,
            Knob::Glow,
            Knob::Shimmer,
            Knob::Glint,
        ]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            density: 0.45,
            speed: 0.3,
            wind: 0.35,
            beat_sync: 0.35,
            beat_source: 1.0 / 3.0, // bass — slow swells suit a still night
            glow: 1.0,
            shimmer: 1.0,
            glint: 1.0,
            ..Default::default()
        }
    }

    fn border(&self, base: Color, _frame: u64, _offset: f64, _beat: f32) -> Color {
        // Steady: any border modulation re-colors every panel edge in lockstep
        // and reads as the palette flashing. This theme lives in its overlay.
        qscale(base, 0.85)
    }

    fn ambient(&self, sim: &mut ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 6 {
            return;
        }
        // Density is the motes' off switch as well as their amount.
        if ctx.tuning.density < 0.02 {
            return;
        }
        let target = 4 + (ctx.tuning.density * 26.0) as usize;
        let max_v = 0.04 + ctx.tuning.speed * 0.16;

        // Steer the living: slow wander plus edge shyness. Motes drift like
        // something half-asleep, so the noise clock is slower than fireflies'.
        for (i, p) in sim.sparks.iter_mut().enumerate() {
            let s = noise(ctx.frame as u32 / 4, i as u32 * 71 + p.seed);
            p.vx += ((s % 7) as f32 - 3.0) * 0.006;
            p.vy += ((s / 7 % 7) as f32 - 3.0) * 0.005;
            if p.x < (area.x + 2) as f32 {
                p.vx += 0.025;
            }
            if p.x > (area.right() - 3) as f32 {
                p.vx -= 0.025;
            }
            if p.y < (area.y + 2) as f32 {
                p.vy += 0.025;
            }
            if p.y > (area.bottom() - 3) as f32 {
                p.vy -= 0.025;
            }
            p.vx = p.vx.clamp(-max_v, max_v);
            p.vy = p.vy.clamp(-max_v * 0.7, max_v * 0.7);
        }

        if sim.sparks.len() < target && ctx.frame.is_multiple_of(5) {
            let s = noise(ctx.frame as u32, sim.sparks.len() as u32 * 131);
            sim.push(Spark {
                x: (area.x + 2 + (s % (area.width as u32 - 4)) as u16) as f32,
                y: (area.y + 2 + (s / 7 % (area.height as u32 - 4)) as u16) as f32,
                vx: 0.0,
                vy: 0.0,
                age: 0,
                life: 500 + (s % 400) as u16,
                seed: s,
            });
        }
        sim.cap();
    }

    fn on_nav(&self, sim: &mut ParticleSim, ctx: &FrameCtx, dir: f32) {
        // Moving the cursor stirs a few motes loose from the row.
        if ctx.tuning.density < 0.02 {
            return;
        }
        let r = ctx.tree_rect;
        if r.width < 4 || r.height < 4 {
            return;
        }
        let vis = ctx.cursor_row.unwrap_or(0);
        let y = (r.y + 1 + vis).min(r.y + r.height - 2) as f32;
        let span = r.width.saturating_sub(2).max(1) as u32;
        for i in 0..5u32 {
            let s = noise(ctx.frame as u32, i + ctx.cursor_index.unwrap_or(0) * 7);
            sim.push(Spark {
                x: (r.x + 1) as f32 + (s % span) as f32,
                y,
                vx: ((s % 5) as f32 - 2.0) * 0.03,
                vy: dir * 0.06,
                age: 0,
                life: 90 + (s % 60) as u16,
                seed: s,
            });
        }
        sim.cap();
    }

    fn overlay(&self, f: &mut Frame, sim: &ParticleSim, ctx: &FrameCtx) {
        let area = ctx.screen;
        if area.width < 8 || area.height < 6 {
            return;
        }
        let (moon_c, mote_c, warm_c, mist_c, _) = colorway(&ctx.tuning);
        let geom = moon_geom(ctx);

        // --- stars: fixed seeded positions in the blank sky ------------------
        // Positions never move; only brightness steps, and only when shimmer is
        // on. Shimmer off freezes them dim rather than removing them — the sky
        // is part of the theme, and stillness is not absence.
        {
            let shimmer = ctx.tuning.is_on(Knob::Shimmer);
            let n = ((area.width as u32 * area.height as u32) / 90).clamp(8, 40);
            let sky_h = (area.height as u32 * 2 / 3).max(1);
            let buf = f.buffer_mut();
            for k in 0..n {
                let s = noise(k.wrapping_mul(2_654_435_761), 0x5eed);
                let x = area.x + (s % area.width as u32) as u16;
                let y = area.y + (s / 7 % sky_h) as u16;
                if y >= area.bottom() || ctx.in_thumb(x, y) {
                    continue;
                }
                let lum = if shimmer {
                    // Each star on its own slow cycle, stepped ~10Hz.
                    let t = (ctx.frame / 6) as f64 * 0.25 + (s % 89) as f64 * 0.7;
                    quant(
                        0.32 + 0.34 * (0.5 + 0.5 * (t * std::f64::consts::TAU).sin()),
                        10.0,
                    )
                } else {
                    0.4
                };
                if !open_sky(buf, area, x, y) {
                    continue; // stars belong in the sky, not between words
                }
                let cell = &mut buf[(x, y)];
                cell.set_char(if lum > 0.55 { '✦' } else { '·' });
                cell.set_fg(scale(moon_c, lum));
            }
        }

        // --- the moon: identical every frame, so the diff skips it ----------
        if let Some((cx, cy, rv)) = geom {
            let buf = f.buffer_mut();
            let x0 = (cx - rv * ASPECT - 1.0).max(area.x as f64) as u16;
            let x1 = (cx + rv * ASPECT + 1.0).min(area.right() as f64 - 1.0) as u16;
            let y0 = (cy - rv - 1.0).max(area.y as f64) as u16;
            let y1 = (cy + rv + 1.0).min(area.bottom() as f64 - 1.0) as u16;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if ctx.in_thumb(x, y) {
                        continue;
                    }
                    let d = moon_dist(x, y, cx, cy);
                    // Carve the crescent with an occluding disc offset right.
                    let (ox, oy, orad) = occluder(cx, cy, rv);
                    let d_occ = moon_dist(x, y, ox, oy);
                    if d > rv || d_occ <= orad {
                        continue;
                    }
                    // Thick middle of the crescent reads brighter than its horns.
                    let thick = (rv - d).min(d_occ - orad);
                    let (ch, lum) = if thick > 1.0 {
                        ('•', 0.95)
                    } else {
                        ('·', 0.6)
                    };
                    // The moon is an overlay, not a painted backdrop: it shows
                    // as light on what is already there. On open sky that is a
                    // drawn crescent; over the panels it lifts the glyphs it
                    // crosses, so the moon reads as moonlight through the
                    // canopy rather than a lit shape behind the UI.
                    let sky = open_sky(buf, area, x, y);
                    let cell = &mut buf[(x, y)];
                    if sky {
                        cell.set_char(ch);
                        cell.set_fg(scale(moon_c, lum));
                    } else if cell.symbol() != " " {
                        tint_fg(cell, moon_c, lum * 0.7);
                    }
                }
            }
        }

        // --- moon glow: constant factors, no time term ----------------------
        // Moonlight falling on the panels near the moon. Because the factor
        // depends only on distance, every frame produces identical RGB and
        // ratatui rewrites nothing.
        if ctx.tuning.is_on(Knob::Glow)
            && let Some((cx, cy, rv)) = geom
        {
            let reach = rv * 2.4;
            let buf = f.buffer_mut();
            let x0 = (cx - reach * ASPECT).max(area.x as f64) as u16;
            let x1 = (cx + reach * ASPECT).min(area.right() as f64 - 1.0) as u16;
            let y0 = (cy - reach).max(area.y as f64) as u16;
            let y1 = (cy + reach).min(area.bottom() as f64 - 1.0) as u16;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if ctx.in_thumb(x, y) {
                        continue;
                    }
                    let d = moon_dist(x, y, cx, cy);
                    if d <= rv || d > reach {
                        continue;
                    }
                    let fall = 1.0 - (d - rv) / (reach - rv);
                    let m = quant(fall * fall * 0.45, 8.0);
                    if m < 0.06 {
                        continue;
                    }
                    let cell = &mut buf[(x, y)];
                    if cell.symbol() == " " {
                        continue; // glow lights what is there; it adds nothing
                    }
                    tint_fg(cell, moon_c, m);
                }
            }
        }

        // --- mist: drifting veil across the lower third --------------------
        // Wind is the mist's off switch as well as its drift rate. The band
        // offset advances on a stepped clock (~7.5Hz) and opacity is quantized,
        // so cells re-color only on real level changes.
        if ctx.tuning.wind >= 0.02 {
            let top = area.y + area.height * 2 / 3;
            let drift = (ctx.frame / 8) as f64 * (0.15 + ctx.tuning.wind as f64 * 0.5);
            let strength = 0.15 + ctx.tuning.wind as f64 * 0.35;
            let buf = f.buffer_mut();
            for y in top..area.bottom() {
                // Two offset bands so the veil has depth rather than one edge.
                let row = (y - top) as f64;
                for x in area.x..area.right() {
                    if ctx.in_thumb(x, y) {
                        continue;
                    }
                    let u = x as f64 * 0.11 - drift;
                    let wave = 0.5 + 0.5 * (u.sin() * 0.6 + (u * 0.37 + row * 0.5).sin() * 0.4);
                    let m = quant(wave * strength, 8.0);
                    if m < 0.06 {
                        continue;
                    }
                    let cell = &mut buf[(x, y)];
                    if cell.symbol() == " " {
                        continue;
                    }
                    tint_fg(cell, mist_c, m);
                }
            }
        }

        // --- spirit motes ---------------------------------------------------
        if ctx.tuning.density < 0.02 {
            return;
        }
        let lift = quant(
            (ctx.routed_beat() * ctx.tuning.beat_sync).clamp(0.0, 1.0) as f64,
            4.0,
        );
        let buf = f.buffer_mut();
        for p in &sim.sparks {
            let (x, y) = (p.x.round() as i32, p.y.round() as i32);
            if x < area.x as i32
                || x >= area.right() as i32
                || y < area.y as i32
                || y >= area.bottom() as i32
            {
                continue;
            }
            if ctx.in_thumb(x as u16, y as u16) {
                continue;
            }
            // Own blink cycle, stepped ~10Hz; bass lifts the whole field.
            let t = ((ctx.frame / 6) as f64 + (p.seed % 97) as f64 * 0.62) * 0.22;
            let own = 0.5 + 0.5 * (t * std::f64::consts::TAU).sin();
            let lum = quant((own * 0.7 + lift * 0.5).min(1.0), 10.0);
            let age_f = p.age as f64 / p.life.max(1) as f64;
            let envelope = (age_f * 12.0).min(1.0) * ((1.0 - age_f) * 8.0).min(1.0);
            let lum = lum * envelope;
            if lum < 0.1 {
                continue;
            }
            // A rare warm mote among the cold ones — the panda's satchel amber.
            let warm = p.seed % 12 == 0;
            let col = if warm { warm_c } else { mote_c };
            // Only glyphs the theme's neighbours already rely on: 'ˑ' (U+02D1)
            // read as tofu in Menlo and other common terminal fonts.
            let ch = if lum > 0.55 { '•' } else { '·' };
            // Motes float behind the UI: they glow in open sky and only warm
            // the glyphs they pass over. Overwriting turned "4.0 MB" into
            // "4.ˑ MB" and ate letters out of filenames.
            let sky = open_sky(buf, area, x as u16, y as u16);
            let cell = &mut buf[(x as u16, y as u16)];
            if sky {
                cell.set_char(ch);
                cell.set_fg(scale(col, 0.4 + lum * 0.6));
            } else if cell.symbol() != " " {
                tint_fg(cell, col, lum * 0.45);
            }
        }
    }

    fn wave_tint(&self, base: Color, ctx: &FrameCtx, frac: f64, played: bool) -> Color {
        if !ctx.tuning.is_on(Knob::Glint) {
            return base;
        }
        // Moonlight travelling along the water. Stepped + quantized so columns
        // only re-color on real level changes.
        let (_, _, _, _, glint_c) = colorway(&ctx.tuning);
        let t = (ctx.frame / 6) as f64 * 0.14;
        let shimmer = quant(
            0.82 + 0.18 * ((frac * 7.0 - t) * std::f64::consts::TAU * 0.16).sin(),
            10.0,
        );
        let bass = quant((ctx.routed_beat() * ctx.tuning.beat_sync) as f64, 4.0);
        let lit = mix(scale(base, shimmer), glint_c, bass * 0.3);
        if played { lit } else { scale(lit, 0.75) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };

    fn ctx(frame: u64, tuning: Tuning) -> FrameCtx {
        FrameCtx {
            frame,
            screen: AREA,
            tree_rect: Rect::new(0, 0, 30, 20),
            scope_rect: Rect::new(30, 0, 50, 6),
            wave_rect: Rect::new(30, 6, 50, 6),
            np_rect: Rect::new(30, 12, 50, 6),
            transport_rect: Rect::new(0, 22, 80, 2),
            np_thumb: None,
            hover: None,
            scope_peak: 0.0,
            beat: 0.0,
            beat_bands: [0.0; 3],
            tempo_phase: None,
            tempo_phase2: None,
            bpm: None,
            tuning,
            cursor_row: None,
            cursor_index: None,
            play_frac: None,
        }
    }

    /// Columns 0..TEXT_W stand in for real UI text; the rest is blank sky.
    /// Both halves matter: the tint layers (glow, mist) only touch glyphs,
    /// while the moon and stars only draw on blank cells.
    const TEXT_W: u16 = 40;

    fn render(ctx: &FrameCtx, sim: &ParticleSim) -> Buffer {
        let mut term = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
        term.draw(|f| {
            let buf = f.buffer_mut();
            for y in 0..AREA.height {
                for x in 0..TEXT_W {
                    let cell = &mut buf[(x, y)];
                    cell.set_char('x');
                    cell.set_fg(Color::Rgb(0x80, 0x80, 0x80));
                }
            }
            BambooMoon.overlay(f, sim, ctx);
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    fn cells() -> impl Iterator<Item = (u16, u16)> {
        (0..AREA.height).flat_map(|y| (0..AREA.width).map(move |x| (x, y)))
    }

    fn moonlit() -> Tuning {
        BambooMoon.default_tuning()
    }

    /// Eyeball the crescent: `cargo test print_sky -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_sky() {
        let mut term = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
        term.draw(|f| BambooMoon.overlay(f, &ParticleSim::new(), &ctx(0, moonlit())))
            .unwrap();
        let buf = term.backend().buffer().clone();
        for y in 0..AREA.height {
            let row: String = (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect();
            println!("|{row}|");
        }
    }

    /// The flicker rule, as a test: between two frames that don't cross a
    /// stepped-clock boundary, with no beat and a still particle pool, the
    /// overlay must produce byte-identical cells. A per-frame sine or an
    /// unquantized decay anywhere in a layer breaks this.
    #[test]
    fn frames_within_a_clock_step_are_identical() {
        let sim = ParticleSim::new();
        // 60 and 61 share frame/6 and frame/8, so nothing may change.
        let a = render(&ctx(60, moonlit()), &sim);
        let b = render(&ctx(61, moonlit()), &sim);
        assert_eq!(a, b, "overlay churned between frames within a clock step");
    }

    #[test]
    fn moon_hangs_in_the_upper_right() {
        let sim = ParticleSim::new();
        let buf = render(&ctx(0, moonlit()), &sim);
        // The crescent sits in the blank upper-right sky.
        let lit = cells()
            .filter(|&(x, y)| x >= TEXT_W && y < AREA.height / 2)
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(
            lit > 12,
            "expected a visible moon and stars, got {lit} cells"
        );
        // Tint layers light the UI; they never blank or replace its glyphs.
        assert!(
            cells()
                .filter(|&(x, _)| x < TEXT_W)
                .all(|(x, y)| buf[(x, y)].symbol() == "x"),
            "a tint layer overwrote real text"
        );
    }

    #[test]
    fn each_layer_toggles_off_independently() {
        let sim = ParticleSim::new();
        let base = render(&ctx(60, moonlit()), &sim);

        let no_glow = Tuning {
            glow: 0.0,
            ..moonlit()
        };
        assert_ne!(
            base,
            render(&ctx(60, no_glow), &sim),
            "glow toggle is inert"
        );

        let no_mist = Tuning {
            wind: 0.0,
            ..moonlit()
        };
        assert_ne!(
            base,
            render(&ctx(60, no_mist), &sim),
            "mist toggle is inert"
        );

        // Shimmer freezes the stars dim rather than removing them.
        let no_shimmer = Tuning {
            shimmer: 0.0,
            ..moonlit()
        };
        assert_ne!(
            base,
            render(&ctx(60, no_shimmer), &sim),
            "shimmer toggle is inert"
        );
    }

    #[test]
    fn density_zero_draws_no_motes() {
        let mut sim = ParticleSim::new();
        sim.push(Spark {
            x: 4.0,
            y: 18.0,
            vx: 0.0,
            vy: 0.0,
            age: 10,
            life: 500,
            seed: 3,
        });
        let off = Tuning {
            density: 0.0,
            wind: 0.0,
            glow: 0.0,
            ..moonlit()
        };
        let with_mote = render(&ctx(60, off), &sim);
        let without = render(&ctx(60, off), &ParticleSim::new());
        assert_eq!(with_mote, without, "motes drew with density at 0");
    }

    #[test]
    fn glint_toggle_gates_the_waveform_tint() {
        let base = Color::Rgb(0x58, 0x7b, 0x7b);
        let off_t = Tuning {
            glint: 0.0,
            ..moonlit()
        };
        // Off is exact along the whole track, not just at one column.
        for i in 0..=20 {
            let frac = i as f64 / 20.0;
            assert_eq!(
                BambooMoon.wave_tint(base, &ctx(60, off_t), frac, true),
                base,
                "glint off must leave the wave color alone at frac {frac}"
            );
        }
        // On, the shimmer travels: some columns sit at the wave's crest and
        // land back on `base`, so look for variation across the track.
        let lit: Vec<_> = (0..=20)
            .map(|i| BambooMoon.wave_tint(base, &ctx(60, moonlit()), i as f64 / 20.0, true))
            .collect();
        assert!(
            lit.iter().any(|&c| c != base),
            "glint on must tint the wave somewhere along the track"
        );
    }
}
