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

    /// Relative seek; clears the spectrum so the FFT history doesn't smear
    /// across the discontinuity.
    pub(super) fn seek_rel(&mut self, secs: f64) {
        self.spectrum.clear();
        self.engine.send(TransportCmd::SeekRel(secs));
    }

    fn seek_to(&mut self, frac: f64) {
        let dur = self.engine.duration_secs();
        if dur > 0.0 {
            self.seek_to_secs(frac * dur);
        }
    }

    /// Absolute seek; clears the spectrum (see `seek_rel`).
    pub(super) fn seek_to_secs(&mut self, secs: f64) {
        self.spectrum.clear();
        self.engine.send(TransportCmd::SeekTo(secs));
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

/// Seek fraction (0..1) for a press at (col,row) if it lands inside `r`'s
/// interior — excluding the border rows/columns — else `None`. Mirrors
/// `tree_row_at`'s border exclusion so a click on a panel's title/border row is
/// not treated as a seek.
fn frac_in_rect(r: Rect, col: u16, row: u16) -> Option<f64> {
    if r.width < 4 || r.height < 3 {
        return None;
    }
    if row <= r.y || row >= r.y + r.height - 1 || col <= r.x || col >= r.x + r.width - 1 {
        return None;
    }
    Some(frac_col(r, col))
}

/// Fraction (0..1) of `r`'s inner width at column `col`, clamped — ignores the
/// row, so a drag keeps scrubbing even if the pointer drifts off the bar. The
/// interior spans columns `[r.x+1, r.x+r.width-2]`, so the divisor is the gap
/// between those ends (`width-3`); the rightmost cell must map to exactly 1.0 so
/// the end of the track is reachable.
fn frac_col(r: Rect, col: u16) -> f64 {
    if r.width < 4 {
        return 0.0;
    }
    let left = r.x + 1;
    let right = r.x + r.width - 2;
    let c = col.clamp(left, right);
    (c - left) as f64 / (right - left) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect {
            x: 4,
            y: 2,
            width: w,
            height: h,
        }
    }

    #[test]
    fn frac_col_spans_full_range() {
        let r = rect(12, 3); // interior cols 5..=14
        assert_eq!(frac_col(r, 5), 0.0); // left edge
        assert_eq!(frac_col(r, 14), 1.0); // right edge reaches end-of-track
        assert!((frac_col(r, 9) - 4.0 / 9.0).abs() < 1e-9);
        // off-bar columns clamp, never exceed the bounds
        assert_eq!(frac_col(r, 0), 0.0);
        assert_eq!(frac_col(r, 200), 1.0);
    }

    #[test]
    fn frac_in_rect_excludes_borders() {
        let r = rect(12, 5); // border rows y=2 and y=6; interior rows 3..=5
        assert!(frac_in_rect(r, 8, 2).is_none()); // top border / title row
        assert!(frac_in_rect(r, 8, 6).is_none()); // bottom border
        assert!(frac_in_rect(r, 4, 4).is_none()); // left border col
        assert!(frac_in_rect(r, 15, 4).is_none()); // right border col
        assert!(frac_in_rect(r, 8, 4).is_some()); // interior hit
    }
}
