//! Shared rendering helpers: panel blocks, the effect-driven border color, and
//! the seek-bar hover guide.

use ratatui::Frame;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::app::App;
use crate::config::Theme;
use crate::util::{fmt_time, fmt_time_precise};

pub(super) fn panel<'a>(title: &'a str, accent: Color) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ))
}

/// A panel with its relevant key hints docked into the bottom-right border, so
/// the controls are always visible without opening the help overlay.
pub(super) fn panel_hint<'a>(
    title: &'a str,
    accent: Color,
    hint: &'a str,
    dim: Color,
) -> Block<'a> {
    panel(title, accent).title_bottom(
        Line::from(Span::styled(format!(" {hint} "), Style::default().fg(dim))).right_aligned(),
    )
}

/// Border accent for a panel: delegates to the theme's effect, which returns the
/// static `base` or an animated color. `offset` spreads animation across panels.
pub(super) fn border(theme: &Theme, frame: u64, base: Color, offset: f64) -> Color {
    theme.effect.border(base, frame, offset)
}

/// Hover affordance over the seek bars: a vertical guide at the pointer column
/// showing where a click would seek to.
pub(super) fn draw_hover_seek(f: &mut Frame, app: &App) {
    let Some((c, r)) = app.hover else { return };
    let mark = app.theme.playing;
    for rect in [app.wave_rect, app.transport_rect] {
        if rect.width < 3 || rect.height < 2 {
            continue;
        }
        if c <= rect.x || c >= rect.x + rect.width - 1 {
            continue;
        }
        if r < rect.y || r >= rect.y + rect.height {
            continue;
        }
        let buf = f.buffer_mut();
        for y in (rect.y + 1)..(rect.y + rect.height - 1) {
            let cell = &mut buf[(c, y)];
            cell.set_char('│');
            cell.set_fg(mark);
        }
        break;
    }
    draw_scrub_tooltip(f, app);
}

/// While dragging a seek bar, float the target time and the signed delta from
/// where the drag started just above the cursor.
fn draw_scrub_tooltip(f: &mut Frame, app: &App) {
    let Some((c, r, target, delta)) = app.scrub_preview() else {
        return;
    };
    let t = &app.theme;
    let sign = if delta >= 0.0 { "+" } else { "-" };
    let label = format!(
        " {}  {}{} ",
        fmt_time_precise(target),
        sign,
        fmt_time(delta.abs())
    );
    let chars: Vec<char> = label.chars().collect();
    let w = chars.len() as u16;
    let area = f.area();
    // Anchor above the bar cursor when there's room, else just below.
    let y = if r > area.y { r - 1 } else { r + 1 };
    // Keep the label fully on-screen horizontally.
    let x = c.min(area.x + area.width.saturating_sub(w));
    let buf = f.buffer_mut();
    for (i, ch) in chars.into_iter().enumerate() {
        let cx = x + i as u16;
        if cx >= area.x + area.width {
            break;
        }
        let cell = &mut buf[(cx, y)];
        cell.set_char(ch);
        cell.set_fg(t.bg_sel);
        cell.set_bg(t.playing);
        cell.set_style(cell.style().add_modifier(Modifier::BOLD));
    }
}
