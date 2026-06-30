//! Persistent per-file metadata cache. Tag reads (lofty parse) are skipped on
//! later launches when a file's size and mtime are unchanged, so browsing a
//! previously-seen library is instant. Keyed by absolute path; validated by
//! (size, mtime) so edited/replaced files are re-read automatically.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::media::Meta;

#[derive(Serialize, Deserialize)]
struct Entry {
    size: u64,
    mtime: u64,
    meta: Meta,
}

#[derive(Default, Serialize, Deserialize)]
pub struct MetaCache {
    /// Keyed by the path's lossy string (JSON object keys must be strings).
    entries: HashMap<String, Entry>,
    /// Set when an entry is added/replaced, so we only rewrite the file if needed.
    #[serde(skip)]
    dirty: bool,
}

/// `(size, seconds-since-epoch mtime)` for `path`, or `None` if it can't be stat-ed.
pub fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((md.len(), mtime))
}

fn cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.cache_dir().join("meta-cache.json"))
}

impl MetaCache {
    pub fn load() -> Self {
        cache_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Cached metadata for `path` if present and still matching `(size, mtime)`.
    pub fn get(&self, path: &Path, size: u64, mtime: u64) -> Option<&Meta> {
        let e = self.entries.get(&path.to_string_lossy().into_owned())?;
        (e.size == size && e.mtime == mtime).then_some(&e.meta)
    }

    pub fn put(&mut self, path: &Path, size: u64, mtime: u64, meta: Meta) {
        self.entries.insert(
            path.to_string_lossy().into_owned(),
            Entry { size, mtime, meta },
        );
        self.dirty = true;
    }

    /// Write the cache to disk if it changed since load. Best-effort.
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
