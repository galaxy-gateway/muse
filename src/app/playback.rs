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
        self.spectrum.clear();
        self.now_playing = Some(path.clone());
        // Open will flip the engine to playing; arm end-of-track detection.
        self.prev_playing = true;
        self.push_media_metadata();
        // Ensure the now-playing waveform is computed even when playback was
        // started by auto-advance / next-prev (no tree selection change).
        if !self.wave_cache.contains_key(&path) && self.wave_pending.as_ref() != Some(&path) {
            self.request_waveform(path);
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
