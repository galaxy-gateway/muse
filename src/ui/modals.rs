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
    let knobs = crate::config::theme_knobs(app.theme_sel);
    let configurable = !knobs.is_empty();
    // The modal keeps its original footprint; when the highlighted theme is
    // configurable the knob panel is docked *inside* it on the right (the list
    // squishes to fit) so the options are always visible. Tab moves focus there.
    let area = centered(46, 80, f.area());
    f.render_widget(Clear, area);
    let items: Vec<ListItem> = THEMES
        .iter()
        .enumerate()
        .map(|(i, th)| {
            let swatch = |c| Span::styled("██", Style::default().fg(c));
            let tag = if th.effect.is_animated() {
                "✦ "
            } else {
                "  "
            };
            // Live preview ONLY on the highlighted row: running every row's
            // border animation at once turned the list into a wall of
            // unsynchronized color flashes (glitch-family borders jump to
            // random colors by design). The highlighted theme also previews
            // across the whole UI behind the modal, so one animated row is
            // plenty; every other row shows its static palette.
            let (live1, live2) = if i == app.theme_sel {
                let beat = app.beat_pulse();
                (
                    th.effect.border(th.accent, app.frame, 0.0, beat),
                    th.effect.border(th.accent2, app.frame, 0.4, beat),
                )
            } else {
                (th.accent, th.accent2)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<15}", th.name), Style::default().fg(th.media)),
                Span::styled(tag, Style::default().fg(t.dim)),
                swatch(live1),
                swatch(live2),
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
        "↑↓ pick · a art · m motion · [] level · ⏎"
    } else if app.theme_config {
        "↑↓ pick · tab back"
    } else {
        "↑↓ pick · tab tune · a art · m motion · [] level · ⏎"
    };
    // Title surfaces the global modifiers so their state is never hidden.
    let mut title = String::from("theme");
    if app.adaptive {
        title.push_str(" · art tint");
    }
    if app.no_motion {
        title.push_str(" · no motion");
    }
    if (app.master_intensity - 0.5).abs() > f32::EPSILON {
        title.push_str(&format!(" · level {:.0}%", app.master_intensity * 200.0));
    }
    let title = title.as_str();
    let list = List::new(items)
        .block(
            panel_hint(
                title,
                border(t, app.frame, t.accent2, 0.14, app.beat_pulse()),
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
        draw_theme_editor(f, app, editor_area, &knobs);
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
        // Toggle knobs show on/off; choices the selected option; range knobs
        // a bar + value.
        match k.kind() {
            crate::effects::KnobKind::Toggle => {
                let on = v >= 0.5;
                let (txt, col) = if on {
                    ("  ● on", bar_col)
                } else {
                    ("  ○ off", t.dim)
                };
                lines.push(Line::from(Span::styled(txt, Style::default().fg(col))));
            }
            crate::effects::KnobKind::Choice(opts) => {
                // The palette knob's real colorway names live in config.
                let opts = if *k == crate::effects::Knob::Palette {
                    let names = crate::config::variant_names(THEMES[app.theme_sel].name);
                    if names.len() == opts.len() {
                        names
                    } else {
                        opts
                    }
                } else {
                    opts
                };
                let idx = tuning.choice(*k).min(opts.len() - 1);
                lines.push(Line::from(vec![
                    Span::styled("  ‹ ", Style::default().fg(t.dim)),
                    Span::styled(opts[idx], Style::default().fg(bar_col)),
                    Span::styled(" ›", Style::default().fg(t.dim)),
                ]));
            }
            crate::effects::KnobKind::Range => {
                let filled = (v * cells as f32).round() as usize;
                let bar: String = "█".repeat(filled) + &"░".repeat(cells - filled);
                lines.push(Line::from(vec![
                    Span::styled(format!(" {bar}"), Style::default().fg(bar_col)),
                    Span::styled(format!(" {v:.2}"), Style::default().fg(t.dim)),
                ]));
            }
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
        panel_hint(
            "tune",
            border(t, app.frame, accent, 0.5, app.beat_pulse()),
            hint,
            t.dim,
        )
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
                border(t, app.frame, t.accent2, 0.14, app.beat_pulse()),
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

pub(super) fn draw_help(f: &mut Frame, app: &mut App) {
    let t = app.theme;
    let area = centered(62, 80, f.area());
    f.render_widget(Clear, area);

    // (key, description) rows grouped under section headers. Keys render in
    // an aligned accent column; headers in accent2.
    type Section = (&'static str, &'static [(&'static str, &'static str)]);
    const SECTIONS: &[Section] = &[
        (
            "navigate",
            &[
                ("j / k · ↑ ↓", "move cursor"),
                ("l / →", "expand directory"),
                ("h / ←", "collapse / go to parent"),
                ("g / G", "jump to top / bottom"),
                ("/", "fuzzy find  (⏎ apply · esc reset)"),
                ("c", "jump to the now-playing track"),
            ],
        ),
        (
            "playback",
            &[
                ("⏎", "expand directory or play file"),
                ("space", "play / pause"),
                ("n / p", "next / previous track"),
                ("u", "back to previous song (resumes position)"),
                ("r", "loop mode  off / all / one"),
                ("s", "shuffle on / off  (no-repeat bag, saved)"),
                ("b", "gapless playback on / off  (saved)"),
            ],
        ),
        (
            "queue",
            &[
                ("a / A", "append / play-next the selection"),
                ("Q", "open the queue manager"),
                ("w", "save the queue as .m3u"),
            ],
        ),
        (
            "seek & volume",
            &[
                (", / .", "seek -5s / +5s"),
                ("shift ← / →", "scrub the playhead -1s / +1s"),
                ("click / drag", "seek on the waveform or transport bar"),
                ("- / +", "volume down / up"),
                ("media keys", "play / pause / next / prev (OS)"),
            ],
        ),
        (
            "look & feel",
            &[
                ("t", "theme picker  (⏎ apply · esc cancel)"),
                ("tab", "in picker: tune knobs — palette, beat source, ..."),
                ("a", "in picker: adaptive art tint on / off"),
                ("[ / ]", "in picker: master effect level"),
                ("m", "motion on / off — keep colors, park animation"),
                ("v / V", "cycle visualizer preset  (saved)"),
                ("W", "cycle inspector view  (saved)"),
                ("i", "toggle cover art"),
            ],
        ),
        (
            "other",
            &[
                ("y / Y · ⧉", "copy selection / now-playing path"),
                ("?", "open / close this help"),
                ("q / esc", "quit (or close a modal)"),
            ],
        ),
    ];

    let mut lines: Vec<Line> = vec![Line::from(vec![
        Span::styled(
            format!("muse v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  ·  theme: {}", t.name),
            Style::default().fg(t.dim),
        ),
    ])];
    for (title, rows) in SECTIONS {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("── ", Style::default().fg(t.dim)),
            Span::styled(
                *title,
                Style::default().fg(t.accent2).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " ".to_string() + &"─".repeat(40usize.saturating_sub(title.len())),
                Style::default().fg(t.dim),
            ),
        ]));
        for (key, desc) in *rows {
            // Pad the key column by display width (chars ≈ good enough here).
            let pad = 14usize.saturating_sub(key.chars().count());
            lines.push(Line::from(vec![
                Span::styled(format!("  {key}"), Style::default().fg(t.accent)),
                Span::raw(" ".repeat(pad + 2)),
                Span::styled(*desc, Style::default().fg(t.media)),
            ]));
        }
    }

    // Scroll: clamp to the real content, then hint + indicate when it
    // overflows the (possibly small) window.
    let inner_h = area.height.saturating_sub(4) as usize; // borders + padding
    let max_scroll = lines.len().saturating_sub(inner_h) as u16;
    app.help_scroll = app.help_scroll.min(max_scroll);
    let overflow = max_scroll > 0;
    let title = if overflow {
        format!(
            "help · {}%",
            (app.help_scroll as usize * 100) / max_scroll.max(1) as usize
        )
    } else {
        "help".to_string()
    };
    let hint = if overflow {
        "j/k scroll · g/G ends · q close"
    } else {
        "q / esc to close"
    };
    let p = Paragraph::new(lines).scroll((app.help_scroll, 0)).block(
        panel_hint(
            title.as_str(),
            border(&t, app.frame, t.accent2, 0.84, app.beat_pulse()),
            hint,
            t.dim,
        )
        .padding(Padding::uniform(1)),
    );
    f.render_widget(p, area);
}
