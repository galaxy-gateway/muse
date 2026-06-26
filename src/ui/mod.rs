//! All rendering. Immediate-mode: reads `App`, draws, never mutates. One module
//! per screen region; `widgets` holds the shared panel/border helpers.

mod inspector;
mod modals;
mod transport;
mod tree;
mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::App;
use inspector::draw_inspector;
use modals::{draw_help, draw_theme_modal};
use transport::draw_transport;
use tree::{draw_selection, draw_tree};
use widgets::draw_hover_seek;

pub fn draw(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(root[0]);

    // Left column: "selection" panel above the file explorer.
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .split(cols[0]);

    // Record click-to-seek hit rects (must mirror draw_inspector's split).
    let inspector = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(9),
        ])
        .split(cols[1]);
    app.wave_rect = inspector[1];
    app.transport_rect = root[1];
    app.scope_rect = inspector[2];
    app.np_rect = inspector[0];
    app.sel_rect = left[0];
    app.screen = f.area();

    draw_selection(f, app, left[0]);
    draw_tree(f, app, left[1]);
    draw_inspector(f, app, cols[1]);
    draw_transport(f, app, root[1]);

    // The active theme's effect paints any overlay (particles, sprites, glitch).
    let ctx = app.frame_ctx();
    app.theme.effect.overlay(f, &app.sim, &ctx);
    draw_hover_seek(f, app);
    if app.show_theme {
        draw_theme_modal(f, app);
    }
    if app.show_help {
        draw_help(f, app);
    }
}

/// Center a `pw`×`ph` (percent) rect within `area` — shared by the modals.
fn centered(pw: u16, ph: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - ph) / 2),
            Constraint::Percentage(ph),
            Constraint::Percentage((100 - ph) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pw) / 2),
            Constraint::Percentage(pw),
            Constraint::Percentage((100 - pw) / 2),
        ])
        .split(v[1])[1]
}
