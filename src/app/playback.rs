//! Playback control: starting a track, walking the current view list for
//! next/prev, and applying the loop mode at end-of-track.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{App, LoopMode};
use crate::audio::TransportCmd;

/// A press within this gap of the previous open request is a burst: defer the
/// engine Open instead of switching audio per press. OS key repeat is 30-90ms;
/// deliberate separate picks are >250ms. Matches `NAV_ACCEL_WINDOW`'s 200ms
/// held-repeat precedent in input.rs.
const OPEN_BURST_GAP: Duration = Duration::from_millis(200);
/// Trailing-edge delay: fire the deferred Open once presses stop for this long.
/// Must exceed the largest press interval classified as burst repeat (~90ms)
/// plus tick granularity (16ms), or it would fire mid-burst.
pub(super) const OPEN_TRAILING_MS: u128 = 150;

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
    /// Start playback of `path` (if it's a supported media file). Records the
    /// outgoing track into the shuffle/back history.
    pub(super) fn play_path(&mut self, path: PathBuf) {
        self.play_path_inner(path, true);
    }

    /// Like `play_path` but without pushing the outgoing track onto the back
    /// history — used by shuffle `previous` so repeated `p` walks backward.
    pub(super) fn play_path_no_history(&mut self, path: PathBuf) {
        self.play_path_inner(path, false);
    }

    fn play_path_inner(&mut self, path: PathBuf, record_history: bool) {
        if !self.registry.is_supported(&path) {
            return;
        }
        // Remember the outgoing track + playhead so `u` can return to it (e.g.
        // after an accidental click on a different song) and resume where it
        // left off. Only on a real switch, not a same-track restart.
        let outgoing = match &self.now_playing {
            Some(cur) if cur != &path => Some(cur.clone()),
            _ => None,
        };
        if let Some(cur) = outgoing {
            self.prev_track = Some((cur.clone(), self.engine.position_secs()));
            if record_history {
                self.push_history(cur);
            }
        }
        // Leading+trailing-edge debounce of the engine Open. A lone press (or
        // presses spaced > OPEN_BURST_GAP) switches audio instantly, exactly as
        // before. During a rapid burst (held `n`/`p`) the UI tracks each press
        // but the engine keeps playing the old track; the surviving pick opens
        // once presses stop (trailing edge, fired from the Tick arm). This is
        // what stops a held key from spawning one ~1MB decoder per press.
        let now = std::time::Instant::now();
        let in_burst = self.open_want.is_some()
            || self
                .last_open_req
                .is_some_and(|t| now.duration_since(t) < OPEN_BURST_GAP);
        self.last_open_req = Some(now);
        if in_burst {
            // Keep only the survivor; the trailing timer restarts per press.
            self.open_want = Some((path.clone(), now));
            self.note_now_playing(path);
        } else {
            self.engine.send(TransportCmd::Open(path.clone()));
            self.begin_now_playing(path);
        }
    }

    /// Send the deferred burst Open (trailing edge or a forced flush from a
    /// transport action that must act on the displayed track). No-op when
    /// nothing is pending. Arms the post-burst allocator relief sweep.
    pub(super) fn fire_deferred_open(&mut self) {
        if let Some((path, _)) = self.open_want.take() {
            self.engine.send(TransportCmd::Open(path.clone()));
            self.on_engine_open(&path);
            self.relief_want = Some(std::time::Instant::now());
        }
    }

    /// Return to the previously-playing track and resume at the playhead it had
    /// when we left it. `play_path` records the current track as the new
    /// previous, so `u` toggles back and forth between the two.
    pub(super) fn play_previous_track(&mut self) {
        if let Some((path, pos)) = self.prev_track.take() {
            self.play_path(path);
            // A resume-at-position play is a deliberate act: flush any deferred
            // burst Open so the SeekTo scrubs the track we just opened, not the
            // old stream.
            self.fire_deferred_open();
            self.engine.send(TransportCmd::SeekTo(pos));
        }
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
    pub(super) fn begin_now_playing(&mut self, path: PathBuf) {
        self.note_now_playing(path.clone());
        self.on_engine_open(&path);
    }

    /// Work that belongs to the moment the ENGINE actually opens the track —
    /// run once per confirmed switch, never per burst press.
    fn on_engine_open(&mut self, path: &Path) {
        // The old audio keeps playing while an Open is deferred, so its
        // spectrum stays valid until the real switch.
        self.spectrum.clear();
        // Ensure now-playing tags are loaded (cached) before pushing OS metadata,
        // since auto-advance may reach a track the cursor never selected. On a
        // cold cache this is a disk stat + tag parse; the OS metadata push is an
        // IPC call — both are why this must not run 25x/sec during a burst.
        self.ensure_meta(path);
        self.push_media_metadata();
    }

    /// Per-press UI bookkeeping — cheap in-memory state, safe to run at
    /// key-repeat rate while the engine Open is deferred.
    fn note_now_playing(&mut self, path: PathBuf) {
        self.now_playing = Some(path.clone());
        // The engine is (or will be, once the deferred Open fires) playing;
        // arm end-of-track detection.
        self.prev_playing = true;
        // Queue the cheap up-front waveform + cover art through the same
        // debounced single-worker slots browsing uses. A held `n`/`p` then does
        // no decode work per press — only the track that survives the debounce
        // computes; `sync_stream_waveform` shows the progressive decode fill as
        // a stopgap until the envelope lands.
        // A leftover partial stream snapshot for a *different* track must not
        // masquerade as its final waveform — drop it so browsing or replaying
        // that track recomputes from scratch.
        if self
            .wave_stream_stopgap
            .as_deref()
            .is_some_and(|o| o != path)
        {
            let old = self.wave_stream_stopgap.take().unwrap();
            self.wave_cache.remove(&old);
            self.wave_order.retain(|p| p != &old);
        }
        if (!self.wave_cache.contains_key(&path)
            || self.wave_stream_stopgap.as_ref() == Some(&path))
            && self.wave_pending.as_ref() != Some(&path)
        {
            self.wave_want = Some((path.clone(), std::time::Instant::now()));
        }
        if !self.wave_art.contains_key(&path) && self.art_pending.as_ref() != Some(&path) {
            self.art_want = Some((path, std::time::Instant::now()));
        }
        // Maintain the shuffle bag (drop the now-playing track, refill if dry).
        self.shuffle_after_play();
        // Decode the track auto-advance will play next, so its boundary is
        // gapless. Debounced: rapid switching would otherwise spawn (and cancel)
        // one prefetch decoder per keypress.
        self.queue_preload();
    }

    /// Ask for the gapless prefetch after now-playing has been stable for a
    /// moment (the Tick arm fires it). The prediction is computed at fire time,
    /// so intervening switches or mode toggles are folded into one request.
    pub(super) fn queue_preload(&mut self) {
        self.preload_want = Some(std::time::Instant::now());
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
        let next = if self.loop_mode == LoopMode::One {
            self.now_playing.clone()
        } else {
            self.predict_auto_next()
        };
        if let Some(n) = next
            && self.registry.is_supported(&n)
        {
            self.engine.preload(n);
        }
    }

    /// Ordered media paths in the current view (filtered results, else the
    /// visible tree's media files) — the list `n`/`p` and auto-advance walk.
    pub(super) fn current_media(&self) -> Vec<PathBuf> {
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
        // The old track ran out while a burst Open was deferred: the user
        // already picked the next track, so open it right now instead of
        // applying the loop mode (no silence, no fighting the pending pick).
        if self.open_want.is_some() {
            self.fire_deferred_open();
            return;
        }
        // LoopMode::One repeats in place; otherwise advance via the queue-aware
        // predictor (which falls back to the tree list, wrapping for All and
        // stopping at the end for Off).
        if self.loop_mode == LoopMode::One {
            if let Some(p) = self.now_playing.clone() {
                self.play_path(p);
            }
        } else if self.shuffle {
            self.advance_shuffle();
        } else if let Some(p) = self.predict_auto_next() {
            self.play_path(p);
        }
    }
}
