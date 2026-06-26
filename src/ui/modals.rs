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
        Line::from("  r                loop mode (off / all / one)"),
        Line::from("  b                gapless playback on / off (saved)"),
        Line::from("  , / .            seek -5s / +5s"),
        Line::from("  shift ← / →      scrub playhead -1s / +1s"),
        Line::from("  click tree       folder: fold · file: play"),
        Line::from("  click/drag bar   seek / scrub the playhead"),
        Line::from("  media keys       play/pause/next/prev (OS)"),
        Line::from("  - / +            volume down / up"),
        Line::from("  v / V            cycle visualizer preset (saved)"),
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
