//! Command palette (`:` / Ctrl+P) and right-click context menus. Both are thin
//! front-ends over the same `Act` table: an `Act::Key` replays a normal-mode
//! key, so every command reuses the existing key handler instead of
//! duplicating it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use super::mouse::hit_rect;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// Replay a normal-mode key.
    Key(KeyCode),
    ResetLayout,
}

const fn k(c: char) -> Act {
    Act::Key(KeyCode::Char(c))
}

/// Palette commands: (title, key hint, action).
pub const CMDS: &[(&str, &str, Act)] = &[
    ("Play / Pause", "space", k(' ')),
    ("Next Track", "n", k('n')),
    ("Previous Track", "p", k('p')),
    ("Previously Played Track", "u", k('u')),
    ("Seek Forward 5s", ".", k('.')),
    ("Seek Back 5s", ",", k(',')),
    ("Volume Up", "+", k('+')),
    ("Volume Down", "-", k('-')),
    ("Add to Queue", "a", k('a')),
    ("Play Next (Queue)", "A", k('A')),
    ("Show Queue", "Q", k('Q')),
    ("Save Queue as M3U", "w", k('w')),
    ("Jump to Now Playing", "c", k('c')),
    ("Cycle Loop Mode", "r", k('r')),
    ("Toggle Gapless", "b", k('b')),
    ("Toggle Shuffle", "s", k('s')),
    ("Copy Selected Path", "y", k('y')),
    ("Copy Now Playing Path", "Y", k('Y')),
    ("Filter Library", "/", k('/')),
    ("Cycle Waveform View", "W", k('W')),
    ("Next Scope Preset", "v", k('v')),
    ("Previous Scope Preset", "V", k('V')),
    ("Pick Theme", "t", k('t')),
    ("Toggle Cover Art", "i", k('i')),
    ("Toggle Motion", "m", k('m')),
    ("Reset Pane Layout", "", Act::ResetLayout),
    ("Help", "?", k('?')),
    ("Quit", "q", k('q')),
];

pub struct Palette {
    pub query: String,
    pub sel: usize,
}

pub struct Menu {
    pub items: Vec<(&'static str, Act)>,
    pub sel: usize,
    pub rect: Rect,
}

impl App {
    pub(super) fn open_palette(&mut self) {
        self.menu = None;
        self.palette = Some(Palette {
            query: String::new(),
            sel: 0,
        });
    }

    /// Indices into `CMDS` matching the query, best match first.
    pub fn palette_hits(&self) -> Vec<usize> {
        let Some(p) = &self.palette else {
            return Vec::new();
        };
        let q = p.query.to_lowercase();
        let mut hits: Vec<(i32, usize)> = CMDS
            .iter()
            .enumerate()
            .filter_map(|(i, c)| crate::util::fuzzy_score(&q, &c.0.to_lowercase()).map(|s| (s, i)))
            .collect();
        hits.sort();
        hits.into_iter().map(|(_, i)| i).collect()
    }

    pub(super) fn run_act(&mut self, act: Act) {
        match act {
            Act::ResetLayout => {
                self.tree_frac = super::panes::TREE_FRAC_DEFAULT;
                self.scope_rows = super::panes::SCOPE_ROWS_DEFAULT;
                self.persist();
            }
            Act::Key(code) => {
                // Menu/palette actions apply to the library, not a half-typed filter.
                self.filtering = false;
                self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            }
        }
    }

    pub(super) fn palette_key(&mut self, key: KeyEvent) {
        let hits = self.palette_hits();
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.palette = None,
            KeyCode::Enter => {
                let act = hits.get(p.sel).map(|&i| CMDS[i].2);
                self.palette = None;
                if let Some(a) = act {
                    self.run_act(a);
                }
            }
            KeyCode::Down => p.sel = (p.sel + 1).min(hits.len().saturating_sub(1)),
            KeyCode::Char('n') if ctrl => p.sel = (p.sel + 1).min(hits.len().saturating_sub(1)),
            KeyCode::Up => p.sel = p.sel.saturating_sub(1),
            KeyCode::Char('p') if ctrl => p.sel = p.sel.saturating_sub(1),
            KeyCode::Backspace => {
                p.query.pop();
                p.sel = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                p.query.push(c);
                p.sel = 0;
            }
            _ => {}
        }
    }

    /// Right click: select what is under the pointer, then open its menu.
    pub(super) fn open_menu(&mut self, x: u16, y: u16) {
        self.palette = None;
        let mut items: Vec<(&'static str, Act)> = Vec::new();
        if let Some(idx) = self.tree_row_at(x, y) {
            self.list_state.select(Some(idx));
            self.on_selection_changed();
            items.push(("Play / Open", Act::Key(KeyCode::Enter)));
            items.push(("Add to Queue", k('a')));
            items.push(("Play Next", k('A')));
            items.push(("Copy Path", k('y')));
        } else if hit_rect(self.transport_rect, x, y)
            || hit_rect(self.np_rect, x, y)
            || hit_rect(self.wave_rect, x, y)
        {
            items.push(("Play / Pause", k(' ')));
            items.push(("Next Track", k('n')));
            items.push(("Previous Track", k('p')));
            items.push(("Toggle Shuffle", k('s')));
            items.push(("Cycle Loop Mode", k('r')));
            items.push(("Copy Now Playing Path", k('Y')));
        } else if hit_rect(self.scope_rect, x, y) {
            items.push(("Next Scope Preset", k('v')));
            items.push(("Previous Scope Preset", k('V')));
        } else {
            items.push(("Pick Theme", k('t')));
            items.push(("Toggle Motion", k('m')));
        }
        items.push(("Reset Pane Layout", Act::ResetLayout));
        items.push(("Command Palette", k(':')));
        let w = items.iter().map(|i| i.0.chars().count()).max().unwrap_or(0) as u16 + 4;
        let h = items.len() as u16 + 2;
        let s = self.screen;
        let rect = Rect {
            x: x.min((s.x + s.width).saturating_sub(w)),
            y: y.min((s.y + s.height).saturating_sub(h)),
            width: w.min(s.width),
            height: h.min(s.height),
        };
        self.menu = Some(Menu {
            items,
            sel: 0,
            rect,
        });
    }

    pub(super) fn menu_key(&mut self, key: KeyEvent) {
        let Some(m) = self.menu.as_mut() else { return };
        let last = m.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => m.sel = (m.sel + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => m.sel = m.sel.saturating_sub(1),
            KeyCode::Enter => self.menu_run(),
            _ => self.menu = None,
        }
    }

    fn menu_run(&mut self) {
        if let Some(m) = self.menu.take() {
            self.run_act(m.items[m.sel].1);
        }
    }

    /// Mouse while a menu is open: hover selects, a left click on an item runs
    /// it, any other click dismisses.
    pub(super) fn menu_mouse(&mut self, m: MouseEvent) {
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        let r = menu.rect;
        let item = (hit_rect(r, m.column, m.row) && m.row > r.y && m.row < r.y + r.height - 1)
            .then(|| (m.row - r.y - 1) as usize);
        match m.kind {
            MouseEventKind::Moved => {
                if let Some(i) = item {
                    menu.sel = i;
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match item {
                Some(i) => {
                    menu.sel = i;
                    self.menu_run();
                }
                None => self.menu = None,
            },
            MouseEventKind::Down(_) => self.menu = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmds_are_unique_and_keyed() {
        for (i, a) in CMDS.iter().enumerate() {
            for b in &CMDS[i + 1..] {
                assert_ne!(a.0, b.0);
                assert!(a.2 != b.2 || a.2 == Act::ResetLayout);
            }
        }
    }

    #[test]
    fn fuzzy_finds_commands() {
        let hit = |q: &str| {
            CMDS.iter()
                .any(|c| crate::util::fuzzy_score(q, &c.0.to_lowercase()).is_some())
        };
        assert!(hit("shuf") && hit("rstlay") && !hit("zzzz"));
    }
}
