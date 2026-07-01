//! Persistent per-directory recursive-stats cache. The startup scan computes
//! each directory's recursive `(count, size)` of media — expensive for big
//! libraries. This caches that result keyed by path and validated by the
//! directory's mtime, so a previously-scanned tree loads instantly on relaunch.
//!
//! Note: a dir's mtime only bumps when its *immediate* children change, not when
//! a file changes deep in a subtree — so a stale entry is possible after a
//! deep-nested edit. Acceptable: the entry refreshes the next time that dir's own
//! contents change, and it is rewritten whenever a live scan recomputes it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy)]
struct Entry {
    count: usize,
    size: u64,
    mtime: u64,
}

#[derive(Default, Serialize, Deserialize)]
pub struct DirCache {
    /// Keyed by the dir's lossy path string (JSON object keys must be strings).
    entries: HashMap<String, Entry>,
    #[serde(skip)]
    dirty: bool,
}

/// Seconds-since-epoch mtime for a directory, or `None` if it can't be stat-ed.
fn dir_mtime(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.cache_dir().join("dir-stats.json"))
}

impl DirCache {
    pub fn load() -> Self {
        cache_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Cached recursive `(count, size)` for `path` if present and the dir's mtime
    /// still matches — i.e. its immediate contents are unchanged since we scanned.
    pub fn get_fresh(&self, path: &Path) -> Option<(usize, u64)> {
        let e = self.entries.get(&path.to_string_lossy().into_owned())?;
        let m = dir_mtime(path)?;
        (m == e.mtime).then_some((e.count, e.size))
    }

    /// Record a freshly-computed stat, stamped with the dir's current mtime.
    pub fn insert(&mut self, path: &Path, count: usize, size: u64) {
        if let Some(mtime) = dir_mtime(path) {
            self.entries.insert(
                path.to_string_lossy().into_owned(),
                Entry { count, size, mtime },
            );
            self.dirty = true;
        }
    }

    /// Write to disk if changed since load. Best-effort.
    pub fn save(&self) {
        if !self.dirty {
            return;
        }
        let Some(path) = cache_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = std::fs::write(path, bytes);
        }
    }
}
