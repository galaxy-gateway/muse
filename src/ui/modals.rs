//! Centered overlay modals: the help sheet and the theme picker.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Padding, Paragraph};

use super::centered;
use super::widgets::{border, panel_hint};
use crate::app::App;
use crate::config::THEMES;

/// Theme picker: list of themes with color swatches; the highlighted row is
/// live-previewed in the rest of the UI. Navigated with j/k, applied with Enter.
pub(super) fn draw_theme_modal(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let knobs = THEMES[app.theme_sel].effect.knobs();
    let configurable = !knobs.is_empty();
    // The modal keeps its original footprint; when the highlighted theme is
    // configurable the knob panel is docked *inside* it on the right (the list
    // squishes to fit) so the options are always visible. Tab moves focus there.
    let area = centered(46, 80, f.area());
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = THEMES
        .iter()
        .map(|th| {
            let swatch = |c| Span::styled("██", Style::default().fg(c));
            let tag = if th.effect.is_animated() {
                "✦ "
            } else {
                "  "
            };
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
    // Dock the knob editor as a right-hand column within the same area; the list
    // keeps the left. Only when it can't reasonably fit do we drop it.
    let editor_w = (area.width * 2 / 5).clamp(16, 26);
    let show_editor = configurable && area.width >= editor_w + 20;
    let (list_area, editor_area) = if show_editor {
        (
            Rect {
                width: area.width - editor_w,
                ..area
            },
            Rect {
                x: area.x + area.width - editor_w,
                width: editor_w,
                ..area
            },
        )
    } else {
        (area, Rect::default())
    };

    let list_hint = if !configurable {
        "↑↓ pick · ⏎ apply · esc cancel"
    } else if app.theme_config {
        "↑↓ pick · tab back"
    } else {
        "↑↓ pick · tab tune · ⏎ · esc"
    };
    let list = List::new(items)
        .block(
            panel_hint(
                "theme",
                border(t, app.frame, t.accent2, 0.14),
                list_hint,
                t.dim,
            )
            .padding(Padding::horizontal(1)),
        )
        .highlight_style(Style::default().bg(t.bg_sel).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌");
    let mut state = ListState::default();
    state.select(Some(app.theme_sel));
    f.render_stateful_widget(list, list_area, &mut state);

    if show_editor {
        draw_theme_editor(f, app, editor_area, knobs);
    }
}

/// The knob-editor column (docked right of the theme list): one labelled bar
/// per tunable knob, live values.
fn draw_theme_editor(f: &mut Frame, app: &App, area: Rect, knobs: &[crate::effects::Knob]) {
    let t = &app.theme;
    let tuning = app.tunings[app.theme_sel];
    let focused = app.theme_config;
    // Stacked layout (label line, then a bar+value line) so it fits the narrow
    // column without overflowing the modal's unchanged footprint. The bar fills
    // whatever width remains after a 2-space indent and the value readout.
    let inner = area.width.saturating_sub(4) as usize; // borders + horizontal padding
    let cells = inner.saturating_sub(2 + 5).clamp(3, 16); // indent + " 0.00"
    let bar_col = if focused { t.scope } else { t.dim };
    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, k) in knobs.iter().enumerate() {
        let v = tuning.get(*k);
        let sel = focused && i == app.config_sel;
        let marker = if sel { "▌" } else { " " };
        let label_col = if sel {
            t.accent
        } else if focused {
            t.media
        } else {
            t.dim
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{}", k.label()),
            Style::default().fg(label_col),
        )));
        // Toggle knobs show on/off; range knobs a bar + value.
        if k.kind() == crate::effects::KnobKind::Toggle {
            let on = v >= 0.5;
            let (txt, col) = if on {
                ("  ● on", bar_col)
            } else {
                ("  ○ off", t.dim)
            };
            lines.push(Line::from(Span::styled(txt, Style::default().fg(col))));
        } else {
            let filled = (v * cells as f32).round() as usize;
            let bar: String = "█".repeat(filled) + &"░".repeat(cells - filled);
            lines.push(Line::from(vec![
                Span::styled(format!(" {bar}"), Style::default().fg(bar_col)),
                Span::styled(format!(" {v:.2}"), Style::default().fg(t.dim)),
            ]));
        }
        lines.push(Line::from(""));
    }
    // Trailing "reset to default" row — selectable, resets on Enter.
    let reset_sel = focused && app.config_sel == knobs.len();
    let (reset_marker, reset_col) = if reset_sel {
        ("▌", t.accent)
    } else if focused {
        (" ", t.media)
    } else {
        (" ", t.dim)
    };
    let reset_label = if reset_sel {
        "↩ reset to default  ⏎"
    } else {
        "↩ reset to default"
    };
    lines.push(Line::from(Span::styled(
        format!("{reset_marker}{reset_label}"),
        Style::default().fg(reset_col),
    )));
    let hint = if focused {
        "↑↓ ←→ · esc back"
    } else {
        "tab to tune"
    };
    let accent = if focused { t.accent } else { t.dim };
    let p = Paragraph::new(lines).block(
        panel_hint("tune", border(t, app.frame, accent, 0.5), hint, t.dim)
            .padding(Padding::horizontal(1)),
    );
    f.render_widget(p, area);
}

/// Queue manager: the ordered play queue with the now-playing track marked.
/// Navigated/edited with j/k, J/K, x, X, Enter, w.
pub(super) fn draw_queue_modal(f: &mut Frame, app: &App) {
    let t = &app.theme;
    let area = centered(60, 80, f.area());
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = if app.queue.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "queue empty — 'a' append, 'A' play-next from the tree",
            Style::default().fg(t.dim),
        )))]
    } else {
        app.queue
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let playing = app.now_playing.as_deref() == Some(p.as_path());
                let color = if playing { t.playing } else { t.media };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:>3} ", i + 1), Style::default().fg(t.dim)),
                    Span::styled(
                        if playing { "♪ " } else { "  " },
                        Style::default().fg(color),
                    ),
                    Span::styled(name, Style::default().fg(color)),
                ]))
            })
            .collect()
    };
    let list = List::new(items)
        .block(
            panel_hint(
                "queue",
                border(t, app.frame, t.accent2, 0.14),
                "j/k move · J/K reorder · x del · X clear · ⏎ play · w save · esc",
                t.dim,
            )
            .padding(Padding::horizontal(1)),
        )
        .highlight_style(Style::default().bg(t.bg_sel).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌");
    let mut state = ListState::default();
    if !app.queue.is_empty() {
        state.select(Some(app.queue_sel.min(app.queue.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

pub(super) fn draw_help(f: &mut Frame, app: &App) {
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
        Line::from("  u                back to previous song (resumes position)"),
        Line::from("  a / A            queue: append / play-next the selection"),
        Line::from("  Q                open queue manager   w  save queue (.m3u)"),
        Line::from("  c                jump cursor to the now-playing track"),
        Line::from("  r                loop mode (off / all / one)"),
        Line::from("  s                shuffle on / off (no-repeat, saved)"),
        Line::from("  b                gapless playback on / off (saved)"),
        Line::from("  y / Y or ⧉       copy selection / now-playing path"),
        Line::from("  , / .            seek -5s / +5s"),
        Line::from("  shift ← / →      scrub playhead -1s / +1s"),
        Line::from("  click tree       folder: fold · file: play"),
        Line::from("  click/drag bar   seek / scrub the playhead"),
        Line::from("  media keys       play/pause/next/prev (OS)"),
        Line::from("  - / +            volume down / up"),
        Line::from("  v / V            cycle visualizer preset (saved)"),
        Line::from("  i                toggle cover art (off by default)"),
        Line::from(format!(
            "  t                theme picker — {} (saved)",
            t.name
        )),
        Line::from("  tab (in picker)  tune glitch themes — intensity / etc."),
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
