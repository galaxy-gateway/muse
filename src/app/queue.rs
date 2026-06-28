//! Explicit play queue: build it from the tree selection, drive next/prev and
//! the gapless prefetch off it, and import/export `.m3u` playlists. The queue
//! takes priority over the tree list whenever the now-playing track is in it.

use std::path::{Path, PathBuf};

use super::{App, LoopMode};

impl App {
    /// Collect playable media under `path`: the file itself if supported, or
    /// every supported file beneath it (sorted) if it is a directory.
    fn collect_media(&self, path: &Path) -> Vec<PathBuf> {
        if path.is_file() {
            return if self.registry.is_supported(path) {
                vec![path.to_path_buf()]
            } else {
                Vec::new()
            };
        }
        let mut out = Vec::new();
        let walker = ignore::WalkBuilder::new(path)
            .standard_filters(false)
            .hidden(true)
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file() && self.registry.is_supported(p) {
                out.push(p.to_path_buf());
            }
        }
        out.sort();
        out
    }

    /// `a`: append the cursor selection (file or recursive dir) to the queue end.
    pub(super) fn queue_append(&mut self) {
        if let Some(p) = self.cursor_path() {
            let items = self.collect_media(&p);
            self.queue.extend(items);
        }
    }

    /// `A`: insert the cursor selection right after the now-playing track (or at
    /// the front when nothing in the queue is playing) — i.e. "play next".
    pub(super) fn queue_play_next(&mut self) {
        let Some(p) = self.cursor_path() else { return };
        let items = self.collect_media(&p);
        if items.is_empty() {
            return;
        }
        let at = self
            .now_playing
            .as_ref()
            .and_then(|np| self.queue.iter().position(|x| x == np))
            .map(|i| i + 1)
            .unwrap_or(0);
        for (k, it) in items.into_iter().enumerate() {
            let idx = (at + k).min(self.queue.len());
            self.queue.insert(idx, it);
        }
    }

    /// The track to auto-advance to (end-of-track and gapless prefetch). When the
    /// now-playing track is in the queue, walk the queue; otherwise fall back to
    /// the tree list honoring the loop mode (All wraps, Off stops). `LoopMode::One`
    /// is handled by the callers.
    pub(super) fn predict_auto_next(&self) -> Option<PathBuf> {
        if let Some(np) = self.now_playing.as_ref()
            && let Some(i) = self.queue.iter().position(|x| x == np)
        {
            if let Some(n) = self.queue.get(i + 1) {
                return Some(n.clone());
            }
            return match self.loop_mode {
                LoopMode::All => self.queue.first().cloned(),
                _ => None,
            };
        }
        let list = self.current_media();
        if list.is_empty() {
            return None;
        }
        let cur = self
            .now_playing
            .as_ref()
            .and_then(|p| list.iter().position(|x| x == p));
        match self.loop_mode {
            LoopMode::All => cur.map(|i| list[(i + 1) % list.len()].clone()),
            LoopMode::Off => cur.and_then(|i| list.get(i + 1).cloned()),
            LoopMode::One => self.now_playing.clone(),
        }
    }

    /// `n`: next track. Queue-first (starting the queue if nothing in it is
    /// playing yet, wrapping at the end); otherwise the tree list.
    pub(super) fn play_next(&mut self) {
        if !self.queue.is_empty() {
            let pos = self
                .now_playing
                .as_ref()
                .and_then(|p| self.queue.iter().position(|x| x == p));
            let next = match pos {
                Some(i) => self
                    .queue
                    .get(i + 1)
                    .cloned()
                    .or_else(|| self.queue.first().cloned()),
                None => self.queue.first().cloned(),
            };
            if let Some(n) = next {
                self.play_path(n);
                return;
            }
        }
        self.play_relative(1);
    }

    /// `p`: previous track. Walks the queue when playing from it (wrapping),
    /// else the tree list.
    pub(super) fn play_prev(&mut self) {
        if !self.queue.is_empty()
            && let Some(i) = self
                .now_playing
                .as_ref()
                .and_then(|p| self.queue.iter().position(|x| x == p))
        {
            let prev = if i > 0 {
                self.queue.get(i - 1).cloned()
            } else {
                self.queue.last().cloned()
            };
            if let Some(p) = prev {
                self.play_path(p);
                return;
            }
        }
        self.play_relative(-1);
    }

    // --- queue-manager modal ops ------------------------------------------

    pub(super) fn toggle_queue(&mut self) {
        self.show_queue = !self.show_queue;
        if self.show_queue {
            self.queue_sel = self.queue_sel.min(self.queue.len().saturating_sub(1));
        }
    }

    pub(super) fn queue_move_cursor(&mut self, delta: i32) {
        if self.queue.is_empty() {
            return;
        }
        let max = self.queue.len() as i32 - 1;
        self.queue_sel = (self.queue_sel as i32 + delta).clamp(0, max) as usize;
    }

    /// Remove the highlighted queue entry.
    pub(super) fn queue_remove_sel(&mut self) {
        if self.queue_sel < self.queue.len() {
            self.queue.remove(self.queue_sel);
            let max = self.queue.len().saturating_sub(1);
            self.queue_sel = self.queue_sel.min(max);
        }
    }

    pub(super) fn queue_clear(&mut self) {
        self.queue.clear();
        self.queue_sel = 0;
    }

    /// Reorder: swap the highlighted entry with its neighbor, following it.
    pub(super) fn queue_reorder(&mut self, delta: i32) {
        let target = self.queue_sel as i32 + delta;
        if target < 0 || target as usize >= self.queue.len() {
            return;
        }
        let target = target as usize;
        self.queue.swap(self.queue_sel, target);
        self.queue_sel = target;
    }

    /// Play the highlighted queue entry now.
    pub(super) fn queue_play_sel(&mut self) {
        if let Some(p) = self.queue.get(self.queue_sel).cloned() {
            self.play_path(p);
        }
    }

    // --- m3u import / export ----------------------------------------------

    /// `w`: write the queue to `<root>/muse-queue.m3u` (absolute paths).
    pub(super) fn write_queue_m3u(&self) {
        if self.queue.is_empty() {
            return;
        }
        let path = self.root_path().join("muse-queue.m3u");
        let mut s = String::from("#EXTM3U\n");
        for p in &self.queue {
            s.push_str(&p.to_string_lossy());
            s.push('\n');
        }
        let _ = std::fs::write(path, s);
    }

    /// Load an `.m3u`/`.m3u8` into the queue and start playing its first track.
    /// Relative entries resolve against the playlist's directory.
    pub(super) fn load_m3u(&mut self, path: PathBuf) {
        let Ok(txt) = std::fs::read_to_string(&path) else {
            return;
        };
        let base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut items = Vec::new();
        for line in txt.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let p = PathBuf::from(line);
            let p = if p.is_absolute() { p } else { base.join(p) };
            if p.exists() && self.registry.is_supported(&p) {
                items.push(p);
            }
        }
        if items.is_empty() {
            return;
        }
        self.queue = items;
        self.queue_sel = 0;
        let first = self.queue[0].clone();
        self.play_path(first);
    }
}
