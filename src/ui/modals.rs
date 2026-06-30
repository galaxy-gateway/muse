//! Centered overlay modals: the help sheet and the theme picker.

use ratatui::Frame;
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
        Line::from("  i                toggle album art (vs. waveform)"),
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
