//! Lazy file tree with a flattened-visible-list for O(visible) rendering.
//! Directories are scanned only when first expanded.

use std::path::{Path, PathBuf};

use crate::media::Registry;

pub type NodeId = usize;

pub struct Node {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_media: bool,
    pub depth: usize,
    pub parent: Option<NodeId>,
    pub children: Option<Vec<NodeId>>, // None = unscanned dir
    pub expanded: bool,
    /// For files: byte size. For dirs: summed size of all media files within (recursive).
    pub size: u64,
    /// For dirs: count of media files within (recursive). 0 for files.
    pub count: usize,
}

pub struct TreeModel {
    pub nodes: Vec<Node>,
    pub root: NodeId,
    pub visible: Vec<NodeId>,
}

impl TreeModel {
    pub fn new(root: &Path, reg: &Registry) -> Self {
        let name = root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned());
        let root_node = Node {
            path: root.to_path_buf(),
            name,
            is_dir: true,
            is_media: false,
            depth: 0,
            parent: None,
            children: None,
            expanded: true,
            size: 0,
            count: 0,
        };
        let mut model = Self {
            nodes: vec![root_node],
            root: 0,
            visible: Vec::new(),
        };
        model.scan(0, reg);
        model.rebuild_visible();
        model
    }

    /// Read a directory's immediate children (once). Dirs first, then files, A→Z.
    pub fn scan(&mut self, id: NodeId, reg: &Registry) {
        if self.nodes[id].children.is_some() || !self.nodes[id].is_dir {
            return;
        }
        let dir = self.nodes[id].path.clone();
        let depth = self.nodes[id].depth + 1;
        let mut entries: Vec<(PathBuf, bool, u64)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with('.') {
                        continue; // skip dotfiles
                    }
                }
                // keep dirs, supported media, and playlist (.m3u) files
                if is_dir || reg.is_visible(&p) {
                    let size = if is_dir {
                        0
                    } else {
                        e.metadata().map(|m| m.len()).unwrap_or(0)
                    };
                    entries.push((p, is_dir, size));
                }
            }
        }
        entries.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.file_name().cmp(&b.0.file_name()))
        });
        let mut kids = Vec::with_capacity(entries.len());
        for (path, is_dir, mut size) in entries {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let is_media = !is_dir && reg.is_supported(&path);
            // Recursive media stats: a dir with no music anywhere below is hidden.
            let count = if is_dir {
                let (c, s) = Self::dir_stats(&path, reg);
                if c == 0 {
                    continue; // prune music-less directories
                }
                size = s;
                c
            } else {
                0
            };
            let nid = self.nodes.len();
            self.nodes.push(Node {
                path,
                name,
                is_dir,
                is_media,
                depth,
                parent: Some(id),
                children: if is_dir { None } else { Some(Vec::new()) },
                expanded: false,
                size,
                count,
            });
            kids.push(nid);
        }
        self.nodes[id].children = Some(kids);
    }

    /// Count all supported media files within `dir` (recursive) and sum their
    /// sizes. Used both to display per-dir totals and to prune dirs holding no
    /// music anywhere below them.
    fn dir_stats(dir: &Path, reg: &Registry) -> (usize, u64) {
        let mut count = 0;
        let mut size = 0;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with('.') {
                        continue;
                    }
                }
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir {
                    let (c, s) = Self::dir_stats(&p, reg);
                    count += c;
                    size += s;
                } else if reg.is_supported(&p) {
                    count += 1;
                    size += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
        (count, size)
    }

    /// Recompute the flattened visible list from the tree's expand state.
    /// (Fuzzy filtering is a separate flat-results view handled in `App`.)
    pub fn rebuild_visible(&mut self) {
        let mut out = Vec::new();
        let kids = self.nodes[self.root].children.clone().unwrap_or_default();
        for k in kids {
            self.push_visible(k, &mut out);
        }
        self.visible = out;
    }

    fn push_visible(&self, id: NodeId, out: &mut Vec<NodeId>) {
        out.push(id);
        let node = &self.nodes[id];
        if node.is_dir && node.expanded {
            if let Some(children) = &node.children {
                for &c in children {
                    self.push_visible(c, out);
                }
            }
        }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }
}
