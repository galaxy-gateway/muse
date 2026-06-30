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
        self.tree.scan(id, &self.registry);
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
                self.tree.scan(id, &self.registry);
            }
            self.tree.nodes[id].expanded = !expanded;
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
    /// from memory and drop its built subtree so re-expanding re-lists fresh.
    fn close_archive_if_any(&mut self, id: NodeId) {
        if self.tree.node(id).is_archive {
            let path = self.tree.node(id).path.clone();
            crate::archive::close(&path);
            self.tree.nodes[id].children = None;
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

    /// Lazy-load metadata + kick off a waveform compute for the selected media.
    pub(super) fn on_selection_changed(&mut self) {
        let Some(path) = self.cursor_path() else {
            return;
        };
        if !self.registry.is_supported(&path) {
            return;
        }
        self.ensure_meta(&path);
        if !self.wave_cache.contains_key(&path) && self.wave_pending.as_ref() != Some(&path) {
            self.request_waveform(path);
        }
    }

    /// Make sure `meta_cache` holds tags for `path`, sourcing them from the
    /// persistent disk cache (validated by size+mtime) when possible and only
    /// re-parsing the file on a cache miss.
    pub(super) fn ensure_meta(&mut self, path: &std::path::Path) {
        if self.meta_cache.contains_key(path) || !self.registry.is_supported(path) {
            return;
        }
        let stamp = crate::metacache::file_stamp(path);
        if let Some((size, mtime)) = stamp
            && let Some(meta) = self.meta_disk.get(path, size, mtime)
        {
            self.meta_cache.insert(path.to_path_buf(), meta.clone());
            return;
        }
        let Some(provider) = self.registry.for_path(path) else {
            return;
        };
        let meta = provider.metadata(path);
        if let Some((size, mtime)) = stamp {
            self.meta_disk.put(path, size, mtime, meta.clone());
        }
        self.meta_cache.insert(path.to_path_buf(), meta);
    }

    pub(super) fn request_waveform(&mut self, path: PathBuf) {
        self.wave_gen += 1;
        let token = self.wave_gen;
        self.wave_pending = Some(path.clone());
        let tx = self.tx.clone();
        thread::spawn(move || {
            if let Ok(bins) = crate::audio::waveform_bins(&path, WAVE_BINS) {
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
}
