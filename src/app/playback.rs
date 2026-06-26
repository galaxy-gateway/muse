//! Playback control: starting a track, walking the current view list for
//! next/prev, and applying the loop mode at end-of-track.

use std::path::{Path, PathBuf};

use super::{App, LoopMode};
use crate::audio::TransportCmd;

/// Copy `path`'s absolute form to the system clipboard. Returns whether it
/// succeeded — a no-op-false when the clipboard is unavailable (headless / no
/// display server).
fn copy_path_to_clipboard(path: &Path) -> bool {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = abs.to_string_lossy().into_owned();
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .is_ok()
}

impl App {
    /// Start playback of `path` (if it's a supported media file).
    pub(super) fn play_path(&mut self, path: PathBuf) {
        if !self.registry.is_supported(&path) {
            return;
        }
        self.engine.send(TransportCmd::Open(path.clone()));
        self.begin_now_playing(path);
    }

    /// The decode thread spliced into the preloaded next track at a gapless
    /// boundary and announced it via `poll_advance`. Sync the UI to match
    /// WITHOUT re-issuing `Open` (which resets the transport and would
    /// reintroduce the very gap we just avoided).
    pub(super) fn on_auto_advanced(&mut self, path: PathBuf) {
        self.begin_now_playing(path);
    }

    /// Shared now-playing bookkeeping once the engine is pointed at `path`,
    /// whether by an explicit `Open` or a decode-thread auto-advance.
    fn begin_now_playing(&mut self, path: PathBuf) {
        self.spectrum.clear();
        self.now_playing = Some(path.clone());
        // The engine is (or is about to be) playing; arm end-of-track detection.
        self.prev_playing = true;
        self.push_media_metadata();
        // Ensure the now-playing waveform is computed even when playback was
        // started by auto-advance / next-prev (no tree selection change).
        if !self.wave_cache.contains_key(&path) && self.wave_pending.as_ref() != Some(&path) {
            self.request_waveform(path);
        }
        // Decode the track auto-advance will play next, so its boundary is gapless.
        self.preload_next();
    }

    /// Predict the next track (per loop mode + current view list) and ask the
    /// engine to decode it ahead of time. Best-effort: if the prediction is
    /// wrong (the user navigates away), the stale preload is simply discarded.
    pub(super) fn preload_next(&self) {
        // Gapless off: never prefetch, so the decode thread has nothing to
        // splice and falls back to the UI-driven (gapped) advance. This also
        // keeps the decoded-audio footprint to a single track.
        if !self.gapless {
            return;
        }
        let list = self.current_media();
        if list.is_empty() {
            return;
        }
        let cur = self
            .now_playing
            .as_ref()
            .and_then(|p| list.iter().position(|x| x == p));
        let next = match self.loop_mode {
            LoopMode::One => self.now_playing.clone(),
            LoopMode::All => cur.map(|i| list[(i + 1) % list.len()].clone()),
            LoopMode::Off => cur.and_then(|i| list.get(i + 1).cloned()),
        };
        if let Some(n) = next {
            if self.registry.is_supported(&n) {
                self.engine.preload(n);
            }
        }
    }

    /// Ordered media paths in the current view (filtered results, else the
    /// visible tree's media files) — the list `n`/`p` and auto-advance walk.
    fn current_media(&self) -> Vec<PathBuf> {
        if self.filter_active() {
            self.filtered.clone()
        } else {
            self.tree
                .visible
                .iter()
                .filter_map(|&id| {
                    let n = self.tree.node(id);
                    n.is_media.then(|| n.path.clone())
                })
                .collect()
        }
    }

    /// Copy the absolute path of the now-playing track to the system clipboard
    /// and flash the now-playing copy button's checkmark.
    pub(super) fn copy_now_playing_path(&mut self) {
        if let Some(p) = self.now_playing.clone()
            && copy_path_to_clipboard(&p)
        {
            self.copy_flash_np = Some(self.frame);
        }
    }

    /// Copy the absolute path of the cursor's selected file to the clipboard and
    /// flash the selection copy button's checkmark.
    pub(super) fn copy_selection_path(&mut self) {
        if let Some(p) = self.cursor_path()
            && copy_path_to_clipboard(&p)
        {
            self.copy_flash_sel = Some(self.frame);
        }
    }

    /// Play the track `delta` steps from the current one in the view list,
    /// wrapping around the ends. Used by `n` / `p`.
    pub(super) fn play_relative(&mut self, delta: i32) {
        let list = self.current_media();
        if list.is_empty() {
            return;
        }
        let cur = self
            .now_playing
            .as_ref()
            .and_then(|p| list.iter().position(|x| x == p));
        let len = list.len() as i32;
        let idx = match cur {
            Some(i) => (i as i32 + delta).rem_euclid(len),
            None => 0,
        };
        self.play_path(list[idx as usize].clone());
    }

    /// On the play->stop edge at the end of a track, apply the loop mode.
    pub(super) fn check_track_end(&mut self) {
        let playing = self.engine.is_playing();
        let dur = self.engine.duration_secs();
        let ended =
            self.prev_playing && !playing && dur > 0.0 && self.engine.position_secs() >= dur - 0.05;
        self.prev_playing = playing;
        if !ended {
            return;
        }
        match self.loop_mode {
            LoopMode::Off => {
                // advance, but stop at the end of the list
                let list = self.current_media();
                let next = self
                    .now_playing
                    .as_ref()
                    .and_then(|p| list.iter().position(|x| x == p))
                    .map(|i| i + 1)
                    .filter(|&i| i < list.len());
                if let Some(i) = next {
                    self.play_path(list[i].clone());
                }
            }
            LoopMode::All => self.play_relative(1),
            LoopMode::One => {
                if let Some(p) = self.now_playing.clone() {
                    self.play_path(p);
                }
            }
        }
    }
}
