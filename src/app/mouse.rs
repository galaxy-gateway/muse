//! Mouse handling: wheel scrolls the list, clicks/drag on the seek bars scrub,
//! clicks on the tree select + activate a row.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::audio::TransportCmd;

impl App {
    /// Mouse handling: left-press on the waveform/transport seeks (and a left
    /// drag scrubs); left-press on the tree selects the row and activates it
    /// (toggle a folder, play a file).
    pub(super) fn on_mouse(&mut self, m: MouseEvent) {
        let (col, row) = (m.column, m.row);
        self.hover = Some((col, row));
        match m.kind {
            MouseEventKind::ScrollDown => {
                self.move_cursor(3);
                let ctx = self.frame_ctx();
                self.theme.effect.on_scroll(&mut self.sim, &ctx);
            }
            MouseEventKind::ScrollUp => {
                self.move_cursor(-3);
                let ctx = self.frame_ctx();
                self.theme.effect.on_scroll(&mut self.sim, &ctx);
            }
            MouseEventKind::ScrollRight => self.tree_hscroll = (self.tree_hscroll + 2).min(80),
            MouseEventKind::ScrollLeft => self.tree_hscroll = self.tree_hscroll.saturating_sub(2),
            MouseEventKind::Down(MouseButton::Left) => {
                // Every click triggers the theme's click interaction at the pointer.
                let ctx = self.frame_ctx();
                self.theme.effect.on_click(&mut self.sim, &ctx, col, row);
                // Seek bars take priority; start a scrub if pressed on one.
                for rect in [self.wave_rect, self.transport_rect] {
                    if frac_in_rect(rect, col, row).is_some() {
                        self.seeking_rect = Some(rect);
                        self.seek_to(frac_col(rect, col));
                        return;
                    }
                }
                // Otherwise, a click in the tree selects + activates that row.
                if let Some(idx) = self.tree_row_at(col, row) {
                    self.list_state.select(Some(idx));
                    self.on_selection_changed();
                    self.enter();
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(rect) = self.seeking_rect {
                    self.seek_to(frac_col(rect, col));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.seeking_rect = None,
            _ => {}
        }
    }

    fn seek_to(&self, frac: f64) {
        let dur = self.engine.duration_secs();
        if dur > 0.0 {
            self.engine.send(TransportCmd::SeekTo(frac * dur));
        }
    }

    /// Visible-list index for a click at (col,row) inside the tree panel.
    pub fn tree_row_at(&self, col: u16, row: u16) -> Option<usize> {
        let r = self.tree_rect;
        if r.width < 3 || r.height < 3 {
            return None;
        }
        if col <= r.x || col >= r.x + r.width - 1 {
            return None;
        }
        if row <= r.y || row >= r.y + r.height - 1 {
            return None;
        }
        let idx = (row - (r.y + 1)) as usize + self.list_state.offset();
        (idx < self.list_len()).then_some(idx)
    }
}

/// Fraction (0..1) along `r`'s inner (inside-border) width at column `col`, if
/// the point is inside the rect; `None` otherwise.
fn frac_in_rect(r: Rect, col: u16, row: u16) -> Option<f64> {
    if r.width < 3 || r.height < 2 {
        return None;
    }
    if row < r.y || row >= r.y + r.height || col <= r.x || col >= r.x + r.width - 1 {
        return None;
    }
    let left = r.x + 1;
    let span = (r.width - 2) as f64;
    Some(((col - left) as f64 / span).clamp(0.0, 1.0))
}

/// Fraction (0..1) of `r`'s inner width at column `col`, clamped — ignores the
/// row, so a drag keeps scrubbing even if the pointer drifts off the bar.
fn frac_col(r: Rect, col: u16) -> f64 {
    if r.width < 3 {
        return 0.0;
    }
    let left = r.x + 1;
    let right = r.x + r.width - 2;
    let c = col.clamp(left, right);
    (c - left) as f64 / (r.width - 2) as f64
}
