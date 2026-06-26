//! All rendering. Immediate-mode: reads `App`, draws, never mutates.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Gauge, List, ListItem, Padding, Paragraph, Wrap,
};
use ratatui::Frame;

use crate::app::App;
use crate::util::fmt_time;

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(root[0]);

    draw_tree(f, app, cols[0]);
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

fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let t = &app.theme;
    let title = if app.filtering || !app.filter.is_empty() {
        format!("muse — /{}", app.filter)
    } else {
        "muse".to_string()
    };
    let items: Vec<ListItem> = app
        .tree
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
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, Style::default().fg(if n.is_dir { t.accent2 } else { color })),
                Span::styled(n.name.clone(), style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(panel(&title, t.accent))
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
            Constraint::Length(7), // metadata
            Constraint::Min(6),    // static waveform
            Constraint::Length(9), // live scope
        ])
        .split(area);

    draw_metadata(f, app, rows[0]);
    draw_waveform(f, app, rows[1]);
    draw_scope(f, app, rows[2]);
    let _ = t;
}

fn draw_metadata(f: &mut Frame, app: &App, area: Rect) {
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
            lines.push(Line::from(Span::styled(
                title,
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
            )));
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
        .block(panel("now", t.accent2).padding(Padding::horizontal(1)))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_waveform(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let block = panel("waveform", t.wave);
    let path = app.cursor_path();
    let bins = path.as_ref().and_then(|p| app.wave_cache.get(p));

    // playhead progress 0..1 if this file is the one playing
    let progress = match (&path, &app.now_playing) {
        (Some(p), Some(np)) if p == np => {
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

    if path.as_ref().map(|p| app.registry.is_supported(p)).unwrap_or(false) && bins.is_none() {
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
    let buf = &app.scope_buf;
    let inner_w = area.width.saturating_sub(2).max(1) as f64;
    let len = buf.len();
    let canvas = Canvas::default()
        .block(panel("oscilloscope", t.scope))
        .marker(Marker::Braille)
        .x_bounds([0.0, inner_w])
        .y_bounds([-1.0, 1.0])
        .paint(move |ctx: &mut Context| {
            ctx.draw(&CLine {
                x1: 0.0,
                y1: 0.0,
                x2: inner_w,
                y2: 0.0,
                color: t.dim,
            });
            let cols = (inner_w * 2.0) as usize;
            let mut prev: Option<(f64, f64)> = None;
            for c in 0..=cols {
                let si = c * len / cols.max(1);
                let y = buf[si.min(len - 1)] as f64;
                let x = c as f64 / 2.0;
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
        });
    f.render_widget(canvas, area);
}

fn draw_transport(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let pos = app.engine.position_secs();
    let dur = app.engine.duration_secs();
    let ratio = if dur > 0.0 { (pos / dur).clamp(0.0, 1.0) } else { 0.0 };
    let state = if app.engine.is_playing() { "▶" } else { "⏸" };
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
        .unwrap_or_else(|| app.status.clone());

    let label = format!(
        " {state}  {}  {} / {}   vol {vol}% ",
        title,
        fmt_time(pos),
        fmt_time(dur),
    );
    let gauge = Gauge::default()
        .block(panel("transport", t.accent))
        .gauge_style(Style::default().fg(t.accent2).bg(t.bg_sel))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default().fg(t.media).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(gauge, area);
}

fn draw_help(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered(60, 50, f.area());
    f.render_widget(Clear, area);
    let lines = vec![
        Line::from(Span::styled(
            "muse — keys",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  j / k or ↑ ↓     move cursor"),
        Line::from("  l / →            expand dir"),
        Line::from("  h / ←            collapse / parent"),
        Line::from("  g / G            top / bottom"),
        Line::from("  ⏎                expand dir or play file"),
        Line::from("  space / p        play / pause"),
        Line::from("  , / .            seek -5s / +5s"),
        Line::from("  - / +            volume down / up"),
        Line::from("  /                filter (esc clears)"),
        Line::from("  ? / q            help / quit"),
    ];
    let p = Paragraph::new(lines).block(panel("help", t.accent2).padding(Padding::uniform(1)));
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
