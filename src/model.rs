//! Lazy file tree with a flattened-visible-list for O(visible) rendering.
//! Directories are scanned only when first expanded.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::media::Registry;

pub type NodeId = usize;

pub struct Node {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    /// An archive container (zip/tar/7z/...). Expandable like a dir, but scanned
    /// from its in-memory listing and `close`d (memory wiped) when collapsed.
    pub is_archive: bool,
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
            is_archive: false,
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
    /// An archive container is scanned from its in-memory listing instead.
    pub fn scan(&mut self, id: NodeId, reg: &Registry) {
        if self.nodes[id].children.is_some() || !self.nodes[id].is_dir {
            return;
        }
        if self.nodes[id].is_archive {
            self.scan_archive(id);
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
                // keep dirs, supported media, playlist (.m3u) files, and archives
                if is_dir || reg.is_visible(&p) || crate::archive::is_archive(&p) {
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
            let is_arch = !is_dir && crate::archive::is_archive(&path);
            let is_media = !is_dir && !is_arch && reg.is_supported(&path);
            // Recursive media stats: a dir (or archive) with no music inside is hidden.
            let count = if is_dir {
                let (c, s) = Self::dir_stats(&path, reg);
                if c == 0 {
                    continue; // prune music-less directories
                }
                size = s;
                c
            } else if is_arch {
                let entries = crate::archive::list_audio(&path);
                if entries.is_empty() {
                    continue; // archive holds no audio → hide it
                }
                size = entries.iter().map(|e| e.size).sum();
                entries.len()
            } else {
                0
            };
            // Archives expand like dirs (is_dir), scanned lazily from memory.
            let expandable = is_dir || is_arch;
            let nid = self.nodes.len();
            self.nodes.push(Node {
                path,
                name,
                is_dir: expandable,
                is_archive: is_arch,
                is_media,
                depth,
                parent: Some(id),
                children: if expandable { None } else { Some(Vec::new()) },
                expanded: false,
                size,
                count,
            });
            kids.push(nid);
        }
        self.nodes[id].children = Some(kids);
    }

    /// Build an archive node's inner subtree from its in-memory listing. Entry
    /// paths are split into virtual directories + media leaves; intermediate
    /// dirs are created once and pre-expanded so diving in touches no disk.
    fn scan_archive(&mut self, id: NodeId) {
        let archive = self.nodes[id].path.clone();
        let base_depth = self.nodes[id].depth;
        let entries = crate::archive::list_audio(&archive);

        // Map inner-dir prefix -> node id (dedup), and parent -> ordered children.
        let mut dir_ids: HashMap<String, NodeId> = HashMap::new();
        let mut kids_of: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for e in entries.iter() {
            let comps: Vec<&str> = e.inner.split('/').filter(|c| !c.is_empty()).collect();
            if comps.is_empty() {
                continue;
            }
            let mut parent = id;
            let mut prefix = String::new();
            for (i, comp) in comps.iter().enumerate() {
                let depth = base_depth + i + 1;
                if i + 1 == comps.len() {
                    // media leaf — virtual path = archive joined with the entry
                    let nid = self.nodes.len();
                    self.nodes.push(Node {
                        path: archive.join(&e.inner),
                        name: (*comp).to_string(),
                        is_dir: false,
                        is_archive: false,
                        is_media: true,
                        depth,
                        parent: Some(parent),
                        children: Some(Vec::new()),
                        expanded: false,
                        size: e.size,
                        count: 0,
                    });
                    kids_of.entry(parent).or_default().push(nid);
                } else {
                    prefix = if prefix.is_empty() {
                        (*comp).to_string()
                    } else {
                        format!("{prefix}/{comp}")
                    };
                    if let Some(&did) = dir_ids.get(&prefix) {
                        parent = did;
                    } else {
                        let nid = self.nodes.len();
                        self.nodes.push(Node {
                            path: archive.join(&prefix),
                            name: (*comp).to_string(),
                            is_dir: true,
                            is_archive: false,
                            is_media: false,
                            depth,
                            parent: Some(parent),
                            children: Some(Vec::new()),
                            expanded: true, // pre-expanded: the whole subtree is known
                            size: 0,
                            count: 0,
                        });
                        dir_ids.insert(prefix.clone(), nid);
                        kids_of.entry(parent).or_default().push(nid);
                        parent = nid;
                    }
                }
            }
        }

        // Assign each container its children (dirs first, then A→Z, like fs scan).
        for (pid, mut kids) in kids_of {
            kids.sort_by(|&a, &b| {
                let (na, nb) = (&self.nodes[a], &self.nodes[b]);
                nb.is_dir
                    .cmp(&na.is_dir)
                    .then_with(|| na.name.cmp(&nb.name))
            });
            self.nodes[pid].children = Some(kids);
        }
        if self.nodes[id].children.is_none() {
            self.nodes[id].children = Some(Vec::new()); // empty archive
        }
        // Roll up track counts / sizes onto the inner directory nodes.
        self.recount(id);
    }

    /// Recompute (media-count, size) for a virtual subtree, writing the totals
    /// onto each directory node. Leaves contribute their own size.
    fn recount(&mut self, id: NodeId) -> (usize, u64) {
        let kids = self.nodes[id].children.clone().unwrap_or_default();
        if kids.is_empty() {
            return if self.nodes[id].is_media {
                (1, self.nodes[id].size)
            } else {
                (0, 0)
            };
        }
        let (mut c, mut s) = (0usize, 0u64);
        for k in kids {
            let (kc, ks) = self.recount(k);
            c += kc;
            s += ks;
        }
        if self.nodes[id].is_dir {
            self.nodes[id].count = c;
            self.nodes[id].size = s;
        }
        (c, s)
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
                } else if crate::archive::is_archive(&p) {
                    // An archive counts its audio toward the parent so a folder
                    // holding only archives isn't pruned as music-less.
                    let entries = crate::archive::list_audio(&p);
                    count += entries.len();
                    size += entries.iter().map(|e| e.size).sum::<u64>();
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
