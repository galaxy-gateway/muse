//! Drag-to-resize pane dividers: the vertical split between the left column
//! (selection + file tree) and the inspector, and the horizontal split between
//! the inspector's wave pane and the oscilloscope.
//!
//! Sizes are stored as user *requests* (`tree_frac`, `scope_rows`) and clamped
//! against the actual terminal size at layout time, so shrinking the window
//! never loses the preferred size and the panes never collapse.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;

/// Default left-column share of the screen width.
pub const TREE_FRAC_DEFAULT: f32 = 0.42;
/// Allowed left-column share range (before the per-side minimum widths apply).
pub const TREE_FRAC_MIN: f32 = 0.15;
pub const TREE_FRAC_MAX: f32 = 0.85;
/// Minimum width (cells) of the left column and the inspector.
pub const MIN_TREE_COLS: u16 = 20;
pub const MIN_INSPECTOR_COLS: u16 = 30;

/// Default oscilloscope height (rows, incl. borders).
pub const SCOPE_ROWS_DEFAULT: u16 = 9;
/// Allowed oscilloscope height range (rows, incl. borders).
pub const SCOPE_ROWS_MIN: u16 = 4;
pub const SCOPE_ROWS_MAX: u16 = 60;
/// Now-playing header height and the wave pane's minimum height.
pub const NP_ROWS: u16 = 5;
pub const MIN_WAVE_ROWS: u16 = 5;

/// Two presses on the same divider within this window reset it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// A draggable pane divider.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Divider {
    /// Vertical: left column (selection + tree) | inspector.
    Tree,
    /// Horizontal: wave pane / oscilloscope.
    Scope,
}

/// Left-column width (cells) for a `total`-wide row given the requested share.
/// Clamps the share to `[TREE_FRAC_MIN, TREE_FRAC_MAX]`, then keeps both sides
/// at least their minimum width when the screen is wide enough for that.
pub fn tree_cols(total: u16, frac: f32) -> u16 {
    let frac = frac.clamp(TREE_FRAC_MIN, TREE_FRAC_MAX);
    let want = (total as f32 * frac).round() as u16;
    if total < MIN_TREE_COLS + MIN_INSPECTOR_COLS {
        // Too narrow for the minimums: honour the share alone.
        return want.min(total);
    }
    want.clamp(MIN_TREE_COLS, total - MIN_INSPECTOR_COLS)
}

/// Oscilloscope height (rows) for an inspector `height` rows tall, given the
/// requested height. Keeps the now-playing header and a minimal wave pane.
pub fn scope_rows(height: u16, want: u16) -> u16 {
    let want = want.clamp(SCOPE_ROWS_MIN, SCOPE_ROWS_MAX);
    let max = height.saturating_sub(NP_ROWS + MIN_WAVE_ROWS);
    want.min(max.max(SCOPE_ROWS_MIN.min(height)))
}

/// Requested left-column share for a drag that puts the divider (the left
/// column's right border) at column `col` of a row spanning `area`.
fn tree_frac_at(area: Rect, col: u16) -> f32 {
    if area.width == 0 {
        return TREE_FRAC_DEFAULT;
    }
    let w = (col.saturating_sub(area.x) + 1).min(area.width);
    let w = tree_cols(area.width, w as f32 / area.width as f32);
    w as f32 / area.width as f32
}

/// Requested scope height for a drag that puts the scope's top border at `row`
/// of an inspector spanning `insp`.
fn scope_rows_at(insp: Rect, row: u16) -> u16 {
    let bottom = insp.y + insp.height;
    scope_rows(insp.height, bottom.saturating_sub(row))
}

impl App {
    /// The divider under (col,row), if any. The vertical divider is the two
    /// touching border columns of the left column and the inspector; the
    /// horizontal one is the wave pane's bottom border plus the scope's top
    /// border (title row). Neither overlaps a seek/click target.
    pub fn divider_at(&self, col: u16, row: u16) -> Option<Divider> {
        let (l, i) = (self.left_rect, self.inspector_rect);
        if l.width > 0 && i.width > 0 {
            let edge = l.x + l.width;
            let in_rows = row >= l.y && row < l.y + l.height;
            if in_rows && (col + 1 == edge || col == i.x) {
                return Some(Divider::Tree);
            }
        }
        let (w, s) = (self.wave_rect, self.scope_rect);
        if w.height > 0 && s.height > 0 {
            let in_cols = col >= s.x && col < s.x + s.width;
            if in_cols && (row + 1 == w.y + w.height || row == s.y) {
                return Some(Divider::Scope);
            }
        }
        None
    }

    /// Divider to highlight: the one being dragged, else the one hovered.
    pub fn divider_highlight(&self) -> Option<Divider> {
        if self.show_help || self.show_theme || self.show_queue {
            return None;
        }
        self.dragging_divider
            .or_else(|| self.hover.and_then(|(c, r)| self.divider_at(c, r)))
    }

    /// Left-press on a divider: start a drag, or reset on a double-click.
    /// Returns false when (col,row) isn't on a divider.
    pub(super) fn divider_press(&mut self, col: u16, row: u16) -> bool {
        let Some(d) = self.divider_at(col, row) else {
            return false;
        };
        let now = Instant::now();
        let double = self
            .divider_click
            .is_some_and(|(prev, t)| prev == d && now.duration_since(t) < DOUBLE_CLICK);
        if double {
            self.divider_click = None;
            self.dragging_divider = None;
            match d {
                Divider::Tree => self.tree_frac = TREE_FRAC_DEFAULT,
                Divider::Scope => self.scope_rows = SCOPE_ROWS_DEFAULT,
            }
            self.persist();
        } else {
            self.divider_click = Some((d, now));
            self.dragging_divider = Some(d);
        }
        true
    }

    /// Left-drag while a divider is held: move it to the pointer.
    pub(super) fn divider_drag(&mut self, col: u16, row: u16) -> bool {
        let Some(d) = self.dragging_divider else {
            return false;
        };
        match d {
            Divider::Tree => {
                let row_area = Rect {
                    x: self.left_rect.x,
                    width: self.left_rect.width + self.inspector_rect.width,
                    ..self.left_rect
                };
                self.tree_frac = tree_frac_at(row_area, col);
            }
            Divider::Scope => self.scope_rows = scope_rows_at(self.inspector_rect, row),
        }
        true
    }

    /// Left-release: end a divider drag and persist the new size.
    pub(super) fn divider_release(&mut self) {
        if self.dragging_divider.take().is_some() {
            self.persist();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_cols_default_and_clamps() {
        assert_eq!(tree_cols(100, TREE_FRAC_DEFAULT), 42);
        // share clamps to the allowed range, then per-side minimums apply
        assert_eq!(tree_cols(100, 0.0), MIN_TREE_COLS);
        assert_eq!(tree_cols(100, 1.0), 100 - MIN_INSPECTOR_COLS);
        assert_eq!(tree_cols(200, 1.0), 170); // 0.85 share
        // too narrow for both minimums: share only, never exceeds the width
        assert_eq!(tree_cols(40, 0.5), 20);
        assert!(tree_cols(3, 0.85) <= 3);
    }

    #[test]
    fn scope_rows_clamps_to_inspector() {
        assert_eq!(scope_rows(40, SCOPE_ROWS_DEFAULT), 9);
        assert_eq!(scope_rows(40, 1), SCOPE_ROWS_MIN);
        assert_eq!(scope_rows(40, 100), 30); // 40 - np(5) - wave(5)
        assert_eq!(scope_rows(12, 9), SCOPE_ROWS_MIN); // tiny: floor at min
        assert_eq!(scope_rows(2, 9), 2); // never taller than the area
    }

    #[test]
    fn drag_positions_map_to_sizes() {
        let row = Rect::new(0, 0, 100, 30);
        // divider (left column's right border) at col 49 -> 50 cols wide
        assert!((tree_frac_at(row, 49) - 0.5).abs() < 1e-6);
        // past the right edge clamps to leave the inspector its minimum
        assert!((tree_frac_at(row, 99) - 0.70).abs() < 1e-6);
        let insp = Rect::new(42, 0, 58, 30);
        assert_eq!(scope_rows_at(insp, 21), 9); // top border at row 21 of 30
        assert_eq!(scope_rows_at(insp, 0), 20); // clamps to keep np + wave
        assert_eq!(scope_rows_at(insp, 29), SCOPE_ROWS_MIN);
    }
}
