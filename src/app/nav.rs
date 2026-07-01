//! Tree navigation: expand/collapse/enter, keeping the cursor stable across
//! visibility rebuilds, and lazy metadata + waveform loading on selection.

use std::path::PathBuf;
use std::thread;

use super::App;
use crate::event::AppEvent;
use crate::model::NodeId;

/// Number of static-waveform bins computed per track (resolution-independent).
const WAVE_BINS: usize = 1600;

impl App {
    /// Count all descendants of `id` (children, grandchildren, etc.).
    /// Used to decide whether to free a collapsed subtree.
    fn count_descendants(&self, id: NodeId) -> usize {
        let mut stack = vec![id];
        let mut count = 0;
        while let Some(node_id) = stack.pop() {
            if let Some(children) = &self.tree.node(node_id).children {
                for &child in children {
                    count += 1;
                    stack.push(child);
                }
            }
        }
        count
    }

    /// Expand a directory progressively: read its immediate children instantly
    /// (a single `read_dir`, no recursive stat walk), apply cached stats where
    /// fresh, and background-scan the rest. So expanding a huge folder shows its
    /// contents at once (unscanned dirs dimmed) instead of freezing the UI while
    /// it recurses the whole subtree.
    pub(super) fn expand_dir(&mut self, id: NodeId) {
        if self.tree.node(id).is_archive {
            // Archives list from an in-memory index — fast, no disk recursion.
            self.tree.scan(id, &self.registry);
            return;
        }
        self.tree.scan_shallow(id, &self.registry);
        let kids = self.tree.node(id).children.clone().unwrap_or_default();
        let mut misses = Vec::new();
        for k in kids {
            if !self.tree.node(k).pending {
                continue;
            }
            let p = self.tree.node(k).path.clone();
            if let Some((c, s)) = self.dir_cache.get_fresh(&p) {
                self.tree.apply_stats(&p, c, s); // instant from cache
            } else {
                misses.push(p);
            }
        }
        if !misses.is_empty() {
            self.scan_done = 0;
            self.scan_total = misses.len();
            crate::event::spawn_dir_stats(misses, self.tx.clone());
        }
    }

    pub(super) fn expand(&mut self) {
        // On a song — or in the flat fuzzy-result list, where there are no tree
        // nodes to expand — treat l / → as a downward move, matching j / Down.
        let Some(id) = self.cursor() else {
            self.move_cursor(1);
            return;
        };
        if !self.tree.node(id).is_dir {
            self.move_cursor(1);
            return;
        }
        self.expand_dir(id);
        self.tree.nodes[id].expanded = true;
        self.refilter_keep();
        // Descend: move the cursor onto the dir's first visible child, if any.
        if let Some(pos) = self.tree.visible.iter().position(|&v| v == id) {
            if let Some(&next) = self.tree.visible.get(pos + 1) {
                if self.tree.node(next).parent == Some(id) {
                    self.select(pos + 1);
                }
            }
        }
    }

    pub(super) fn collapse_or_parent(&mut self) {
        let Some(id) = self.cursor() else { return };
        let node = self.tree.node(id);
        if node.is_dir && node.expanded {
            self.close_archive_if_any(id);
            self.tree.nodes[id].expanded = false;
            // Release large subtrees to prevent unbounded memory growth.
            let desc_count = self.count_descendants(id);
            if desc_count > 2000 {
                self.tree.release_subtree(id);
            }
            self.refilter_keep();
        } else if let Some(parent) = node.parent {
            if parent != self.tree.root {
                if let Some(pos) = self.tree.visible.iter().position(|&v| v == parent) {
                    self.select(pos);
                }
            }
        }
    }

    pub(super) fn enter(&mut self) {
        // In fuzzy-filter mode the row is a flat result path: play it.
        if self.filter_active() {
            if let Some(path) = self.cursor_path() {
                self.play_path(path);
            }
            return;
        }
        let Some(id) = self.cursor() else { return };
        let node = self.tree.node(id);
        if node.is_dir {
            let expanded = node.expanded;
            if expanded {
                self.close_archive_if_any(id);
            } else {
                self.expand_dir(id);
            }
            self.tree.nodes[id].expanded = !expanded;
            // Release large subtrees when collapsing to prevent unbounded memory growth.
            if expanded {
                let desc_count = self.count_descendants(id);
                if desc_count > 2000 {
                    self.tree.release_subtree(id);
                }
            }
            self.refilter_keep();
        } else if node.is_media {
            let path = node.path.clone();
            self.play_path(path);
        } else if self.registry.is_playlist(&node.path) {
            let path = node.path.clone();
            self.load_m3u(path);
        }
    }

    /// If `id` is an archive node being collapsed, wipe its decompressed bytes
    /// from memory and release its built subtree so re-expanding re-lists fresh.
    fn close_archive_if_any(&mut self, id: NodeId) {
        if self.tree.node(id).is_archive {
            let path = self.tree.node(id).path.clone();
            crate::archive::close(&path);
            // Release the archive's subtree to the free list (not just drop it).
            self.tree.release_subtree(id);
        }
    }

    /// Rebuild visible but keep the current cursor node selected if still visible.
    fn refilter_keep(&mut self) {
        let keep = self.cursor();
        self.tree.rebuild_visible();
        if let Some(k) = keep {
            if let Some(pos) = self.tree.visible.iter().position(|&v| v == k) {
                self.list_state.select(Some(pos));
            }
        }
        self.on_selection_changed();
    }

    /// Lazy-load metadata + queue a debounced waveform compute for the selected media.
    pub(super) fn on_selection_changed(&mut self) {
        let Some(path) = self.cursor_path() else {
            return;
        };
        if !self.registry.is_supported(&path) {
            return;
        }
        self.ensure_meta(&path);
        // Queue art request with debounce (write the want, Tick will fire it after 150ms idle).
        if !self.wave_art.contains_key(&path) && self.art_pending.as_ref() != Some(&path) {
            self.art_want = Some((path.clone(), std::time::Instant::now()));
        }
        // Queue waveform request with debounce (write the want, Tick will fire it after 150ms idle).
        if !self.wave_cache.contains_key(&path) && self.wave_pending.as_ref() != Some(&path) {
            self.wave_want = Some((path, std::time::Instant::now()));
        }
    }

    /// Make sure `meta_cache` holds tags for `path`, sourcing them from the
    /// persistent disk cache (validated by size+mtime) when possible and only
    /// re-parsing the file on a cache miss. Touch LRU order for eviction.
    pub(super) fn ensure_meta(&mut self, path: &std::path::Path) {
        let path_buf = path.to_path_buf();
        if self.meta_cache.contains_key(&path_buf) || !self.registry.is_supported(path) {
            // Touch the LRU order even on cache hit.
            super::touch_lru(&mut self.meta_order, &path_buf);
            self.evict_meta();
            return;
        }
        let stamp = crate::metacache::file_stamp(path);
        if let Some((size, mtime)) = stamp
            && let Some(meta) = self.meta_disk.get(path, size, mtime)
        {
            self.meta_cache.insert(path_buf.clone(), meta.clone());
            super::touch_lru(&mut self.meta_order, &path_buf);
            self.evict_meta();
            return;
        }
        let Some(provider) = self.registry.for_path(path) else {
            return;
        };
        let meta = provider.metadata(path);
        if let Some((size, mtime)) = stamp {
            self.meta_disk.put(path, size, mtime, meta.clone());
        }
        self.meta_cache.insert(path_buf.clone(), meta);
        super::touch_lru(&mut self.meta_order, &path_buf);
        self.evict_meta();
    }

    pub(super) fn request_waveform(&mut self, path: PathBuf) {
        self.wave_gen += 1;
        let token = self.wave_gen;
        self.wave_pending = Some(path.clone());
        let tx = self.tx.clone();
        thread::spawn(move || {
            // Cheap packet-size envelope first (no decode, near-instant); fall
            // back to a full decode-scan only for CBR files where it's useless.
            let bins = crate::audio::waveform_envelope(&path, WAVE_BINS)
                .or_else(|| crate::audio::waveform_bins(&path, WAVE_BINS).ok());
            if let Some(bins) = bins {
                let _ = tx.send(AppEvent::Wave(path, token, bins));
            }
        });
    }

    /// Decode embedded cover art off-thread (bounded to 512px) and post it back.
    pub(super) fn request_art(&mut self, path: PathBuf) {
        if self.wave_art.contains_key(&path) || self.art_pending.as_ref() == Some(&path) {
            return;
        }
        self.art_pending = Some(path.clone());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let art = crate::media::cover_art_bytes(&path)
                .and_then(|bytes| image::load_from_memory(&bytes).ok())
                .map(|img| {
                    // Bound the cached image; it's downscaled again per render.
                    img.thumbnail(512, 512).to_rgb8()
                });
            let _ = tx.send(AppEvent::Art(path, art));
        });
    }

    fn evict_meta(&mut self) {
        const META_CAP: usize = 4096;
        let keep = self.keep_paths();
        super::evict_lru(&mut self.meta_cache, &mut self.meta_order, META_CAP, &keep);
    }
}
