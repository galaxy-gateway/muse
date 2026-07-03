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
use crate::util::fmt_time;

pub(super) fn draw_inspector(f: &mut Frame, app: &App, area: Rect) {
    let rows = inspector_rows(area);
    draw_now_playing(f, app, rows[0]);
    // The middle panel is a cycleable view (W), like the scope presets.
    match crate::config::WAVE_VIEWS[app.wave_view_idx].0 {
        crate::config::WaveView::Waveform => {
            draw_waveform(f, app, rows[1], app.now_playing.clone(), "waveform", true)
        }
        crate::config::WaveView::Spectrogram => draw_spectrogram(f, app, rows[1]),
        crate::config::WaveView::Bands => draw_bands(f, app, rows[1]),
        crate::config::WaveView::Bitrate => draw_bitrate(f, app, rows[1]),
        crate::config::WaveView::Dynamics => draw_dynamics(f, app, rows[1]),
    }
    draw_scope(f, app, rows[2]);
}

/// Inspector row rects: now-playing (5), waveform (remainder), scope (9). Shared
/// by the renderer and `ui::draw` (which records the rects) so they stay in sync.
pub(super) fn inspector_rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(9),
        ])
        .split(area)
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
            let state: (String, Color) = if app.engine.is_buffering() {
                (
                    format!("{} loading", super::widgets::spinner(app.frame)),
                    t.dim,
                )
            } else if app.engine.is_playing() {
                ("▶ playing".to_string(), t.playing)
            } else {
                ("⏸ paused".to_string(), t.dim)
            };
            // Line 1: state + clock. A live "pos / dur" ticks by default; but a
            // per-frame change on this row makes the adjacent graphics-protocol
            // cover thumbnail flicker on iTerm2, so when that thumbnail is
            // actually on screen we fall back to the static total duration.
            let (pos, dur) = (app.engine.position_secs(), app.engine.duration_secs());
            let cover_flickers = app.show_art && app.graphics_capable;
            let clock = if cover_flickers {
                fmt_time(dur)
            } else {
                format!("{} / {}", fmt_time(pos), fmt_time(dur))
            };
            let time = Line::from(vec![
                Span::styled(
                    format!("{}   ", state.0),
                    Style::default().fg(state.1).add_modifier(Modifier::BOLD),
                ),
                Span::styled(clock, Style::default().fg(t.dim)),
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
    let block = panel(
        "now playing",
        border(t, app.frame, t.playing, 0.28, app.beat_pulse()),
    )
    .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Cover thumbnail on the left (when available), text to its right.
    let off = panel_cover_thumb(
        f,
        app,
        inner,
        app.np_thumb_cols(),
        app.now_playing.as_ref(),
        true,
    );
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

/// Reserve the left `off-1` columns of `inner` for a cover thumbnail (a 1-col
/// gap follows), returning the text x-offset (`off`, or 0 when there's no art).
/// When `protocol` is true on a pixel-graphics terminal the cells are left blank
/// — the crisp protocol image is drawn over them out-of-band by `ui::draw` (only
/// the now-playing panel does this). Otherwise it's filled with the half-block
/// cover here (the selection panel always uses half-blocks).
pub(super) fn panel_cover_thumb(
    f: &mut Frame,
    app: &App,
    inner: Rect,
    off: u16,
    path: Option<&std::path::PathBuf>,
    protocol: bool,
) -> u16 {
    if off < 2 {
        return 0;
    }
    let img = path
        .and_then(|p| app.wave_art.get(p))
        .and_then(|o| o.as_ref());
    let Some(img) = img else { return 0 };
    if !(protocol && app.graphics_capable) {
        let thumb = Rect {
            x: inner.x,
            y: inner.y,
            width: off - 1,
            height: inner.height,
        };
        fill_album_art(f, app, thumb, img, 100);
    }
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

/// Playhead progress fraction for the now-playing track, or -1.0.
fn np_progress(app: &App) -> f64 {
    let d = app.engine.duration_secs();
    if app.now_playing.is_some() && d > 0.0 {
        (app.engine.position_secs() / d).clamp(0.0, 1.0)
    } else {
        -1.0
    }
}

/// Spectrogram strip: time on x, pseudo-frequency on y (bass at the bottom),
/// colored by band energy through a theme-aware heat gradient. Built from the
/// analysis cache (three bands interpolated vertically with triangular
/// weights); static per track, so only the playhead column changes per frame.
fn draw_spectrogram(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let block = panel_hint(
        "spectrogram",
        border(t, app.frame, t.wave, 0.42, app.beat_pulse()),
        "W next view",
        t.dim,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 2 {
        return;
    }
    let analysis = app
        .now_playing
        .as_ref()
        .and_then(|p| app.analysis_cache.get(p));
    let Some(a) = analysis else {
        let msg = if app.now_playing.is_some() {
            "analyzing…"
        } else {
            "nothing playing"
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(t.dim))),
            Rect {
                x: inner.x + 1,
                y: inner.y + inner.height / 2,
                width: inner.width.saturating_sub(1),
                height: 1,
            },
            );
        return;
    };
    let n = a.bands.len().max(1);
    let progress = np_progress(app);
    let cold = crate::color::scale(t.wave, 0.18);
    let buf = f.buffer_mut();
    for cx in 0..inner.width {
        let bi = (cx as usize * n / inner.width as usize).min(n - 1);
        let (b, m, tr) = a.bands[bi];
        let is_played = progress >= 0.0 && (cx as f64 / inner.width as f64) <= progress;
        for cy in 0..inner.height {
            // Pseudo-frequency: bottom row = bass, middle = mid, top = treble,
            // triangular interpolation between the three bands.
            let ff = if inner.height > 1 {
                1.0 - cy as f64 / (inner.height - 1) as f64
            } else {
                0.0
            };
            let wb = (1.0 - ff * 2.0).max(0.0);
            let wm = (1.0 - (ff - 0.5).abs() * 2.0).max(0.0);
            let wt = (ff * 2.0 - 1.0).max(0.0);
            let e = (b as f64 * wb + m as f64 * wm + tr as f64 * wt).clamp(0.0, 1.0);
            // Heat: cold wave -> wave -> playing, quantized for diff stability.
            let e = crate::color::quant(e, 12.0);
            let col = if e < 0.5 {
                mix(cold, t.wave, e * 2.0)
            } else {
                mix(t.wave, t.playing, (e - 0.5) * 2.0)
            };
            let col = if is_played {
                col
            } else {
                crate::color::scale(col, 0.55)
            };
            let cell = &mut buf[(inner.x + cx, inner.y + cy)];
            cell.set_char('█');
            cell.set_fg(col);
        }
    }
    // Playhead column.
    if progress >= 0.0 {
        let px = inner.x + ((progress * (inner.width - 1) as f64).round() as u16);
        for cy in 0..inner.height {
            let cell = &mut buf[(px, inner.y + cy)];
            cell.set_char('│');
            cell.set_fg(t.playing);
        }
    }
}

/// Shared scaffolding for the analysis-backed views: draws the panel block,
/// bails with a status line when there is nothing to draw, and hands the
/// inner rect + analysis + playhead progress to the body closure.
fn with_analysis(
    f: &mut Frame,
    app: &App,
    area: Rect,
    title: &str,
    body: impl FnOnce(&mut Frame, &App, Rect, &crate::audio::TrackAnalysis, f64),
) {
    let t = &app.theme;
    let block = panel_hint(
        title,
        border(t, app.frame, t.wave, 0.42, app.beat_pulse()),
        "W next view",
        t.dim,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.width < 4 || inner.height < 2 {
        return;
    }
    let analysis = app
        .now_playing
        .as_ref()
        .and_then(|p| app.analysis_cache.get(p))
        .cloned();
    match analysis {
        Some(a) => body(f, app, inner, &a, np_progress(app)),
        None => {
            let msg = if app.now_playing.is_some() {
                "analyzing…"
            } else {
                "nothing playing"
            };
            f.render_widget(
                Paragraph::new(Span::styled(msg, Style::default().fg(t.dim))),
                Rect {
                    x: inner.x + 1,
                    y: inner.y + inner.height / 2,
                    width: inner.width.saturating_sub(1),
                    height: 1,
                },
            );
        }
    }
}

/// Draw the playhead column over a filled view.
fn playhead_column(f: &mut Frame, app: &App, inner: Rect, progress: f64) {
    if progress < 0.0 || inner.width == 0 {
        return;
    }
    let px = inner.x + ((progress * (inner.width - 1) as f64).round() as u16);
    let col = app.theme.playing;
    let buf = f.buffer_mut();
    for cy in 0..inner.height {
        let cell = &mut buf[(px, inner.y + cy)];
        cell.set_char('│');
        cell.set_fg(col);
    }
}

/// Stacked band envelopes: bass / mid / treble as three labeled ribbons, a
/// mixing-console read of the track. Uses half-block resolution per ribbon.
fn draw_bands(f: &mut Frame, app: &App, area: Rect) {
    with_analysis(f, app, area, "bands", |f, app, inner, a, progress| {
        let t = &app.theme;
        let n = a.bands.len().max(1);
        let lane_h = (inner.height / 3).max(1);
        let colors = [t.accent, t.wave, t.accent2];
        let labels = ["bass", "mid", "treble"];
        let buf = f.buffer_mut();
        for (lane, (label, col)) in labels.iter().zip(colors).enumerate() {
            let base = inner.y + lane as u16 * lane_h + lane_h - 1;
            for cx in 0..inner.width {
                let bi = (cx as usize * n / inner.width as usize).min(n - 1);
                let e = match lane {
                    0 => a.bands[bi].0,
                    1 => a.bands[bi].1,
                    _ => a.bands[bi].2,
                } as f64;
                let is_played =
                    progress >= 0.0 && (cx as f64 / inner.width as f64) <= progress;
                let col = if is_played {
                    col
                } else {
                    crate::color::scale(col, 0.5)
                };
                // Ribbon height in half-rows.
                let hh = (e * (lane_h as f64 * 2.0 - 1.0)).round() as u16;
                for step in 0..hh {
                    let y = base - step / 2;
                    if y < inner.y {
                        break;
                    }
                    let cell = &mut buf[(inner.x + cx, y)];
                    cell.set_char(if step == hh - 1 && step % 2 == 0 {
                        '▄'
                    } else {
                        '█'
                    });
                    cell.set_fg(col);
                }
            }
            // Lane label on the left, above the baseline.
            for (i, ch) in label.chars().enumerate() {
                let x = inner.x + 1 + i as u16;
                if x < inner.right() {
                    let cell = &mut buf[(x, inner.y + lane as u16 * lane_h)];
                    if cell.symbol() == " " {
                        cell.set_char(ch);
                        cell.set_fg(crate::color::scale(col, 0.7));
                    }
                }
            }
        }
        playhead_column(f, app, inner, progress);
    });
}

/// Bitrate over time (kbps) from compressed packet sizes: a filled area chart
/// with the track average marked. Near-flat for CBR files — which is honest.
fn draw_bitrate(f: &mut Frame, app: &App, area: Rect) {
    with_analysis(f, app, area, "bitrate", |f, app, inner, a, progress| {
        let t = &app.theme;
        let Some(kbps) = &a.bitrate else {
            f.render_widget(
                Paragraph::new(Span::styled(
                    "no timing info in this container",
                    Style::default().fg(t.dim),
                )),
                inner,
            );
            return;
        };
        let n = kbps.len().max(1);
        let peak = kbps.iter().fold(0.0f32, |m, &v| m.max(v)).max(1.0);
        let avg = kbps.iter().sum::<f32>() / n as f32;
        let buf = f.buffer_mut();
        for cx in 0..inner.width {
            let bi = (cx as usize * n / inner.width as usize).min(n - 1);
            let e = (kbps[bi] / peak) as f64;
            let is_played = progress >= 0.0 && (cx as f64 / inner.width as f64) <= progress;
            let col = if is_played {
                t.wave
            } else {
                crate::color::scale(t.wave, 0.5)
            };
            let hh = (e * (inner.height as f64 * 2.0 - 1.0)).round() as u16;
            for step in 0..hh {
                let y = inner.y + inner.height - 1 - step / 2;
                let cell = &mut buf[(inner.x + cx, y)];
                cell.set_char(if step == hh.saturating_sub(1) && step % 2 == 0 {
                    '▄'
                } else {
                    '█'
                });
                cell.set_fg(col);
            }
        }
        // Average line + labels.
        let ay = inner.y + inner.height
            - 1
            - ((avg / peak) as f64 * (inner.height as f64 - 1.0)).round() as u16;
        for cx in 0..inner.width {
            let cell = &mut buf[(inner.x + cx, ay)];
            if cell.symbol() == " " || cell.symbol() == "█" || cell.symbol() == "▄" {
                cell.set_char('┈');
                cell.set_fg(t.playing);
            }
        }
        playhead_column(f, app, inner, progress);
        // Label drawn last so the playhead never cuts through the numbers.
        let label = format!(" avg {avg:.0} kbps · peak {peak:.0} ");
        let buf = f.buffer_mut();
        for (i, ch) in label.chars().enumerate() {
            let x = inner.x + 1 + i as u16;
            if x < inner.right() {
                let cell = &mut buf[(x, inner.y)];
                cell.set_char(ch);
                cell.set_fg(t.media);
            }
        }
    });
}

/// Dynamics: the RMS loudness curve drawn inside the min/max peak envelope in
/// a second tone — compressed masters read as RMS hugging the peaks.
fn draw_dynamics(f: &mut Frame, app: &App, area: Rect) {
    with_analysis(f, app, area, "dynamics", |f, app, inner, a, progress| {
        let t = &app.theme;
        let peaks = app
            .now_playing
            .as_ref()
            .and_then(|p| app.wave_cache.get(p));
        let n = a.rms.len().max(1);
        let mid = inner.y as f64 + inner.height as f64 / 2.0;
        let half = inner.height as f64 / 2.0;
        let buf = f.buffer_mut();
        for cx in 0..inner.width {
            let bi = (cx as usize * n / inner.width as usize).min(n - 1);
            let is_played = progress >= 0.0 && (cx as f64 / inner.width as f64) <= progress;
            let dimf = if is_played { 1.0 } else { 0.5 };
            // Peak envelope (outer, dim) from the waveform cache when present.
            if let Some(pk) = peaks {
                let pbi = (cx as usize * pk.len().max(1) / inner.width as usize)
                    .min(pk.len().saturating_sub(1));
                if let Some(&(lo, hi)) = pk.get(pbi) {
                    let y0 = (mid - hi as f64 * half).round() as i32;
                    let y1 = (mid - lo as f64 * half).round() as i32;
                    for y in y0..=y1 {
                        if y >= inner.y as i32 && y < inner.bottom() as i32 {
                            let cell = &mut buf[(inner.x + cx, y as u16)];
                            cell.set_char('│');
                            cell.set_fg(crate::color::scale(t.wave, 0.35 * dimf));
                        }
                    }
                }
            }
            // RMS body (inner, bright) — symmetric around the midline.
            let r = a.rms[bi] as f64;
            let y0 = (mid - r * half).round() as i32;
            let y1 = (mid + r * half).round() as i32;
            for y in y0..=y1 {
                if y >= inner.y as i32 && y < inner.bottom() as i32 {
                    let cell = &mut buf[(inner.x + cx, y as u16)];
                    cell.set_char('█');
                    cell.set_fg(crate::color::scale(t.accent, dimf));
                }
            }
        }
        playhead_column(f, app, inner, progress);
    });
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
    let block = panel(title, border(t, app.frame, t.wave, 0.42, app.beat_pulse()));
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
    // Snapshot for the theme's per-column wave tint (FrameCtx is Copy; the
    // paint closure is `move`).
    let fx = t.effect;
    let fx_ctx = app.frame_ctx();
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
                    let frac = c as f64 / cols.max(1) as f64;
                    let played = progress >= 0.0 && frac <= progress;
                    ctx.draw(&CLine {
                        x1: x,
                        y1: lo as f64,
                        x2: x,
                        y2: hi as f64,
                        color: fx.wave_tint(t.wave, &fx_ctx, frac, played),
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
            border(t, app.frame, t.scope, 0.56, app.beat_pulse()),
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
            border(t, app.frame, t.scope, 0.56, app.beat_pulse()),
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
