//! All rendering. Immediate-mode: reads `App`, draws, never mutates.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine, Points};

use crate::config::{ScopeMode, ScopeStyle, Theme};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, Padding, Paragraph, Wrap,
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

    draw_selection(f, app, left[0]);
    draw_tree(f, app, left[1]);
    draw_inspector(f, app, cols[1]);
    draw_transport(f, app, root[1]);

    if app.show_help {
        draw_help(f, app);
    }
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

/// Border accent for a panel: the theme's `base`, or — in prismatic mode — a
/// time-shifting rainbow hue, offset per panel so they spread across the wheel.
fn border(theme: &Theme, frame: u64, base: Color, offset: f64) -> Color {
    if theme.prismatic {
        hue(frame as f64 * 0.012 + offset)
    } else {
        base
    }
}

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
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
    let items: Vec<ListItem> = if filtering {
        // Flat fuzzy results: relative paths to matching media files.
        let root = app.root_path();
        app.filtered
            .iter()
            .map(|p| {
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
                ListItem::new(Line::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::styled(rel, style),
                ]))
            })
            .collect()
    } else {
        app.tree
            .visible
            .iter()
            .map(|&id| {
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
                ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        icon,
                        Style::default().fg(if n.is_dir { t.accent2 } else { color }),
                    ),
                    Span::styled(n.name.clone(), style),
                    Span::styled(meta, Style::default().fg(t.dim)),
                ]))
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
    let area = centered(60, 50, f.area());
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
        Line::from("  - / +            volume down / up"),
        Line::from("  v / V            cycle scope preset (saved)"),
        Line::from(format!(
            "  t / T            cycle color theme — {} (saved)",
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
