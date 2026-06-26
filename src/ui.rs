//! All rendering. Immediate-mode: reads `App`, draws, never mutates.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine, Points};

use crate::config::{Anim, ScopeMode, ScopeStyle, THEMES, Theme};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

use crate::app::App;
use crate::util::{fmt_time, fmt_time_precise};

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(root[0]);

    // Left column: "selection" panel above the file explorer.
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(cols[0]);

    // Record click-to-seek hit rects (must mirror draw_inspector's split below).
    let inspector = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(9),
        ])
        .split(cols[1]);
    app.wave_rect = inspector[1];
    app.transport_rect = root[1];
    app.scope_rect = inspector[2];
    app.screen = f.area();

    draw_selection(f, app, left[0]);
    draw_tree(f, app, left[1]);
    draw_inspector(f, app, cols[1]);
    draw_transport(f, app, root[1]);

    match app.theme.anim {
        Anim::Snow => draw_snow(f, app),
        Anim::Flame => draw_flame_effects(f, app),
        Anim::Flag => draw_flag(f, app, inspector[0]),
        Anim::Glitch => draw_glitch(f, app),
        Anim::Electric => draw_electric_effects(f, app),
        _ => {}
    }
    draw_hover_seek(f, app);
    if app.show_theme {
        draw_theme_modal(f, app);
    }
    if app.show_help {
        draw_help(f, app);
    }
}

/// Theme picker: list of themes with color swatches; the highlighted row is
/// live-previewed in the rest of the UI. Navigated with j/k, applied with Enter.
fn draw_theme_modal(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered(46, 80, f.area());
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = THEMES
        .iter()
        .map(|th| {
            let swatch = |c| Span::styled("██", Style::default().fg(c));
            let tag = if th.anim == Anim::None { "  " } else { "✦ " };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<11}", th.name), Style::default().fg(th.media)),
                Span::styled(tag, Style::default().fg(t.dim)),
                swatch(th.accent),
                swatch(th.accent2),
                swatch(th.scope),
                swatch(th.playing),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(
            panel_hint(
                "theme",
                border(t, app.frame, t.accent2, 0.14),
                "↑↓ pick · ⏎ apply · esc cancel",
                t.dim,
            )
            .padding(Padding::horizontal(1)),
        )
        .highlight_style(Style::default().bg(t.bg_sel).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌");
    let mut state = ListState::default();
    state.select(Some(app.theme_sel));
    f.render_stateful_widget(list, area, &mut state);
}

/// Deterministic falling-snow overlay drawn over the whole window. Each flake's
/// column, glyph, fall speed and sway derive from its index + the frame counter,
/// so it animates without any RNG or persistent state.
fn draw_snow(f: &mut Frame, app: &App) {
    const GLYPHS: [char; 5] = ['❄', '❅', '*', '·', '✦'];
    let area = f.area();
    let (w, h) = (area.width as u32, area.height as u32);
    if w == 0 || h == 0 {
        return;
    }
    let buf = f.buffer_mut();
    let frame = app.frame;
    let flakes = (w / 2).max(8);
    for k in 0..flakes {
        let col = k.wrapping_mul(2_654_435_761) % w;
        let phase = k.wrapping_mul(40_503) % h;
        let div = 3 + (k % 4); // fall speed: one cell every 3..6 frames
        let y = ((frame as u32 / div) + phase) % h;
        let sway = ((frame as i64) / 9 + k as i64).rem_euclid(3) as i32 - 1;
        let x = (col as i32 + sway).clamp(0, w as i32 - 1) as u32;
        let shade = match k % 3 {
            0 => Color::Rgb(0xff, 0xff, 0xff),
            1 => Color::Rgb(0xcf, 0xe4, 0xff),
            _ => Color::Rgb(0x9e, 0xb8, 0xd8),
        };
        let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
        cell.set_char(GLYPHS[k as usize % GLYPHS.len()]);
        cell.set_fg(shade);
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

/// Flame-theme overlays: navigation particles, a burning playhead column, and
/// the Calcifer fireball in the top-right corner.
fn draw_flame_effects(f: &mut Frame, app: &App) {
    let area = f.area();
    let frame = app.frame as u32;

    let wr = app.wave_rect;
    let dur = app.engine.duration_secs();
    let pos = app.engine.position_secs();
    let playing = app.engine.is_playing();
    let frac = if dur > 0.0 && app.now_playing.is_some() && playing {
        Some((pos / dur).clamp(0.0, 1.0))
    } else {
        None
    };
    let cal = calcifer_rows(app.frame);
    let fphase = app.frame as f64;
    let buf = f.buffer_mut();

    // Particles (nav bursts, click radials, scope shoot-offs).
    for p in &app.particles {
        let (x, y) = (p.x.round() as i32, p.y.round() as i32);
        if x < area.x as i32
            || x >= area.right() as i32
            || y < area.y as i32
            || y >= area.bottom() as i32
        {
            continue;
        }
        let t = p.age as f64 / p.life.max(1) as f64;
        let (ch, col) = flame_glyph(t, p.seed ^ frame);
        let cell = &mut buf[(x as u16, y as u16)];
        cell.set_char(ch);
        cell.set_fg(col);
    }

    // Burning playhead — a small flame cluster pinned to the play column that
    // oscillates up and down the seek line as the song plays.
    if let Some(frac) = frac {
        if wr.width > 2 && wr.height > 3 {
            let cx = wr.x + 1 + (frac * (wr.width - 2) as f64) as u16;
            let top = (wr.y + 1) as f64;
            let bottom = (wr.y + wr.height - 2) as f64;
            let mid = (top + bottom) / 2.0;
            let amp = (bottom - top) / 2.0;
            let yc = mid + (fphase * 0.14).sin() * amp;
            for dy in -2i32..=2 {
                let y = (yc.round() as i32 + dy).clamp(top as i32, bottom as i32) as u16;
                let n = crate::util::noise(frame + dy as u32 * 17, cx as u32);
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
        for (i, (text, color)) in cal.iter().enumerate() {
            let y = area.y + i as u16;
            if y < area.bottom() {
                buf.set_string(bx, y, text, Style::default().fg(*color));
            }
        }
    }
}

/// Electric overlay: spark particles + lightning bolts inside the oscilloscope.
fn draw_electric_effects(f: &mut Frame, app: &App) {
    let area = f.area();
    let (w, h) = (area.width, area.height);
    if w < 4 || h < 4 {
        return;
    }
    let frame = app.frame as u32;
    let sr = app.scope_rect;
    let peak = app.scope_buf.iter().fold(0f32, |m, &s| m.max(s.abs()));
    let buf = f.buffer_mut();

    // Spark particles (nav / click / scope), electric palette.
    for p in &app.particles {
        let (x, y) = (p.x.round() as i32, p.y.round() as i32);
        if x < area.x as i32
            || x >= area.right() as i32
            || y < area.y as i32
            || y >= area.bottom() as i32
        {
            continue;
        }
        let (ch, col) = spark_glyph(p.age as f64 / p.life.max(1) as f64, p.seed ^ frame);
        let cell = &mut buf[(x as u16, y as u16)];
        cell.set_char(ch);
        cell.set_fg(col);
    }

    // Lightning bolts strike inside the oscilloscope when it peaks loud
    // (same amplitude trigger as the spark particles).
    if sr.width > 3 && sr.height > 3 && peak >= 0.12 && crate::util::noise(frame / 2, 7) % 3 == 0 {
        let bolts = 1 + crate::util::noise(frame, 3) % 2;
        let lo = (sr.x + 1) as i32;
        let hi = (sr.x + sr.width - 2) as i32;
        for b in 0..bolts {
            let seed = crate::util::noise(frame / 2, b * 53 + 11);
            let mut x = lo + (seed % (sr.width - 2).max(1) as u32) as i32;
            for y in (sr.y + 1)..(sr.y + sr.height - 1) {
                let n = crate::util::noise(seed + y as u32, frame);
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

/// Retro glitch overlay: a few jumping artifact bands plus scattered noise
/// glyphs, in green with cyan/magenta channel-split flecks.
fn draw_glitch(f: &mut Frame, app: &App) {
    const GLYPHS: [char; 14] = [
        '▓', '▒', '░', '█', '▚', '▞', '▙', '▟', '╳', '┊', '/', '\\', '¦', '╎',
    ];
    const COLORS: [Color; 4] = [
        Color::Rgb(0x33, 0xff, 0x66),
        Color::Rgb(0x00, 0xff, 0xff),
        Color::Rgb(0xff, 0x00, 0xcc),
        Color::Rgb(0x00, 0xff, 0x88),
    ];
    let area = f.area();
    let (w, h) = (area.width as u32, area.height as u32);
    if w < 4 || h < 4 {
        return;
    }
    // Slow the whole effect down: only update the glitch state every ~10 frames.
    let slot = app.frame as u32 / 10;
    let buf = f.buffer_mut();

    // A couple of horizontal artifact bands that occasionally appear.
    for b in 0..2u32 {
        let on = crate::util::noise(slot + b * 97, b * 13 + 5);
        if on % 7 != 0 {
            continue; // band idle this slot (rarer)
        }
        let y = (crate::util::noise(slot, b * 31) % h) as u16;
        let x0 = (crate::util::noise(slot + 1, b * 17) % w) as u16;
        let len = 3 + (crate::util::noise(slot, b * 7) % (w / 3).max(1)) as u16;
        for i in 0..len {
            let x = area.x + x0 + i;
            if x >= area.right() {
                break;
            }
            let s = crate::util::noise(slot + i as u32, b * 101 + y as u32);
            let cell = &mut buf[(x, area.y + y)];
            cell.set_char(GLYPHS[s as usize % GLYPHS.len()]);
            cell.set_fg(COLORS[(s as usize / 7) % COLORS.len()]);
        }
    }

    // Sparse scattered sparkle artifacts (calm).
    let scatter = (w * h / 200).max(4);
    for k in 0..scatter {
        let s = crate::util::noise(slot.wrapping_add(k.wrapping_mul(40_503)), k ^ slot);
        if s % 11 != 0 {
            continue;
        }
        let x = area.x + (s % w) as u16;
        let y = area.y + ((s / w) % h) as u16;
        let cell = &mut buf[(x, y)];
        cell.set_char(GLYPHS[(s as usize / 3) % GLYPHS.len()]);
        cell.set_fg(COLORS[s as usize % COLORS.len()]);
    }
}

/// Small static stars-and-stripes in the top-right of the now-playing panel,
/// plus click-fireworks (Flag theme). Left side keeps the track text.
fn draw_flag(f: &mut Frame, app: &App, panel: Rect) {
    let red = Color::Rgb(0xb2, 0x22, 0x34);
    let white = Color::Rgb(0xf2, 0xf2, 0xf2);
    let blue = Color::Rgb(0x3c, 0x3b, 0x6e);
    if panel.width >= 12 && panel.height >= 4 {
        let inner_w = panel.width - 2;
        let h = panel.height - 2;
        // Small but detailed: ~30% of the inner width, top-right corner.
        let fw = ((inner_w as u32 * 12 / 40) as u16).clamp(9, inner_w);
        let fx = panel.x + 1 + (inner_w - fw);
        let y0 = panel.y + 1;
        let canton_w = (fw / 2).max(5);
        // Taller blue field so it holds at least two rows of stars.
        let canton_h = (h * 2 / 3).max(2).min(h);
        // Half-block stripes: 2 stripes per cell row -> ~13-stripe look in a
        // few rows. Stripe 0 (top) is red, alternating down.
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

    // Click fireworks: colorful bursts anywhere on screen.
    let scr = app.screen;
    let frame = app.frame as u32;
    let buf = f.buffer_mut();
    for p in &app.particles {
        let (x, y) = (p.x.round() as i32, p.y.round() as i32);
        if x < scr.x as i32
            || x >= scr.right() as i32
            || y < scr.y as i32
            || y >= scr.bottom() as i32
        {
            continue;
        }
        let (ch, col) = firework_glyph(p.age as f64 / p.life.max(1) as f64, p.seed ^ frame);
        let cell = &mut buf[(x as u16, y as u16)];
        cell.set_char(ch);
        cell.set_fg(col);
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

fn panel<'a>(title: &'a str, accent: ratatui::style::Color) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
}

/// A panel with its relevant key hints docked into the bottom-right border, so
/// the controls are always visible without opening the help overlay.
fn panel_hint<'a>(
    title: &'a str,
    accent: ratatui::style::Color,
    hint: &'a str,
    dim: ratatui::style::Color,
) -> Block<'a> {
    panel(title, accent).title_bottom(
        Line::from(Span::styled(format!(" {hint} "), Style::default().fg(dim))).right_aligned(),
    )
}

/// Map a phase (any real; wraps at 1.0) to a fully-saturated rainbow color.
fn hue(phase: f64) -> Color {
    let h = phase.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

fn rgb_of(c: Color) -> (f64, f64, f64) {
    if let Color::Rgb(r, g, b) = c {
        (r as f64, g as f64, b as f64)
    } else {
        (200.0, 200.0, 200.0)
    }
}

/// Interpolate around a looping list of color stops at `phase` (wraps at 1.0).
fn gradient(stops: &[(f64, f64, f64)], phase: f64) -> Color {
    let n = stops.len();
    let p = phase.rem_euclid(1.0) * n as f64;
    let i = p.floor() as usize % n;
    let frac = p - p.floor();
    let a = stops[i];
    let b = stops[(i + 1) % n];
    Color::Rgb(
        (a.0 + (b.0 - a.0) * frac) as u8,
        (a.1 + (b.1 - a.1) * frac) as u8,
        (a.2 + (b.2 - a.2) * frac) as u8,
    )
}

/// Pulse `base`'s brightness; `offset` phase-shifts so the pulse ripples panel
/// to panel.
fn glow(base: Color, frame: u64, offset: f64) -> Color {
    let (r, g, b) = rgb_of(base);
    let t = frame as f64 * 0.025 - offset * 1.4;
    let f = 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f64::consts::TAU).sin());
    Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
}

const TRANS_STOPS: &[(f64, f64, f64)] = &[
    (91.0, 206.0, 250.0),
    (245.0, 169.0, 184.0),
    (255.0, 255.0, 255.0),
    (245.0, 169.0, 184.0),
];
const CMY_STOPS: &[(f64, f64, f64)] = &[
    (0.0, 229.0, 229.0),
    (229.0, 0.0, 229.0),
    (229.0, 229.0, 0.0),
];

/// Border accent for a panel: the theme's static `base`, or an animated color
/// per the theme's `anim`. `offset` spreads the animation across panels.
fn border(theme: &Theme, frame: u64, base: Color, offset: f64) -> Color {
    match theme.anim {
        Anim::None | Anim::Snow | Anim::Flag => base,
        Anim::Prismatic => hue(frame as f64 * 0.012 + offset),
        Anim::TransSlow => gradient(TRANS_STOPS, frame as f64 * 0.0035 + offset * 0.25),
        Anim::Ripple => glow(base, frame, offset),
        Anim::Cmyk => gradient(CMY_STOPS, frame as f64 * 0.006 + offset),
        Anim::Flame => {
            // Erratic green flicker on the borders.
            let n = crate::util::noise(frame as u32 / 2, (offset * 131.0) as u32) % 100;
            let f = 0.55 + 0.45 * (n as f64 / 100.0);
            let (r, g, b) = rgb_of(base);
            Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
        }
        Anim::Glitch => {
            // Mostly green, with occasional brief RGB-split jumps (calmer).
            let n = crate::util::noise(frame as u32 / 9, (offset * 200.0) as u32) % 100;
            if n < 4 {
                Color::Rgb(0x00, 0xff, 0xff)
            } else if n < 8 {
                Color::Rgb(0xff, 0x00, 0xcc)
            } else {
                let f = 0.7 + 0.3 * ((n % 40) as f64 / 40.0);
                let (r, g, b) = rgb_of(base);
                Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
            }
        }
        Anim::Electric => {
            // Crackling cyan/white border flicker with occasional bright arcs.
            let n = crate::util::noise(frame as u32, (offset * 173.0) as u32) % 100;
            if n < 6 {
                Color::Rgb(0xff, 0xff, 0xff)
            } else if n < 14 {
                Color::Rgb(0x9d, 0xf0, 0xff)
            } else {
                let f = 0.5 + 0.5 * (n as f64 / 100.0);
                let (r, g, b) = rgb_of(base);
                Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
            }
        }
    }
}

/// Hover affordance over the seek bars: a vertical guide at the pointer column
/// showing where a click would seek to.
fn draw_hover_seek(f: &mut Frame, app: &App) {
    let Some((c, r)) = app.hover else { return };
    let mark = app.theme.playing;
    for rect in [app.wave_rect, app.transport_rect] {
        if rect.width < 3 || rect.height < 2 {
            continue;
        }
        if c <= rect.x || c >= rect.x + rect.width - 1 {
            continue;
        }
        if r < rect.y || r >= rect.y + rect.height {
            continue;
        }
        let buf = f.buffer_mut();
        for y in (rect.y + 1)..(rect.y + rect.height - 1) {
            let cell = &mut buf[(c, y)];
            cell.set_char('│');
            cell.set_fg(mark);
        }
        break;
    }
}

/// Drop the first `n` display columns from a line of spans (horizontal scroll).
fn trim_spans_left(spans: Vec<Span<'static>>, mut n: usize) -> Vec<Span<'static>> {
    if n == 0 {
        return spans;
    }
    let mut out = Vec::with_capacity(spans.len());
    for sp in spans {
        if n == 0 {
            out.push(sp);
            continue;
        }
        let len = sp.content.chars().count();
        if len <= n {
            n -= len;
            continue;
        }
        let s: String = sp.content.chars().skip(n).collect();
        n = 0;
        out.push(Span::styled(s, sp.style));
    }
    out
}

/// Lighten a color by adding `d` to each channel (for the hover row tint).
fn brighten(c: Color, d: u8) -> Color {
    let (r, g, b) = rgb_of(c);
    Color::Rgb(
        (r as u8).saturating_add(d),
        (g as u8).saturating_add(d),
        (b as u8).saturating_add(d),
    )
}

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    app.tree_rect = area;
    let t = &app.theme;
    let bc = border(t, app.frame, t.accent, 0.0);
    let filtering = app.filtering || !app.filter.is_empty();
    let yellow = Color::Rgb(0xf1, 0xfa, 0x8c);
    // Title: plain "muse", or "muse /<filter>  esc" with a yellow esc cue.
    let title_line = if filtering {
        Line::from(vec![
            Span::styled(
                " muse ",
                Style::default().fg(bc).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "/",
                Style::default().fg(yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}  ", app.filter),
                Style::default().fg(t.media).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "esc ",
                Style::default().fg(yellow).add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(Span::styled(
            " muse ",
            Style::default().fg(bc).add_modifier(Modifier::BOLD),
        ))
    };
    let hint = if filtering {
        Line::from(vec![
            Span::styled(
                " esc ",
                Style::default().fg(yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled("cancel · ⏎ apply ", Style::default().fg(t.dim)),
        ])
    } else {
        Line::from(Span::styled(
            " j/k move · l/h fold · g/G ends · / filter ",
            Style::default().fg(t.dim),
        ))
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(bc))
        .title(title_line)
        .title_bottom(hint.right_aligned());
    let hs = app.tree_hscroll as usize;
    let hover_idx = app.hover.and_then(|(c, r)| app.tree_row_at(c, r));
    let hover_bg = brighten(t.bg_sel, 0x1a);
    let finish = |idx: usize, spans: Vec<Span<'static>>| -> ListItem<'static> {
        let item = ListItem::new(Line::from(trim_spans_left(spans, hs)));
        if Some(idx) == hover_idx {
            item.style(Style::default().bg(hover_bg))
        } else {
            item
        }
    };
    let items: Vec<ListItem> = if filtering {
        // Flat fuzzy results: relative paths to matching media files.
        let root = app.root_path();
        app.filtered
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned();
                let playing = app.now_playing.as_deref() == Some(p.as_path());
                let (icon, color) = if playing {
                    ("♪ ", t.playing)
                } else {
                    ("· ", t.media)
                };
                let mut style = Style::default().fg(color);
                if playing {
                    style = style.add_modifier(Modifier::BOLD);
                }
                finish(
                    idx,
                    vec![
                        Span::styled(icon, Style::default().fg(color)),
                        Span::styled(rel, style),
                    ],
                )
            })
            .collect()
    } else {
        app.tree
            .visible
            .iter()
            .enumerate()
            .map(|(idx, &id)| {
                let n = app.tree.node(id);
                let indent = "  ".repeat(n.depth.saturating_sub(1));
                let playing = app.now_playing.as_deref() == Some(n.path.as_path());
                let (icon, color) = if n.is_dir {
                    (if n.expanded { "▾ " } else { "▸ " }, t.dir)
                } else if playing {
                    ("♪ ", t.playing)
                } else {
                    ("· ", t.media)
                };
                let mut style = Style::default().fg(color);
                if playing {
                    style = style.add_modifier(Modifier::BOLD);
                }
                let meta = if n.is_dir {
                    let items = format!("{} track{}", n.count, if n.count == 1 { "" } else { "s" });
                    if n.size > 0 {
                        format!("  {items} · {}", crate::util::fmt_size(n.size))
                    } else {
                        format!("  {items}")
                    }
                } else if n.size > 0 {
                    format!("  {}", crate::util::fmt_size(n.size))
                } else {
                    String::new()
                };
                finish(
                    idx,
                    vec![
                        Span::raw(indent),
                        Span::styled(
                            icon,
                            Style::default().fg(if n.is_dir { t.accent2 } else { color }),
                        ),
                        Span::styled(n.name.clone(), style),
                        Span::styled(meta, Style::default().fg(t.dim)),
                    ],
                )
            })
            .collect()
    };

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(t.bg_sel)
                .fg(t.accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_inspector(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // "now playing": current track
            Constraint::Min(5),    // static waveform
            Constraint::Length(9), // live scope
        ])
        .split(area);

    draw_now_playing(f, app, rows[0]);
    draw_waveform(f, app, rows[1], app.now_playing.clone(), "waveform", true);
    draw_scope(f, app, rows[2]);
    let _ = t;
}

/// "Now playing": the track the engine is actually playing, distinct from the
/// "now" panel above which follows the cursor selection.
fn draw_now_playing(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let lines: Vec<Line> = match &app.now_playing {
        Some(p) => {
            let meta = app.meta_cache.get(p);
            let (title, artist) = meta
                .filter(|m| !m.title.is_empty())
                .map(|m| (m.title.clone(), m.artist.clone()))
                .unwrap_or_else(|| {
                    (
                        p.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        String::new(),
                    )
                });
            let state = if app.engine.is_playing() {
                ("▶ playing", t.playing)
            } else {
                ("⏸ paused", t.dim)
            };
            // line 1: state + live position (picosecond precision) / duration
            let time = Line::from(vec![
                Span::styled(
                    format!("{}  ", state.0),
                    Style::default().fg(state.1).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    fmt_time_precise(app.engine.position_secs()),
                    Style::default().fg(t.media).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" / {}", fmt_time(app.engine.duration_secs())),
                    Style::default().fg(t.dim),
                ),
            ]);
            // line 2: title — artist
            let mut titleline = vec![Span::styled(title, Style::default().fg(t.playing))];
            if !artist.is_empty() {
                titleline.push(Span::styled(
                    format!("  — {artist}"),
                    Style::default().fg(t.dim),
                ));
            }
            let mut lines = vec![time, Line::from(titleline)];
            // codec/quality row: sample rate · bitrate · channels
            if let Some(m) = meta {
                let extra: Vec<String> = m.fields.iter().map(|(k, v)| format!("{k} {v}")).collect();
                if !extra.is_empty() {
                    lines.push(Line::from(Span::styled(
                        extra.join("  ·  "),
                        Style::default().fg(t.dim),
                    )));
                }
            }
            lines
        }
        None => vec![Line::from(Span::styled(
            "nothing playing",
            Style::default().fg(t.dim),
        ))],
    };
    let p = Paragraph::new(lines)
        .block(
            panel("now playing", border(t, app.frame, t.playing, 0.28))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_selection(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let path = app.cursor_path();
    let mut lines: Vec<Line> = Vec::new();
    if let Some(path) = &path {
        if let Some(m) = app.meta_cache.get(path) {
            let title = if m.title.is_empty() {
                path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                m.title.clone()
            };
            let mut titleline = vec![Span::styled(
                title,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )];
            if m.duration > 0.0 {
                titleline.push(Span::styled(
                    format!("   {}", fmt_time(m.duration)),
                    Style::default().fg(t.dim),
                ));
            }
            lines.push(Line::from(titleline));
            let kv = |k: &str, v: &str| {
                Line::from(vec![
                    Span::styled(format!("{k:<8}"), Style::default().fg(t.dim)),
                    Span::styled(v.to_string(), Style::default().fg(t.media)),
                ])
            };
            if !m.artist.is_empty() {
                lines.push(kv("artist", &m.artist));
            }
            if !m.album.is_empty() {
                lines.push(kv("album", &m.album));
            }
            if !m.genre.is_empty() {
                lines.push(kv("genre", &m.genre));
            }
            let extra: Vec<String> = m.fields.iter().map(|(k, v)| format!("{k} {v}")).collect();
            if !extra.is_empty() {
                lines.push(Line::from(Span::styled(
                    extra.join("  ·  "),
                    Style::default().fg(t.dim),
                )));
            }
        } else if app.registry.is_supported(path) {
            lines.push(Line::from(Span::styled(
                "reading tags…",
                Style::default().fg(t.dim),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                Style::default().fg(t.media),
            )));
        }
    }
    let p = Paragraph::new(lines)
        .block(
            panel("selection", border(t, app.frame, t.accent2, 0.14))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

/// Render a waveform for `path`. `with_playhead` overlays the live transport
/// position when `path` is the track currently playing.
fn draw_waveform(
    f: &mut Frame,
    app: &App,
    area: Rect,
    path: Option<PathBuf>,
    title: &str,
    with_playhead: bool,
) {
    let t = &app.theme;
    let block = panel(title, border(t, app.frame, t.wave, 0.42));
    let bins = path.as_ref().and_then(|p| app.wave_cache.get(p));

    // playhead progress 0..1 if this file is the one playing
    let progress = match (with_playhead, &path, &app.now_playing) {
        (true, Some(p), Some(np)) if p == np => {
            let d = app.engine.duration_secs();
            if d > 0.0 {
                (app.engine.position_secs() / d).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        _ => -1.0,
    };

    let inner_w = area.width.saturating_sub(2).max(1) as f64;
    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([0.0, inner_w])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx: &mut Context| {
            // zero line
            ctx.draw(&CLine {
                x1: 0.0,
                y1: 0.0,
                x2: inner_w,
                y2: 0.0,
                color: t.dim,
            });
            if let Some(bins) = bins {
                let n = bins.len();
                let cols = (inner_w * 2.0) as usize; // braille sub-columns
                for c in 0..cols {
                    let bi = c * n / cols.max(1);
                    let (lo, hi) = bins[bi.min(n - 1)];
                    let x = c as f64 / 2.0;
                    ctx.draw(&CLine {
                        x1: x,
                        y1: lo as f64,
                        x2: x,
                        y2: hi as f64,
                        color: t.wave,
                    });
                }
                if progress >= 0.0 {
                    let px = progress * inner_w;
                    ctx.draw(&CLine {
                        x1: px,
                        y1: -1.0,
                        x2: px,
                        y2: 1.0,
                        color: t.playing,
                    });
                }
            }
        });
    f.render_widget(canvas, area);

    if path
        .as_ref()
        .map(|p| app.registry.is_supported(p))
        .unwrap_or(false)
        && bins.is_none()
    {
        let hint = Paragraph::new(Span::styled("analyzing…", Style::default().fg(t.dim)));
        let inner = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: 1,
        };
        f.render_widget(hint, inner);
    }
}

fn draw_scope(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let preset = app.scope_preset();
    let buf = &app.scope_buf; // interleaved stereo, most recent last
    let inner_w = area.width.saturating_sub(2).max(1) as f64;

    // Take the most-recent `window` frames out of the rolling buffer.
    let frames = buf.len() / 2;
    let window = preset.window.min(frames).max(1);
    let win = &buf[(frames - window) * 2..];

    // Auto-gain: scale so the loudest sample in the window nearly fills the panel.
    let gain = if preset.auto_gain {
        let peak = win.iter().fold(0f32, |m, &s| m.max(s.abs()));
        if peak > 0.02 {
            (0.92 / peak).min(12.0)
        } else {
            1.0
        }
    } else {
        1.0
    };

    // Precompute owned point data so the paint closure stays allocation-free.
    let mono: Vec<f64> = if preset.mode == ScopeMode::Mono {
        (0..window)
            .map(|i| (0.5 * (win[i * 2] + win[i * 2 + 1]) * gain).clamp(-1.0, 1.0) as f64)
            .collect()
    } else {
        Vec::new()
    };
    let xy: Vec<(f64, f64)> = if preset.mode == ScopeMode::StereoXy {
        let half = inner_w * 0.5;
        (0..window)
            .map(|i| {
                let l = (win[i * 2] * gain).clamp(-1.0, 1.0) as f64;
                let r = (win[i * 2 + 1] * gain).clamp(-1.0, 1.0) as f64;
                (half + l * half, r)
            })
            .collect()
    } else {
        Vec::new()
    };

    let style = preset.style;
    let mode = preset.mode;
    let title = format!("oscilloscope · {}", preset.name);
    let canvas = Canvas::default()
        .block(panel_hint(
            &title,
            border(t, app.frame, t.scope, 0.56),
            "v/V preset",
            t.dim,
        ))
        .marker(Marker::Braille)
        .x_bounds([0.0, inner_w])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx: &mut Context| {
            // zero line / reference axes
            ctx.draw(&CLine {
                x1: 0.0,
                y1: 0.0,
                x2: inner_w,
                y2: 0.0,
                color: t.dim,
            });
            if mode == ScopeMode::StereoXy {
                ctx.draw(&CLine {
                    x1: inner_w * 0.5,
                    y1: -1.0,
                    x2: inner_w * 0.5,
                    y2: 1.0,
                    color: t.dim,
                });
                ctx.draw(&Points {
                    coords: &xy,
                    color: t.scope,
                });
                return;
            }
            let n = mono.len();
            if n == 0 {
                return;
            }
            let cols = (inner_w * 2.0) as usize;
            let sample = |c: usize| -> (f64, f64) {
                let si = (c * n / cols.max(1)).min(n - 1);
                (c as f64 / 2.0, mono[si])
            };
            match style {
                ScopeStyle::Line => {
                    let mut prev: Option<(f64, f64)> = None;
                    for c in 0..=cols {
                        let (x, y) = sample(c);
                        if let Some((px, py)) = prev {
                            ctx.draw(&CLine {
                                x1: px,
                                y1: py,
                                x2: x,
                                y2: y,
                                color: t.scope,
                            });
                        }
                        prev = Some((x, y));
                    }
                }
                ScopeStyle::Mirror => {
                    for c in 0..=cols {
                        let (x, y) = sample(c);
                        ctx.draw(&CLine {
                            x1: x,
                            y1: -y.abs(),
                            x2: x,
                            y2: y.abs(),
                            color: t.scope,
                        });
                    }
                }
                ScopeStyle::Bars => {
                    for c in 0..=cols {
                        let (x, y) = sample(c);
                        ctx.draw(&CLine {
                            x1: x,
                            y1: 0.0,
                            x2: x,
                            y2: y,
                            color: t.scope,
                        });
                    }
                }
                ScopeStyle::Dots => {
                    let pts: Vec<(f64, f64)> = (0..=cols).map(&sample).collect();
                    ctx.draw(&Points {
                        coords: &pts,
                        color: t.scope,
                    });
                }
            }
        });
    f.render_widget(canvas, area);
}

fn draw_transport(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let pos = app.engine.position_secs();
    let dur = app.engine.duration_secs();
    let ratio = if dur > 0.0 {
        (pos / dur).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let state = if app.engine.is_playing() {
        "▶"
    } else {
        "⏸"
    };
    let vol = (app.engine.volume() * 100.0) as u32;

    let title = app
        .now_playing
        .as_ref()
        .map(|p| {
            app.meta_cache
                .get(p)
                .filter(|m| !m.title.is_empty())
                .map(|m| {
                    if m.artist.is_empty() {
                        m.title.clone()
                    } else {
                        format!("{} — {}", m.artist, m.title)
                    }
                })
                .unwrap_or_else(|| {
                    p.file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
        })
        .unwrap_or_else(|| "no track".to_string());

    let loop_glyph = match app.loop_mode {
        crate::app::LoopMode::Off => "→",
        crate::app::LoopMode::All => "↻",
        crate::app::LoopMode::One => "↻¹",
    };
    let label = format!(
        " {state}  {}  {} / {}   vol {vol}%   {loop_glyph} {} ",
        title,
        fmt_time(pos),
        fmt_time(dur),
        app.loop_mode.label(),
    );
    let gauge = Gauge::default()
        .block(panel_hint(
            "transport",
            border(t, app.frame, t.accent, 0.70),
            "space play · n/p next/prev · r loop · t theme · ,/. seek · -/+ vol · ? help",
            t.dim,
        ))
        .gauge_style(Style::default().fg(t.accent2).bg(t.bg_sel))
        .ratio(ratio)
        // No explicit fg: ratatui swaps fg/bg under the label over the filled
        // part, so the text reads dark-on-fill there and light-on-dark elsewhere.
        .label(Span::styled(
            label,
            Style::default().add_modifier(Modifier::BOLD),
        ));
    f.render_widget(gauge, area);
}

fn draw_help(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered(62, 75, f.area());
    f.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled(
            format!("muse v{} — keys", env!("CARGO_PKG_VERSION")),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  j / k or ↑ ↓     move cursor"),
        Line::from("  l / →            expand dir"),
        Line::from("  h / ←            collapse / parent"),
        Line::from("  g / G            top / bottom"),
        Line::from("  ⏎                expand dir or play file"),
        Line::from("  space            play / pause"),
        Line::from("  n / p            next / previous track"),
        Line::from("  r                loop mode (off / all / one)"),
        Line::from("  , / .            seek -5s / +5s"),
        Line::from("  shift ← / →      scrub playhead -1s / +1s"),
        Line::from("  click tree       folder: fold · file: play"),
        Line::from("  click/drag bar   seek / scrub the playhead"),
        Line::from("  media keys       play/pause/next/prev (OS)"),
        Line::from("  - / +            volume down / up"),
        Line::from("  v / V            cycle scope preset (saved)"),
        Line::from(format!(
            "  t                theme picker — {} (saved)",
            t.name
        )),
        Line::from("  /                fuzzy find (⏎ apply · esc reset)"),
        Line::from("  ? then q / esc   open / close this help"),
        Line::from("  q                quit"),
    ];
    let p = Paragraph::new(lines).block(
        panel_hint(
            "help",
            border(t, app.frame, t.accent2, 0.84),
            "q / esc to close",
            t.dim,
        )
        .padding(Padding::uniform(1)),
    );
    f.render_widget(p, area);
}

fn centered(pw: u16, ph: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - ph) / 2),
            Constraint::Percentage(ph),
            Constraint::Percentage((100 - ph) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pw) / 2),
            Constraint::Percentage(pw),
            Constraint::Percentage((100 - pw) / 2),
        ])
        .split(v[1])[1]
}
