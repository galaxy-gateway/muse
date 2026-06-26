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
        };
        let mut model = Self {
            nodes: vec![root_node],
            root: 0,
            visible: Vec::new(),
        };
        model.scan(0, reg);
        model.rebuild_visible(None);
        model
    }

    /// Read a directory's immediate children (once). Dirs first, then files, A→Z.
    pub fn scan(&mut self, id: NodeId, reg: &Registry) {
        if self.nodes[id].children.is_some() || !self.nodes[id].is_dir {
            return;
        }
        let dir = self.nodes[id].path.clone();
        let depth = self.nodes[id].depth + 1;
        let mut entries: Vec<(PathBuf, bool)> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with('.') {
                        continue; // skip dotfiles
                    }
                }
                // keep dirs and supported media files only
                if is_dir || reg.is_supported(&p) {
                    entries.push((p, is_dir));
                }
            }
        }
        entries.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.file_name().cmp(&b.0.file_name()))
        });
        let mut kids = Vec::with_capacity(entries.len());
        for (path, is_dir) in entries {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let is_media = !is_dir && reg.is_supported(&path);
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
            });
            kids.push(nid);
        }
        self.nodes[id].children = Some(kids);
    }

    /// Recompute the flattened visible list. `filter` hides non-matching media files.
    pub fn rebuild_visible(&mut self, filter: Option<&str>) {
        let mut out = Vec::new();
        let kids = self.nodes[self.root].children.clone().unwrap_or_default();
        for k in kids {
            self.push_visible(k, filter, &mut out);
        }
        self.visible = out;
    }

    fn push_visible(&self, id: NodeId, filter: Option<&str>, out: &mut Vec<NodeId>) {
        let node = &self.nodes[id];
        if let Some(f) = filter {
            if !node.is_dir && !node.name.to_lowercase().contains(f) {
                return;
            }
        }
        out.push(id);
        if node.is_dir && node.expanded {
            if let Some(children) = &node.children {
                for &c in children {
                    self.push_visible(c, filter, out);
                }
            }
        }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }
}
