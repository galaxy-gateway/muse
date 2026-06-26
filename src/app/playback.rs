//! Playback control: starting a track, walking the current view list for
//! next/prev, and applying the loop mode at end-of-track.

use std::path::PathBuf;

use super::{App, LoopMode};
use crate::audio::TransportCmd;

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
