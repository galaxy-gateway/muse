//! Right column: now-playing header, the static per-track waveform, and the live
//! oscilloscope.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine, Points};
use ratatui::widgets::{Padding, Paragraph, Wrap};

use super::widgets::{border, panel, panel_hint};
use crate::app::App;
use crate::config::{ScopeMode, ScopeStyle};
use crate::util::{fmt_time, fmt_time_precise};

pub(super) fn draw_inspector(f: &mut Frame, app: &App, area: Rect) {
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
}

/// "Now playing": the track the engine is actually playing, distinct from the
/// "selection" panel which follows the cursor.
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
            // line 1: state + live position (millisecond precision) / duration
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
