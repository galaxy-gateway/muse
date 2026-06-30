//! Mouse handling: wheel scrolls the list, clicks/drag on the seek bars scrub,
//! clicks on the tree select + activate a row.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::audio::TransportCmd;

/// Frames the copy button shows a checkmark after a successful copy (~1.5s@60Hz).
pub const COPY_FLASH_FRAMES: u64 = 90;

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
                // Copy-path buttons in the selection / now-playing headers.
                if hit(self.sel_copy_btn_rect(), col, row) {
                    self.copy_selection_path();
                    return;
                }
                if hit(self.np_copy_btn_rect(), col, row) {
                    self.copy_now_playing_path();
                    return;
                }
                // Seek bars take priority; start a scrub if pressed on one.
                for rect in [self.wave_rect, self.transport_rect] {
                    if frac_in_rect(rect, col, row).is_some() {
                        self.seeking_rect = Some(rect);
                        self.seek_start_secs = Some(self.engine.position_secs());
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
            MouseEventKind::Up(MouseButton::Left) => {
                self.seeking_rect = None;
                self.seek_start_secs = None;
            }
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

    /// Screen rect of the now-playing copy-path button (right end of its title
    /// row, which is the 2nd inner row: border, time, title). `None` when
    /// nothing is playing or the panel is too small. Derived from `np_rect` so
    /// the renderer and hit-test agree without a draw-time write-back.
    pub fn np_copy_btn_rect(&self) -> Option<Rect> {
        let name = self.now_playing_title_text()?;
        copy_btn_after(self.np_rect, 2, name.chars().count(), self.np_thumb_cols())
    }

    /// Width (cells, incl. a 1-col gap) of the now-playing cover thumbnail, or 0.
    pub fn np_thumb_cols(&self) -> u16 {
        let has = self
            .now_playing
            .as_ref()
            .is_some_and(|p| matches!(self.wave_art.get(p), Some(Some(_))));
        thumb_cols(self.np_rect, has)
    }

    /// Width (cells, incl. a 1-col gap) of the selection cover thumbnail, or 0.
    pub fn sel_thumb_cols(&self) -> u16 {
        let has = self
            .cursor_path()
            .is_some_and(|p| matches!(self.wave_art.get(&p), Some(Some(_))));
        thumb_cols(self.sel_rect, has)
    }

    /// Screen rect of the selection copy-path button (right end of its title
    /// row, the 1st inner row). `None` when the cursor has no path or the panel
    /// is too small.
    pub fn sel_copy_btn_rect(&self) -> Option<Rect> {
        let name = self.selection_title_text()?;
        copy_btn_after(
            self.sel_rect,
            1,
            name.chars().count(),
            self.sel_thumb_cols(),
        )
    }

    /// Whether the now-playing copy button should show its "copied" checkmark.
    pub fn np_copy_flashing(&self) -> bool {
        flashing(self.copy_flash_np, self.frame)
    }

    /// Whether the selection copy button should show its "copied" checkmark.
    pub fn sel_copy_flashing(&self) -> bool {
        flashing(self.copy_flash_sel, self.frame)
    }

    /// During an active scrub, the cursor cell plus the target time and the
    /// signed delta from where the drag began — for the drag tooltip. `None`
    /// when not scrubbing or the track has no duration.
    pub fn scrub_preview(&self) -> Option<(u16, u16, f64, f64)> {
        let rect = self.seeking_rect?;
        let (c, r) = self.hover?;
        let dur = self.engine.duration_secs();
        if dur <= 0.0 {
            return None;
        }
        let target = frac_col(rect, c) * dur;
        let start = self.seek_start_secs.unwrap_or(target);
        Some((c, r, target, target - start))
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

/// Copy-button cell placed one space after the title text on a panel's title
/// row, or `None` when the panel is too small. `title_row` is the title's offset
/// from the panel top (1 = first inner row, 2 = second, …); `name_cols` is the
/// title's display width; `text_off` is the cover-thumbnail offset that pushes
/// the title text right. The panel has a 1-col border + 1-col padding, so text
/// starts at `panel.x + 2 + text_off`; the icon clamps to the last interior
/// column for very long titles so it never lands on the border.
fn copy_btn_after(panel: Rect, title_row: u16, name_cols: usize, text_off: u16) -> Option<Rect> {
    if panel.width < 6 || panel.height < title_row + 2 {
        return None;
    }
    let text_x = panel.x + 2 + text_off;
    let last_inner = panel.x + panel.width - 2; // last col before the right border
    let x = (text_x as usize + name_cols + 1).min(last_inner as usize) as u16;
    Some(Rect {
        x,
        y: panel.y + title_row,
        width: 1,
        height: 1,
    })
}

/// Width (cells, including a trailing 1-col gap) of a panel's cover thumbnail,
/// or 0 when there is no art or the panel is too small. The thumbnail is a
/// roughly-square half-block image filling the panel's left edge.
fn thumb_cols(panel: Rect, has_art: bool) -> u16 {
    if !has_art {
        return 0;
    }
    let inner_w = panel.width.saturating_sub(4); // borders + 1-col padding each side
    let inner_h = panel.height.saturating_sub(2);
    if inner_h == 0 || inner_w < 12 {
        return 0;
    }
    ((inner_h * 2).min(inner_w / 3).max(1)) + 1
}

/// True while `stamp` is within the flash window of the current `frame`.
fn flashing(stamp: Option<u64>, frame: u64) -> bool {
    stamp
        .map(|f0| frame.wrapping_sub(f0) < COPY_FLASH_FRAMES)
        .unwrap_or(false)
}

/// Whether (col,row) lands inside an optional button rect.
fn hit(rect: Option<Rect>, col: u16, row: u16) -> bool {
    rect.map(|r| col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height)
        .unwrap_or(false)
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
