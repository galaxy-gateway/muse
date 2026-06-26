//! Keyboard handling: normal-mode keys, the fuzzy-filter line editor, list
//! cursor movement, scope cycling, and the theme picker.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::App;
use crate::audio::TransportCmd;
use crate::config::{SCOPE_PRESETS, THEMES};

/// Max gap between same-direction nav presses still counted as a held repeat.
/// OS key-repeat fires well under this; deliberate taps fall outside it.
const NAV_ACCEL_WINDOW: Duration = Duration::from_millis(200);

impl App {
    pub(super) fn on_key(&mut self, key: KeyEvent) {
        if self.filtering {
            self.filter_key(key);
            return;
        }
        if self.show_help {
            // Modal: only q / esc dismiss; everything else is swallowed.
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }
        if self.show_theme {
            self.theme_key(key);
            return;
        }
        // Esc in normal mode clears an applied filter, restoring the tree view.
        if key.code == KeyCode::Esc && self.filter_active() {
            self.clear_filter();
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Char('?'), _) => self.show_help = true,
            (KeyCode::Char('/'), _) => {
                self.filtering = true;
                self.list_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.move_cursor_accel(1),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.move_cursor_accel(-1),
            (KeyCode::Char('g'), _) | (KeyCode::Home, _) => self.select(0),
            (KeyCode::Char('G'), _) | (KeyCode::End, _) => {
                self.select(self.list_len().saturating_sub(1))
            }
            // Shift+arrows scrub the playhead (fine seek); plain arrows nav tree.
            (KeyCode::Left, KeyModifiers::SHIFT) => self.seek_rel(-1.0),
            (KeyCode::Right, KeyModifiers::SHIFT) => self.seek_rel(1.0),
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => self.collapse_or_parent(),
            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => self.expand(),
            (KeyCode::Enter, _) => self.enter(),
            (KeyCode::Char(' '), _) => self.engine.send(TransportCmd::Toggle),
            (KeyCode::Char('n'), _) => self.play_relative(1),
            (KeyCode::Char('p'), _) => self.play_relative(-1),
            (KeyCode::Char('r'), _) => {
                self.loop_mode = self.loop_mode.next();
                // The predicted-next track depends on the loop mode.
                self.preload_next();
            }
            (KeyCode::Char('b'), _) => {
                // Toggle gapless (back-to-back) playback; (un)prime the prefetch.
                self.gapless = !self.gapless;
                self.preload_next();
                self.persist();
            }
            (KeyCode::Char('y'), _) => self.copy_selection_path(),
            (KeyCode::Char('Y'), _) => self.copy_now_playing_path(),
            (KeyCode::Char('.'), _) => self.seek_rel(5.0),
            (KeyCode::Char(','), _) => self.seek_rel(-5.0),
            (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => {
                self.engine.send(TransportCmd::VolRel(0.05))
            }
            (KeyCode::Char('-'), _) => self.engine.send(TransportCmd::VolRel(-0.05)),
            (KeyCode::Char('v'), _) => self.cycle_scope(1),
            (KeyCode::Char('V'), _) => self.cycle_scope(-1),
            (KeyCode::Char('t'), _) => self.open_theme_picker(),
            _ => {}
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            // Esc cancels: clear the query and return to the tree + normal keys.
            KeyCode::Esc => self.clear_filter(),
            // Enter accepts the filter; stop typing but keep results for nav/play.
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_filtered();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.rebuild_filtered();
            }
            _ => {}
        }
    }

    /// Exit filtering entirely: drop the query/results and restore the tree
    /// view with the cursor on the first row.
    fn clear_filter(&mut self) {
        self.filtering = false;
        self.filter.clear();
        self.filtered.clear();
        self.list_state
            .select((!self.tree.visible.is_empty()).then_some(0));
        self.on_selection_changed();
    }

    /// Recompute fuzzy results from the background index against the query.
    pub(super) fn rebuild_filtered(&mut self) {
        if self.filter.is_empty() {
            self.filtered.clear();
            self.list_state
                .select((!self.tree.visible.is_empty()).then_some(0));
            self.on_selection_changed();
            return;
        }
        let q = self.filter.to_lowercase();
        let root = self.root_path();
        let mut scored: Vec<(i32, &PathBuf)> = self
            .index
            .iter()
            .filter_map(|p| {
                let hay = p
                    .strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_lowercase();
                crate::util::fuzzy_score(&q, &hay).map(|s| (s, p))
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        self.filtered = scored.into_iter().map(|(_, p)| p.clone()).collect();
        self.list_state
            .select((!self.filtered.is_empty()).then_some(0));
        self.on_selection_changed();
    }

    /// Accelerated cursor move for held j/k/↑/↓: consecutive same-direction
    /// presses inside `NAV_ACCEL_WINDOW` (i.e. OS key-repeat) ramp the step so a
    /// long list scrolls quickly, while deliberate taps stay single-row.
    pub(super) fn move_cursor_accel(&mut self, dir: i32) {
        let now = Instant::now();
        let held = matches!(
            self.nav_last,
            Some((d, t)) if d == dir && now.duration_since(t) <= NAV_ACCEL_WINDOW
        );
        self.nav_streak = if held { self.nav_streak + 1 } else { 0 };
        self.nav_last = Some((dir, now));
        let step = match self.nav_streak {
            0..=3 => 1,
            4..=7 => 2,
            8..=12 => 4,
            _ => 8,
        };
        self.move_cursor(dir * step);
    }

    pub(super) fn move_cursor(&mut self, delta: i32) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let max = len as i32 - 1;
        self.select((cur + delta).clamp(0, max) as usize);
    }

    pub(super) fn select(&mut self, i: usize) {
        let prev = self.list_state.selected();
        self.list_state.select(Some(i));
        self.on_selection_changed();
        // Particles shoot opposite the move: moving down fires up, up fires down.
        let dir = match prev {
            Some(p) if i > p => -1.0, // moved down -> up
            Some(p) if i < p => 1.0,  // moved up   -> down
            _ => -1.0,
        };
        let ctx = self.frame_ctx();
        self.theme.effect.on_nav(&mut self.sim, &ctx, dir);
    }

    fn cycle_scope(&mut self, delta: i32) {
        let n = SCOPE_PRESETS.len() as i32;
        self.scope_idx = (((self.scope_idx as i32 + delta) % n + n) % n) as usize;
        self.persist();
    }

    fn open_theme_picker(&mut self) {
        self.show_theme = true;
        self.theme_prev = self.theme_idx;
        self.theme_sel = self.theme_idx;
    }

    /// Move the highlight (wrapping) and live-preview that theme.
    fn theme_move(&mut self, delta: i32) {
        let n = THEMES.len() as i32;
        self.theme_sel = (((self.theme_sel as i32 + delta) % n + n) % n) as usize;
        self.apply_preview();
    }

    fn theme_jump(&mut self, i: usize) {
        self.theme_sel = i.min(THEMES.len() - 1);
        self.apply_preview();
    }

    fn apply_preview(&mut self) {
        self.theme_idx = self.theme_sel;
        self.theme = THEMES[self.theme_idx];
    }

    fn theme_confirm(&mut self) {
        self.apply_preview();
        self.show_theme = false;
        self.persist();
    }

    fn theme_cancel(&mut self) {
        self.theme_idx = self.theme_prev;
        self.theme = THEMES[self.theme_idx];
        self.show_theme = false;
    }

    fn theme_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.theme_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.theme_move(-1),
            KeyCode::Char('g') | KeyCode::Home => self.theme_jump(0),
            KeyCode::Char('G') | KeyCode::End => self.theme_jump(THEMES.len() - 1),
            KeyCode::Enter => self.theme_confirm(),
            KeyCode::Esc | KeyCode::Char('q') => self.theme_cancel(),
            _ => {}
        }
    }
}
