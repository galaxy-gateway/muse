//! Right column: now-playing header, the static per-track waveform, and the live
//! oscilloscope.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Context, Line as CLine, Points};
use ratatui::widgets::{Padding, Paragraph, Wrap};

use super::widgets::{border, panel, panel_hint};
use crate::app::App;
use crate::color::mix;
use crate::config::{ScopeMode, ScopeStyle, Theme};
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
    // When album art is toggled on and available, render it as a dimmed backdrop
    // with the waveform drawn over it; otherwise the plain braille waveform.
    let art = app
        .show_art
        .then(|| app.now_playing.as_ref().and_then(|p| app.wave_art.get(p)))
        .flatten()
        .and_then(|o| o.as_ref());
    if let Some(img) = art {
        let t = &app.theme;
        let block = panel("waveform", border(t, app.frame, t.wave, 0.42));
        let inner = block.inner(rows[1]);
        f.render_widget(block, rows[1]);
        fill_album_art(f, app, inner, img, 38);
        draw_wave_overlay(f, app, inner);
    } else {
        draw_waveform(f, app, rows[1], app.now_playing.clone(), "waveform", true);
    }
    draw_scope(f, app, rows[2]);
}

/// Fill `inner` with the cover using upper-half-block (`▀`) cells (two pixels
/// per cell: fg = top, bg = bottom), aspect-fit and centered on a themed mat.
/// `dim` (out of 100) scales brightness — a dim backdrop behind the waveform,
/// or full brightness for the detail-panel thumbnails.
pub(super) fn fill_album_art(
    f: &mut Frame,
    app: &App,
    inner: Rect,
    img: &image::RgbImage,
    dim: u16,
) {
    use ratatui::style::Color;

    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let cols = inner.width as u32;
    let rows = inner.height as u32 * 2; // two pixels per cell vertically

    let (iw, ih) = (img.width().max(1), img.height().max(1));
    let scale = (cols as f32 / iw as f32).min(rows as f32 / ih as f32);
    let fw = ((iw as f32 * scale).round() as u32).clamp(1, cols);
    let fh = ((ih as f32 * scale).round() as u32).clamp(1, rows);
    let fitted = image::imageops::resize(img, fw, fh, image::imageops::FilterType::Triangle);
    let ox = (cols - fw) / 2;
    let oy = (rows - fh) / 2;

    let (mr, mg, mb) = crate::color::rgb_of(app.theme.bg_sel);
    let mat = Color::Rgb(
        (mr as u16 * dim / 100) as u8,
        (mg as u16 * dim / 100) as u8,
        (mb as u16 * dim / 100) as u8,
    );
    let at = |px: u32, py: u32| -> Color {
        if px >= ox && px < ox + fw && py >= oy && py < oy + fh {
            let p = fitted.get_pixel(px - ox, py - oy);
            let d = |c: u8| (c as u16 * dim / 100) as u8;
            Color::Rgb(d(p[0]), d(p[1]), d(p[2]))
        } else {
            mat
        }
    };

    let buf = f.buffer_mut();
    for cy in 0..inner.height {
        for cx in 0..inner.width {
            let px = cx as u32;
            let top = at(px, cy as u32 * 2);
            let bot = at(px, cy as u32 * 2 + 1);
            let cell = &mut buf[(inner.x + cx, inner.y + cy)];
            cell.set_char('▀');
            cell.set_fg(top);
            cell.set_bg(bot);
        }
    }
}

/// Draw the static waveform (and live playhead) over an already-painted album
/// backdrop, at cell resolution so the art shows through behind the trace.
fn draw_wave_overlay(f: &mut Frame, app: &App, inner: Rect) {
    let t = &app.theme;
    let Some(np) = app.now_playing.clone() else {
        return;
    };
    let Some(bins) = app.wave_cache.get(&np) else {
        return;
    };
    if inner.width == 0 || inner.height == 0 || bins.is_empty() {
        return;
    }
    let n = bins.len();
    let mid = inner.y as f32 + inner.height as f32 / 2.0;
    let half = inner.height as f32 / 2.0;
    let y_lo = inner.y;
    let y_hi = inner.y + inner.height; // exclusive

    let buf = f.buffer_mut();
    for cx in 0..inner.width {
        let bi = (cx as usize * n / inner.width as usize).min(n - 1);
        let (lo, hi) = bins[bi];
        let top = (mid - hi.clamp(-1.0, 1.0) * half).round() as i32;
        let bot = (mid - lo.clamp(-1.0, 1.0) * half).round() as i32;
        for y in top.min(bot)..=top.max(bot) {
            if y < y_lo as i32 || y >= y_hi as i32 {
                continue;
            }
            let cell = &mut buf[(inner.x + cx, y as u16)];
            cell.set_char('│');
            cell.set_fg(t.wave);
        }
    }

    // Live playhead (only when this is the playing track and it has a duration).
    let dur = app.engine.duration_secs();
    if dur > 0.0 {
        let frac = (app.engine.position_secs() / dur).clamp(0.0, 1.0);
        let px = inner.x + (frac * (inner.width.saturating_sub(1)) as f64).round() as u16;
        for y in y_lo..y_hi {
            let cell = &mut buf[(px.min(inner.x + inner.width - 1), y)];
            cell.set_char('│');
            cell.set_fg(t.playing);
        }
    }
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
    let block =
        panel("now playing", border(t, app.frame, t.playing, 0.28)).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Cover thumbnail on the left (when available), text to its right.
    let off = panel_cover_thumb(f, app, inner, app.np_thumb_cols(), app.now_playing.as_ref());
    let text = Rect {
        x: inner.x + off,
        width: inner.width.saturating_sub(off),
        ..inner
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);

    // Copy-path button: right end of the title row. Overlaid after the
    // paragraph so it sits on top of any long title text.
    draw_copy_button(f, app, app.np_copy_btn_rect(), app.np_copy_flashing());
}

/// Draw a full-brightness cover thumbnail filling the left `off-1` columns of
/// `inner` (a 1-col gap follows), for the track at `path`. Returns the text
/// x-offset to use (`off`, or 0 when there's no art). Shared by the now-playing
/// and selection panels.
pub(super) fn panel_cover_thumb(
    f: &mut Frame,
    app: &App,
    inner: Rect,
    off: u16,
    path: Option<&std::path::PathBuf>,
) -> u16 {
    if off < 2 {
        return 0;
    }
    let img = path
        .and_then(|p| app.wave_art.get(p))
        .and_then(|o| o.as_ref());
    let Some(img) = img else { return 0 };
    let thumb = Rect {
        x: inner.x,
        y: inner.y,
        width: off - 1,
        height: inner.height,
    };
    fill_album_art(f, app, thumb, img, 100);
    off
}

/// Overlay a copy-path button glyph at `btn`: a checkmark while flashing, an
/// accent-bright copy glyph on hover, else dim. Shared by the now-playing and
/// selection headers.
pub(super) fn draw_copy_button(f: &mut Frame, app: &App, btn: Option<Rect>, flashing: bool) {
    let Some(btn) = btn else { return };
    let t = &app.theme;
    let hovered = app
        .hover
        .map(|(c, r)| c >= btn.x && c < btn.x + btn.width && r == btn.y)
        .unwrap_or(false);
    let (glyph, color) = if flashing {
        ("✓", t.playing)
    } else if hovered {
        ("⧉", t.accent)
    } else {
        ("⧉", t.dim)
    };
    let cell = &mut f.buffer_mut()[(btn.x + btn.width / 2, btn.y)];
    cell.set_symbol(glyph);
    cell.set_fg(color);
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
    if preset.style == ScopeStyle::Spectrum {
        draw_spectrum(f, app, area);
        return;
    }

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
                // Spectrum is rendered by draw_spectrum via the early return at
                // the top of draw_scope; this arm is never reached.
                ScopeStyle::Spectrum => {}
            }
        });
    f.render_widget(canvas, area);
}

/// Live FFT spectrum analyzer: per-column bars rendered in braille sub-cells,
/// left/right braille columns packed two interpolated bands per terminal cell.
fn draw_spectrum(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let bands = app.spectrum.bands();
    if bands.is_empty() {
        return;
    }

    f.render_widget(
        panel_hint(
            "spectrum · live",
            border(t, app.frame, t.scope, 0.56),
            "v/V preset",
            t.dim,
        ),
        area,
    );

    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let sub_cols = inner.width as usize * 2;
    let sub_rows = inner.height as usize * 4;
    const DOTS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    let buf = f.buffer_mut();
    for cell_x in 0..inner.width as usize {
        let left = spectrum_value_at(bands, cell_x * 2, sub_cols).clamp(0.0, 1.0);
        let right = spectrum_value_at(bands, cell_x * 2 + 1, sub_cols).clamp(0.0, 1.0);
        let left_h = if left > 0.005 {
            (left * sub_rows as f32).ceil() as usize
        } else {
            0
        };
        let right_h = if right > 0.005 {
            (right * sub_rows as f32).ceil() as usize
        } else {
            0
        };
        let color = spectrum_color(t, left.max(right));

        for cell_y in 0..inner.height as usize {
            let mut mask = 0u8;
            for row in 0..4 {
                let sub_y_from_top = cell_y * 4 + row;
                let sub_y_from_bottom = sub_rows - 1 - sub_y_from_top;
                if sub_y_from_bottom < left_h {
                    mask |= DOTS[0][row];
                }
                if sub_y_from_bottom < right_h {
                    mask |= DOTS[1][row];
                }
            }

            let cell = &mut buf[(inner.x + cell_x as u16, inner.y + cell_y as u16)];
            if mask == 0 {
                cell.set_char(' ');
                cell.set_fg(t.dim);
            } else {
                cell.set_char(char::from_u32(0x2800 + mask as u32).unwrap_or(' '));
                cell.set_fg(color);
            }
        }
    }
}

/// Ramp a 0..1 band magnitude through dim → scope → playing for a heat-map feel.
fn spectrum_color(theme: &Theme, value: f32) -> Color {
    let (a, b, t) = if value < 0.5 {
        (theme.dim, theme.scope, value * 2.0)
    } else {
        (theme.scope, theme.playing, (value - 0.5) * 2.0)
    };
    mix(a, b, t as f64)
}

/// Magnitude for one sub-column: the peak over the band span it covers. Taking
/// the max (rather than point-sampling) means a narrow spectral line is never
/// dropped when there are fewer sub-columns than bands on a narrow panel.
fn spectrum_value_at(bands: &[f32], sub_x: usize, sub_cols: usize) -> f32 {
    let n = bands.len();
    if n == 0 {
        return 0.0;
    }
    if sub_cols <= 1 {
        return bands[0];
    }
    let lo = sub_x * n / sub_cols;
    let hi = ((sub_x + 1) * n).div_ceil(sub_cols).clamp(lo + 1, n);
    bands[lo..hi].iter().copied().fold(0.0, f32::max)
}
