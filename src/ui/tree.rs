//! Left column: the file-explorer tree (or flat fuzzy results) and the selection
//! detail panel above it.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Padding, Paragraph, Wrap};

use super::widgets::{border, panel, spinner};
use crate::app::App;
use crate::color::rgb_of;
use crate::util::{fmt_size, fmt_time};

pub(super) fn draw_tree(f: &mut Frame, app: &mut App, area: Rect) {
    app.tree_rect = area;
    let t = &app.theme;
    let bc = border(t, app.frame, t.accent, 0.0, app.beat_pulse());
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
    let hint = if app.scanning() && !filtering {
        scan_hint(app, t.accent2, t.dim)
    } else if filtering {
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
                // A dir still awaiting its background scan renders dim, so it's
                // clear which folders are confirmed to hold music and which are
                // still being counted.
                let pending_dir = n.is_dir && n.pending;
                let (icon, color) = if n.is_dir {
                    let c = if pending_dir { t.dim } else { t.dir };
                    (if n.expanded { "▾ " } else { "▸ " }, c)
                } else if playing {
                    ("♪ ", t.playing)
                } else {
                    ("· ", t.media)
                };
                let icon_color = if pending_dir {
                    t.dim
                } else if n.is_dir {
                    t.accent2
                } else {
                    color
                };
                // Spinner in place of the ♪ while the now-playing track is still
                // buffering (playing but nothing decoded yet).
                let buffering_row = playing && app.engine.is_buffering();
                let icon_span = if buffering_row {
                    Span::styled(
                        format!("{} ", spinner(app.frame)),
                        Style::default().fg(t.accent2),
                    )
                } else {
                    Span::styled(icon, Style::default().fg(icon_color))
                };
                let mut style = Style::default().fg(color);
                if playing {
                    style = style.add_modifier(Modifier::BOLD);
                }
                let meta = if n.is_dir && n.pending {
                    "  …".to_string() // recursive stats still being counted
                } else if n.is_dir {
                    let items = format!("{} track{}", n.count, if n.count == 1 { "" } else { "s" });
                    if n.size > 0 {
                        format!("  {items} · {}", fmt_size(n.size))
                    } else {
                        format!("  {items}")
                    }
                } else if n.size > 0 {
                    format!("  {}", fmt_size(n.size))
                } else {
                    String::new()
                };
                finish(
                    idx,
                    vec![
                        Span::raw(indent),
                        icon_span,
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

pub(super) fn draw_selection(f: &mut Frame, app: &App, area: Rect) {
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
    let block = panel(
        "selection",
        border(t, app.frame, t.accent2, 0.14, app.beat_pulse()),
    )
    .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Cover thumbnail on the left (when available), detail text to its right.
    // Half-blocks (protocol = false): only the now-playing panel uses a graphics
    // image, so at most one is on screen at a time.
    let off = super::inspector::panel_cover_thumb(
        f,
        app,
        inner,
        app.sel_thumb_cols(),
        path.as_ref(),
        false,
    );
    let text = Rect {
        x: inner.x + off,
        width: inner.width.saturating_sub(off),
        ..inner
    };
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);

    // Copy-path button on the selection title row (first inner row).
    super::inspector::draw_copy_button(f, app, app.sel_copy_btn_rect(), app.sel_copy_flashing());
}

/// Tongue-in-cheek status lines shown while the library scan runs.
const SCAN_QUIPS: [&str; 8] = [
    "rummaging through your questionable taste",
    "counting the bangers",
    "bribing the filesystem",
    "alphabetizing the chaos",
    "waking the hamsters",
    "judging your folder names",
    "summoning the beats",
    "untangling the headphones",
];

/// Startup scan progress for the tree's bottom border: a spinner + rotating quip
/// + a text progress bar + `done/total`.
fn scan_hint(app: &App, accent: Color, dim: Color) -> Line<'static> {
    const SPINNER: [char; 4] = ['⠋', '⠙', '⠹', '⠸'];
    let (done, total) = (app.scan_done, app.scan_total.max(1));
    let ratio = (done as f64 / total as f64).clamp(0.0, 1.0);
    const CELLS: usize = 8;
    let filled = (ratio * CELLS as f64).round() as usize;
    let bar: String = "█".repeat(filled) + &"░".repeat(CELLS - filled);
    // Advance the spinner + quip off the redraw counter (~60Hz).
    let spin = SPINNER[(app.frame / 8) as usize % SPINNER.len()];
    let quip = SCAN_QUIPS[(app.frame / 90) as usize % SCAN_QUIPS.len()];
    Line::from(vec![
        Span::styled(
            format!(" {spin} {quip} "),
            Style::default().fg(dim).add_modifier(Modifier::ITALIC),
        ),
        Span::styled(format!("[{bar}] "), Style::default().fg(accent)),
        Span::styled(format!("{done}/{total} "), Style::default().fg(dim)),
    ])
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
