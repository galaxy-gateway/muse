//! Bottom transport bar: play state, track label, progress gauge, volume, loop.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Gauge;

use super::widgets::{border, panel_hint};
use crate::app::{App, LoopMode};
use crate::util::fmt_time;

pub(super) fn draw_transport(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let pos = app.engine.position_secs();
    let dur = app.engine.duration_secs();
    let ratio = if dur > 0.0 {
        (pos / dur).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let state = if app.engine.is_playing() {
        "▶"
    } else {
        "⏸"
    };
    let vol = (app.engine.volume() * 100.0) as u32;

    let title = app
        .now_playing
        .as_ref()
        .map(|p| {
            app.meta_cache
                .get(p)
                .filter(|m| !m.title.is_empty())
                .map(|m| {
                    if m.artist.is_empty() {
                        m.title.clone()
                    } else {
                        format!("{} — {}", m.artist, m.title)
                    }
                })
                .unwrap_or_else(|| {
                    p.file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                })
        })
        .unwrap_or_else(|| "no track".to_string());

    let loop_glyph = match app.loop_mode {
        LoopMode::Off => "→",
        LoopMode::All => "↻",
        LoopMode::One => "↻¹",
    };
    let shuffle = if app.shuffle { "  ⤨shuffle" } else { "" };
    // Gapless is on by default, so only surface the noteworthy off state.
    let gapless = if app.gapless { "" } else { "  ⊘gapless" };
    let queue = if app.queue.is_empty() {
        String::new()
    } else {
        format!("  ≡{}", app.queue.len())
    };
    let label = format!(
        " {state}  {}  {} / {}   vol {vol}%   {loop_glyph} {}{shuffle}{gapless}{queue} ",
        title,
        fmt_time(pos),
        fmt_time(dur),
        app.loop_mode.label(),
    );
    let gauge = Gauge::default()
        .block(panel_hint(
            "transport",
            border(t, app.frame, t.accent, 0.70, app.beat_pulse()),
            "space play · n/p next/prev · r loop · t theme · ,/. seek · -/+ vol · ? help",
            t.dim,
        ))
        .gauge_style(Style::default().fg(t.accent2).bg(t.bg_sel))
        .ratio(ratio)
        // No explicit fg: ratatui swaps fg/bg under the label over the filled
        // part, so the text reads dark-on-fill there and light-on-dark elsewhere.
        .label(Span::styled(
            label,
            Style::default().add_modifier(Modifier::BOLD),
        ));
    f.render_widget(gauge, area);
}
