//! App state and the single `update` surface. The UI thread only mutates state
//! here; rendering elsewhere only reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;

use crossbeam_channel::Sender;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::audio::{AudioEngine, TransportCmd};
use crate::config::Theme;
use crate::event::AppEvent;
use crate::media::{Meta, Registry};
use crate::model::{NodeId, TreeModel};

/// Number of static-waveform bins computed per track (resolution-independent).
const WAVE_BINS: usize = 1600;

pub struct App {
    pub tree: TreeModel,
    pub registry: Registry,
    pub engine: AudioEngine,
    pub theme: Theme,
    pub list_state: ListState,

    pub meta_cache: HashMap<PathBuf, Meta>,
    pub wave_cache: HashMap<PathBuf, Vec<(f32, f32)>>,
    pub wave_pending: Option<PathBuf>,
    pub wave_gen: u64,

    pub now_playing: Option<PathBuf>,
    pub scope_buf: Vec<f32>,
    pub status: String,

    pub filter: String,
    pub filtering: bool,
    pub show_help: bool,
    pub should_quit: bool,

    tx: Sender<AppEvent>,
}

impl App {
    pub fn new(root: &Path, tx: Sender<AppEvent>) -> anyhow::Result<Self> {
        let registry = Registry::new();
        let tree = TreeModel::new(root, &registry);
        let engine = AudioEngine::new()?;
        let mut list_state = ListState::default();
        if !tree.visible.is_empty() {
            list_state.select(Some(0));
        }
        let mut app = Self {
            tree,
            registry,
            engine,
            theme: Theme::default(),
            list_state,
            meta_cache: HashMap::new(),
            wave_cache: HashMap::new(),
            wave_pending: None,
            wave_gen: 0,
            now_playing: None,
            scope_buf: vec![0.0; crate::audio::SCOPE_LEN],
            status: "↑↓ move · l expand · ⏎ play · space pause · ←→ seek · / filter · ? help · q quit".into(),
            filter: String::new(),
            filtering: false,
            show_help: false,
            should_quit: false,
            tx,
        };
        app.on_selection_changed();
        Ok(app)
    }

    pub fn cursor(&self) -> Option<NodeId> {
        self.list_state
            .selected()
            .and_then(|i| self.tree.visible.get(i).copied())
    }

    pub fn cursor_path(&self) -> Option<PathBuf> {
        self.cursor().map(|id| self.tree.node(id).path.clone())
    }

    fn filter_opt(&self) -> Option<String> {
        if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.to_lowercase())
        }
    }

    pub fn handle(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Input(key) => self.on_key(key),
            AppEvent::Tick => {
                self.scope_buf.copy_from_slice(self.engine.scope());
            }
            AppEvent::Wave(path, token, bins) => {
                if token == self.wave_gen {
                    self.wave_cache.insert(path.clone(), bins);
                    if self.wave_pending.as_ref() == Some(&path) {
                        self.wave_pending = None;
                    }
                }
            }
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if self.filtering {
            self.filter_key(key);
            return;
        }
        if self.show_help {
            self.show_help = false;
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Char('?'), _) => self.show_help = true,
            (KeyCode::Char('/'), _) => {
                self.filtering = true;
                self.status = "filter: ".into();
            }
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.move_cursor(1),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.move_cursor(-1),
            (KeyCode::Char('g'), _) | (KeyCode::Home, _) => self.select(0),
            (KeyCode::Char('G'), _) | (KeyCode::End, _) => {
                self.select(self.tree.visible.len().saturating_sub(1))
            }
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => self.collapse_or_parent(),
            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => self.expand(),
            (KeyCode::Enter, _) => self.enter(),
            (KeyCode::Char(' '), _) | (KeyCode::Char('p'), _) => {
                self.engine.send(TransportCmd::Toggle)
            }
            (KeyCode::Char('.'), _) => self.engine.send(TransportCmd::SeekRel(5.0)),
            (KeyCode::Char(','), _) => self.engine.send(TransportCmd::SeekRel(-5.0)),
            (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => {
                self.engine.send(TransportCmd::VolRel(0.05))
            }
            (KeyCode::Char('-'), _) => self.engine.send(TransportCmd::VolRel(-0.05)),
            _ => {}
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filtering = false;
                self.filter.clear();
                self.refilter();
            }
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.refilter();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.refilter();
            }
            _ => {}
        }
    }

    fn refilter(&mut self) {
        let f = self.filter_opt();
        self.tree.rebuild_visible(f.as_deref());
        if self.tree.visible.is_empty() {
            self.list_state.select(None);
        } else {
            let i = self
                .list_state
                .selected()
                .unwrap_or(0)
                .min(self.tree.visible.len() - 1);
            self.list_state.select(Some(i));
        }
        self.on_selection_changed();
    }

    fn move_cursor(&mut self, delta: i32) {
        if self.tree.visible.is_empty() {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let max = self.tree.visible.len() as i32 - 1;
        self.select((cur + delta).clamp(0, max) as usize);
    }

    fn select(&mut self, i: usize) {
        self.list_state.select(Some(i));
        self.on_selection_changed();
    }

    fn expand(&mut self) {
        if let Some(id) = self.cursor() {
            if self.tree.node(id).is_dir {
                self.tree.scan(id, &self.registry);
                self.tree.nodes[id].expanded = true;
                self.refilter_keep();
            }
        }
    }

    fn collapse_or_parent(&mut self) {
        let Some(id) = self.cursor() else { return };
        let node = self.tree.node(id);
        if node.is_dir && node.expanded {
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

    fn enter(&mut self) {
        let Some(id) = self.cursor() else { return };
        let node = self.tree.node(id);
        if node.is_dir {
            let expanded = node.expanded;
            self.tree.scan(id, &self.registry);
            self.tree.nodes[id].expanded = !expanded;
            self.refilter_keep();
        } else if node.is_media {
            let path = node.path.clone();
            self.engine.send(TransportCmd::Open(path.clone()));
            self.now_playing = Some(path);
        }
    }

    /// Rebuild visible but keep the current cursor node selected if still visible.
    fn refilter_keep(&mut self) {
        let keep = self.cursor();
        let f = self.filter_opt();
        self.tree.rebuild_visible(f.as_deref());
        if let Some(k) = keep {
            if let Some(pos) = self.tree.visible.iter().position(|&v| v == k) {
                self.list_state.select(Some(pos));
            }
        }
        self.on_selection_changed();
    }

    /// Lazy-load metadata + kick off a waveform compute for the selected media.
    fn on_selection_changed(&mut self) {
        let Some(path) = self.cursor_path() else { return };
        if !self.registry.is_supported(&path) {
            return;
        }
        if !self.meta_cache.contains_key(&path) {
            if let Some(p) = self.registry.for_path(&path) {
                self.meta_cache.insert(path.clone(), p.metadata(&path));
            }
        }
        if !self.wave_cache.contains_key(&path) && self.wave_pending.as_ref() != Some(&path) {
            self.request_waveform(path);
        }
    }

    fn request_waveform(&mut self, path: PathBuf) {
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
}
