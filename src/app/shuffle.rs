//! Shuffle mode: a no-repeat "bag" of upcoming tracks drawn from the active
//! pool (the queue when playing from it, else the current tree/filter list).
//! Each cycle plays every track once before reshuffling. `play_history` lets
//! `previous` walk back through what actually played.

use std::path::PathBuf;

use super::App;

/// Cap on the back-history kept for shuffle's `previous`.
const HISTORY_CAP: usize = 256;

impl App {
    /// xorshift64 step — cheap, no dependency. Good enough for track shuffling.
    fn next_rng(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// The pool shuffle draws from: the queue when the now-playing track is in
    /// it, otherwise the current tree/filter media list.
    fn shuffle_pool(&self) -> Vec<PathBuf> {
        if !self.queue.is_empty()
            && let Some(np) = self.now_playing.as_ref()
            && self.queue.iter().any(|x| x == np)
        {
            self.queue.clone()
        } else {
            self.current_media()
        }
    }

    /// Rebuild the upcoming bag: the pool minus the now-playing track, shuffled
    /// (Fisher-Yates). Called when the bag drains or shuffle is switched on.
    pub(super) fn refill_bag(&mut self) {
        let np = self.now_playing.clone();
        let mut pool: Vec<PathBuf> = self
            .shuffle_pool()
            .into_iter()
            .filter(|p| Some(p) != np.as_ref())
            .collect();
        for i in (1..pool.len()).rev() {
            let j = (self.next_rng() % (i as u64 + 1)) as usize;
            pool.swap(i, j);
        }
        self.shuffle_bag = pool;
    }

    /// After a track starts playing: drop it from the upcoming bag, and refill
    /// the bag if it ran dry. No-op when shuffle is off.
    pub(super) fn shuffle_after_play(&mut self) {
        if !self.shuffle {
            return;
        }
        if let Some(np) = self.now_playing.clone() {
            self.shuffle_bag.retain(|p| p != &np);
        }
        if self.shuffle_bag.is_empty() {
            self.refill_bag();
        }
    }

    /// Record `path` as the most recent history entry (for shuffle `previous`).
    pub(super) fn push_history(&mut self, path: PathBuf) {
        self.play_history.push(path);
        if self.play_history.len() > HISTORY_CAP {
            let overflow = self.play_history.len() - HISTORY_CAP;
            self.play_history.drain(0..overflow);
        }
    }

    /// End-of-track / `n` advance while shuffling: play the next bag entry,
    /// reshuffling for `LoopMode::All` when the bag is empty (a cycle finished).
    /// `LoopMode::Off` stops at the end of the cycle.
    pub(super) fn advance_shuffle(&mut self) {
        if let Some(next) = self.shuffle_bag.first().cloned() {
            self.play_path(next);
        } else if self.loop_mode == super::LoopMode::All {
            self.refill_bag();
            if let Some(next) = self.shuffle_bag.first().cloned() {
                self.play_path(next);
            }
        }
    }

    /// `p` while shuffling: step back through real play history.
    pub(super) fn shuffle_prev(&mut self) {
        if let Some(prev) = self.play_history.pop() {
            // Don't re-record into history, so repeated `p` walks back instead of
            // ping-ponging between two tracks.
            self.play_path_no_history(prev);
        }
    }
}
