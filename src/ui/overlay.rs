//! Command palette, right-click menu, and divider hover/drag highlight.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Padding, Paragraph};

use super::centered;
use super::widgets::{border, panel, panel_hint};
use crate::app::{App, CMDS};

pub(super) fn draw_palette(f: &mut Frame, app: &App) {
    let Some(p) = &app.palette else { return };
    let t = &app.theme;
    let area = centered(50, 60, f.area());
    f.render_widget(Clear, area);
    let block = panel_hint(
        "commands",
        border(t, app.frame, t.accent2, 0.14, app.beat_pulse()),
        "↑↓ pick · ⏎ run · esc",
        t.dim,
    )
    .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 2 {
        return;
    }
    let input = Rect { height: 1, ..inner };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(": ", Style::default().fg(t.accent)),
            Span::styled(p.query.as_str(), Style::default().fg(t.media)),
            Span::styled("▏", Style::default().fg(t.accent)),
        ])),
        input,
    );
    let hits = app.palette_hits();
    let items: Vec<ListItem> = hits
        .iter()
        .map(|&i| {
            let (title, key, _) = CMDS[i];
            let pad = (inner.width as usize)
                .saturating_sub(title.chars().count() + key.chars().count() + 2);
            ListItem::new(Line::from(vec![
                Span::styled(title, Style::default().fg(t.media)),
                Span::raw(" ".repeat(pad)),
                Span::styled(key, Style::default().fg(t.dim)),
            ]))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().bg(t.bg_sel).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌");
    let mut st = ListState::default();
    if !hits.is_empty() {
        st.select(Some(p.sel.min(hits.len() - 1)));
    }
    let list_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    f.render_stateful_widget(list, list_area, &mut st);
}

pub(super) fn draw_menu(f: &mut Frame, app: &App) {
    let Some(m) = &app.menu else { return };
    let t = &app.theme;
    f.render_widget(Clear, m.rect);
    let items: Vec<ListItem> = m
        .items
        .iter()
        .map(|(l, _)| ListItem::new(Span::styled(*l, Style::default().fg(t.media))))
        .collect();
    let list = List::new(items)
        .block(panel(
            "",
            border(t, app.frame, t.accent2, 0.14, app.beat_pulse()),
        ))
        .highlight_style(Style::default().bg(t.bg_sel).add_modifier(Modifier::BOLD))
        .highlight_symbol("▌");
    let mut st = ListState::default();
    st.select(Some(m.sel));
    f.render_stateful_widget(list, m.rect, &mut st);
}
