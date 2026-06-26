//! App state and the single `update` surface. The UI thread only mutates state
//! here; rendering elsewhere only reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;

use crossbeam_channel::Sender;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig, SeekDirection,
};

use crate::audio::{AudioEngine, TransportCmd};
use crate::config::{
    SCOPE_PRESETS, ScopePreset, Settings, THEMES, Theme, load_settings, save_settings,
};
use crate::event::AppEvent;
use crate::media::{Meta, Registry};
use crate::model::{NodeId, TreeModel};

/// Number of static-waveform bins computed per track (resolution-independent).
const WAVE_BINS: usize = 1600;

/// Auto-advance / repeat behavior when a track finishes, cycled with `r`.
#[derive(Clone, Copy, PartialEq)]
pub enum LoopMode {
    /// Stop at the end of the list.
    Off,
    /// Advance, wrapping from last back to first.
    All,
    /// Repeat the current track.
    One,
}

impl LoopMode {
    fn next(self) -> Self {
        match self {
            LoopMode::Off => LoopMode::All,
            LoopMode::All => LoopMode::One,
            LoopMode::One => LoopMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LoopMode::Off => "loop off",
            LoopMode::All => "loop all",
            LoopMode::One => "loop one",
        }
    }
}

pub struct App {
    pub tree: TreeModel,
    pub registry: Registry,
    pub engine: AudioEngine,
    pub theme: Theme,
    pub theme_idx: usize,
    /// Monotonic redraw counter, drives prismatic border animation.
    pub frame: u64,
    pub list_state: ListState,

    pub meta_cache: HashMap<PathBuf, Meta>,
    pub wave_cache: HashMap<PathBuf, Vec<(f32, f32)>>,
    pub wave_pending: Option<PathBuf>,
    pub wave_gen: u64,

    pub now_playing: Option<PathBuf>,
    pub loop_mode: LoopMode,
    /// Previous engine play-state, to detect the end-of-track falling edge.
    prev_playing: bool,
    pub scope_buf: Vec<f32>,
    pub scope_idx: usize,

    pub filter: String,
    pub filtering: bool,
    /// Flat fuzzy-result paths (used while a filter is active).
    pub filtered: Vec<PathBuf>,
    /// Full background-built media-file index searched by the fuzzy filter.
    pub index: Vec<PathBuf>,
    /// Click-to-seek hit rects, written each render: the now-playing waveform
    /// panel and the transport bar.
    pub wave_rect: Rect,
    pub transport_rect: Rect,
    pub show_help: bool,
    /// Theme-picker modal: open flag, highlighted row, and the theme to restore
    /// on cancel.
    pub show_theme: bool,
    pub theme_sel: usize,
    theme_prev: usize,
    pub should_quit: bool,

    /// OS media-key integration (now-playing controls). `None` if unavailable.
    media: Option<MediaControls>,
    /// Last play-state pushed to the OS, to avoid redundant updates.
    media_playing: bool,

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
        // Restore the last-used scope preset + theme.
        let settings = load_settings();
        let scope_idx = settings
            .scope_preset
            .as_deref()
            .and_then(|name| SCOPE_PRESETS.iter().position(|p| p.name == name))
            .unwrap_or(0);
        let theme_idx = settings
            .theme
            .as_deref()
            .and_then(|name| THEMES.iter().position(|t| t.name == name))
            .unwrap_or(0);
        let mut app = Self {
            tree,
            registry,
            engine,
            theme: THEMES[theme_idx],
            theme_idx,
            frame: 0,
            list_state,
            meta_cache: HashMap::new(),
            wave_cache: HashMap::new(),
            wave_pending: None,
            wave_gen: 0,
            now_playing: None,
            loop_mode: LoopMode::Off,
            prev_playing: false,
            scope_buf: vec![0.0; crate::audio::SCOPE_LEN * 2],
            scope_idx,
            filter: String::new(),
            filtering: false,
            filtered: Vec::new(),
            index: Vec::new(),
            wave_rect: Rect::default(),
            transport_rect: Rect::default(),
            show_help: false,
            show_theme: false,
            theme_sel: theme_idx,
            theme_prev: theme_idx,
            media: init_media(&tx),
            media_playing: false,
            should_quit: false,
            tx,
        };
        app.on_selection_changed();
        Ok(app)
    }

    pub fn scope_preset(&self) -> ScopePreset {
        SCOPE_PRESETS[self.scope_idx % SCOPE_PRESETS.len()]
    }

    fn cycle_scope(&mut self, delta: i32) {
        let n = SCOPE_PRESETS.len() as i32;
        self.scope_idx = (((self.scope_idx as i32 + delta) % n + n) % n) as usize;
        self.persist();
    }

    fn open_theme_picker(&mut self) {
        self.show_theme = true;
        self.theme_prev = self.theme_idx;
        self.theme_sel = self.theme_idx;
    }

    /// Move the highlight (wrapping) and live-preview that theme.
    fn theme_move(&mut self, delta: i32) {
        let n = THEMES.len() as i32;
        self.theme_sel = (((self.theme_sel as i32 + delta) % n + n) % n) as usize;
        self.apply_preview();
    }

    fn theme_jump(&mut self, i: usize) {
        self.theme_sel = i.min(THEMES.len() - 1);
        self.apply_preview();
    }

    fn apply_preview(&mut self) {
        self.theme_idx = self.theme_sel;
        self.theme = THEMES[self.theme_idx];
    }

    fn theme_confirm(&mut self) {
        self.apply_preview();
        self.show_theme = false;
        self.persist();
    }

    fn theme_cancel(&mut self) {
        self.theme_idx = self.theme_prev;
        self.theme = THEMES[self.theme_idx];
        self.show_theme = false;
    }

    fn theme_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.theme_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.theme_move(-1),
            KeyCode::Char('g') | KeyCode::Home => self.theme_jump(0),
            KeyCode::Char('G') | KeyCode::End => self.theme_jump(THEMES.len() - 1),
            KeyCode::Enter => self.theme_confirm(),
            KeyCode::Esc | KeyCode::Char('q') => self.theme_cancel(),
            _ => {}
        }
    }

    fn persist(&self) {
        save_settings(&Settings {
            scope_preset: Some(self.scope_preset().name.to_string()),
            theme: Some(self.theme.name.to_string()),
        });
    }

    /// Whether the fuzzy filter is driving the list (vs. the file tree).
    pub fn filter_active(&self) -> bool {
        !self.filter.is_empty()
    }

    pub fn root_path(&self) -> PathBuf {
        self.tree.node(self.tree.root).path.clone()
    }

    /// The selected tree node — `None` while the fuzzy filter is active (the
    /// flat result list has no tree nodes), which also gates expand/collapse.
    pub fn cursor(&self) -> Option<NodeId> {
        if self.filter_active() {
            return None;
        }
        self.list_state
            .selected()
            .and_then(|i| self.tree.visible.get(i).copied())
    }

    pub fn cursor_path(&self) -> Option<PathBuf> {
        let i = self.list_state.selected()?;
        if self.filter_active() {
            self.filtered.get(i).cloned()
        } else {
            self.tree
                .visible
                .get(i)
                .map(|&id| self.tree.node(id).path.clone())
        }
    }

    fn list_len(&self) -> usize {
        if self.filter_active() {
            self.filtered.len()
        } else {
            self.tree.visible.len()
        }
    }

    pub fn handle(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Input(key) => self.on_key(key),
            AppEvent::Mouse(m) => self.on_mouse(m),
            AppEvent::Media(e) => self.on_media(e),
            AppEvent::Tick => {
                self.scope_buf.copy_from_slice(self.engine.scope());
                self.check_track_end();
                self.sync_media_playback();
                self.frame = self.frame.wrapping_add(1);
            }
            AppEvent::Wave(path, token, bins) => {
                if token == self.wave_gen {
                    self.wave_cache.insert(path.clone(), bins);
                    if self.wave_pending.as_ref() == Some(&path) {
                        self.wave_pending = None;
                    }
                }
            }
            AppEvent::Index(files) => {
                self.index = files;
                if self.filter_active() {
                    self.rebuild_filtered();
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
            // Modal: only q / esc dismiss; everything else is swallowed.
            if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }
        if self.show_theme {
            self.theme_key(key);
            return;
        }
        // Esc in normal mode clears an applied filter, restoring the tree view.
        if key.code == KeyCode::Esc && self.filter_active() {
            self.clear_filter();
            return;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            (KeyCode::Char('?'), _) => self.show_help = true,
            (KeyCode::Char('/'), _) => {
                self.filtering = true;
                self.list_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(0)
                });
            }
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.move_cursor(1),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.move_cursor(-1),
            (KeyCode::Char('g'), _) | (KeyCode::Home, _) => self.select(0),
            (KeyCode::Char('G'), _) | (KeyCode::End, _) => {
                self.select(self.list_len().saturating_sub(1))
            }
            // Shift+arrows scrub the playhead (fine seek); plain arrows nav tree.
            (KeyCode::Left, KeyModifiers::SHIFT) => self.engine.send(TransportCmd::SeekRel(-1.0)),
            (KeyCode::Right, KeyModifiers::SHIFT) => self.engine.send(TransportCmd::SeekRel(1.0)),
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => self.collapse_or_parent(),
            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => self.expand(),
            (KeyCode::Enter, _) => self.enter(),
            (KeyCode::Char(' '), _) => self.engine.send(TransportCmd::Toggle),
            (KeyCode::Char('n'), _) => self.play_relative(1),
            (KeyCode::Char('p'), _) => self.play_relative(-1),
            (KeyCode::Char('r'), _) => self.loop_mode = self.loop_mode.next(),
            (KeyCode::Char('.'), _) => self.engine.send(TransportCmd::SeekRel(5.0)),
            (KeyCode::Char(','), _) => self.engine.send(TransportCmd::SeekRel(-5.0)),
            (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => {
                self.engine.send(TransportCmd::VolRel(0.05))
            }
            (KeyCode::Char('-'), _) => self.engine.send(TransportCmd::VolRel(-0.05)),
            (KeyCode::Char('v'), _) => self.cycle_scope(1),
            (KeyCode::Char('V'), _) => self.cycle_scope(-1),
            (KeyCode::Char('t'), _) => self.open_theme_picker(),
            _ => {}
        }
    }

    fn filter_key(&mut self, key: KeyEvent) {
        match key.code {
            // Esc cancels: clear the query and return to the tree + normal keys.
            KeyCode::Esc => self.clear_filter(),
            // Enter accepts the filter; stop typing but keep results for nav/play.
            KeyCode::Enter => self.filtering = false,
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild_filtered();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.rebuild_filtered();
            }
            _ => {}
        }
    }

    /// Exit filtering entirely: drop the query/results and restore the tree
    /// view with the cursor on the first row.
    fn clear_filter(&mut self) {
        self.filtering = false;
        self.filter.clear();
        self.filtered.clear();
        self.list_state
            .select((!self.tree.visible.is_empty()).then_some(0));
        self.on_selection_changed();
    }

    /// Recompute fuzzy results from the background index against the query.
    fn rebuild_filtered(&mut self) {
        if self.filter.is_empty() {
            self.filtered.clear();
            self.list_state
                .select((!self.tree.visible.is_empty()).then_some(0));
            self.on_selection_changed();
            return;
        }
        let q = self.filter.to_lowercase();
        let root = self.root_path();
        let mut scored: Vec<(i32, &PathBuf)> = self
            .index
            .iter()
            .filter_map(|p| {
                let hay = p
                    .strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_lowercase();
                crate::util::fuzzy_score(&q, &hay).map(|s| (s, p))
            })
            .collect();
        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        self.filtered = scored.into_iter().map(|(_, p)| p.clone()).collect();
        self.list_state
            .select((!self.filtered.is_empty()).then_some(0));
        self.on_selection_changed();
    }

    fn move_cursor(&mut self, delta: i32) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let max = len as i32 - 1;
        self.select((cur + delta).clamp(0, max) as usize);
    }

    fn select(&mut self, i: usize) {
        self.list_state.select(Some(i));
        self.on_selection_changed();
    }

    fn expand(&mut self) {
        let Some(id) = self.cursor() else { return };
        if !self.tree.node(id).is_dir {
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
            self.tree.scan(id, &self.registry);
            self.tree.nodes[id].expanded = !expanded;
            self.refilter_keep();
        } else if node.is_media {
            let path = node.path.clone();
            self.play_path(path);
        }
    }

    /// Start playback of `path` (if it's a supported media file).
    fn play_path(&mut self, path: PathBuf) {
        if !self.registry.is_supported(&path) {
            return;
        }
        self.engine.send(TransportCmd::Open(path.clone()));
        self.now_playing = Some(path);
        // Open will flip the engine to playing; arm end-of-track detection.
        self.prev_playing = true;
        self.push_media_metadata();
    }

    /// Ordered media paths in the current view (filtered results, else the
    /// visible tree's media files) — the list `n`/`p` and auto-advance walk.
    fn current_media(&self) -> Vec<PathBuf> {
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

    /// Play the track `delta` steps from the current one in the view list,
    /// wrapping around the ends. Used by `n` / `p`.
    fn play_relative(&mut self, delta: i32) {
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

    /// Left-click on the now-playing waveform or transport bar seeks the track
    /// to the matching position.
    fn on_mouse(&mut self, m: MouseEvent) {
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let dur = self.engine.duration_secs();
        if dur <= 0.0 {
            return;
        }
        for rect in [self.wave_rect, self.transport_rect] {
            if let Some(frac) = frac_in_rect(rect, m.column, m.row) {
                self.engine.send(TransportCmd::SeekTo(frac * dur));
                return;
            }
        }
    }

    /// Handle an OS media-key / now-playing control event.
    fn on_media(&mut self, e: MediaControlEvent) {
        match e {
            MediaControlEvent::Play => self.engine.send(TransportCmd::Play),
            MediaControlEvent::Pause => self.engine.send(TransportCmd::Pause),
            MediaControlEvent::Toggle => self.engine.send(TransportCmd::Toggle),
            MediaControlEvent::Stop => self.engine.send(TransportCmd::Pause),
            MediaControlEvent::Next => self.play_relative(1),
            MediaControlEvent::Previous => self.play_relative(-1),
            MediaControlEvent::Seek(SeekDirection::Forward) => {
                self.engine.send(TransportCmd::SeekRel(5.0))
            }
            MediaControlEvent::Seek(SeekDirection::Backward) => {
                self.engine.send(TransportCmd::SeekRel(-5.0))
            }
            MediaControlEvent::SeekBy(dir, dur) => {
                let s = dur.as_secs_f64();
                let d = if matches!(dir, SeekDirection::Backward) {
                    -s
                } else {
                    s
                };
                self.engine.send(TransportCmd::SeekRel(d));
            }
            MediaControlEvent::SetPosition(pos) => {
                self.engine.send(TransportCmd::SeekTo(pos.0.as_secs_f64()))
            }
            MediaControlEvent::Quit => self.should_quit = true,
            _ => {}
        }
    }

    /// Push the now-playing track's tags to the OS now-playing display.
    fn push_media_metadata(&mut self) {
        let Some(p) = self.now_playing.clone() else {
            return;
        };
        let meta = self.meta_cache.get(&p);
        let title = meta
            .filter(|m| !m.title.is_empty())
            .map(|m| m.title.clone())
            .unwrap_or_else(|| {
                p.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let artist = meta.map(|m| m.artist.clone()).unwrap_or_default();
        let album = meta.map(|m| m.album.clone()).unwrap_or_default();
        let duration = meta
            .map(|m| m.duration)
            .filter(|d| *d > 0.0)
            .map(std::time::Duration::from_secs_f64);
        if let Some(controls) = self.media.as_mut() {
            let _ = controls.set_metadata(MediaMetadata {
                title: Some(&title),
                artist: (!artist.is_empty()).then_some(&artist),
                album: (!album.is_empty()).then_some(&album),
                duration,
                cover_url: None,
            });
        }
    }

    /// Keep the OS play/pause indicator in sync with the engine.
    fn sync_media_playback(&mut self) {
        let playing = self.engine.is_playing();
        if playing == self.media_playing {
            return;
        }
        self.media_playing = playing;
        if let Some(controls) = self.media.as_mut() {
            let state = if playing {
                MediaPlayback::Playing { progress: None }
            } else {
                MediaPlayback::Paused { progress: None }
            };
            let _ = controls.set_playback(state);
        }
    }

    /// On the play->stop edge at the end of a track, apply the loop mode.
    fn check_track_end(&mut self) {
        let playing = self.engine.is_playing();
        let dur = self.engine.duration_secs();
        let ended =
            self.prev_playing && !playing && dur > 0.0 && self.engine.position_secs() >= dur - 0.05;
        self.prev_playing = playing;
        if !ended {
            return;
        }
        match self.loop_mode {
            LoopMode::Off => {
                // advance, but stop at the end of the list
                let list = self.current_media();
                let next = self
                    .now_playing
                    .as_ref()
                    .and_then(|p| list.iter().position(|x| x == p))
                    .map(|i| i + 1)
                    .filter(|&i| i < list.len());
                if let Some(i) = next {
                    self.play_path(list[i].clone());
                }
            }
            LoopMode::All => self.play_relative(1),
            LoopMode::One => {
                if let Some(p) = self.now_playing.clone() {
                    self.play_path(p);
                }
            }
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
    fn on_selection_changed(&mut self) {
        let Some(path) = self.cursor_path() else {
            return;
        };
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

/// Best-effort OS media-control setup. Returns `None` (and the app keeps
/// working) if the platform integration is unavailable.
fn init_media(tx: &Sender<AppEvent>) -> Option<MediaControls> {
    let config = PlatformConfig {
        dbus_name: "muse",
        display_name: "muse",
        hwnd: console_hwnd(),
    };
    let mut controls = MediaControls::new(config).ok()?;
    let tx = tx.clone();
    controls
        .attach(move |event| {
            let _ = tx.send(AppEvent::Media(event));
        })
        .ok()?;
    Some(controls)
}

/// Windows SMTC needs a window handle; a console app can borrow the console's.
#[cfg(target_os = "windows")]
fn console_hwnd() -> Option<*mut std::ffi::c_void> {
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }
    let h = unsafe { GetConsoleWindow() };
    (!h.is_null()).then_some(h)
}

#[cfg(not(target_os = "windows"))]
fn console_hwnd() -> Option<*mut std::ffi::c_void> {
    None
}

/// Fraction (0..1) along `r`'s inner (inside-border) width at column `col`, if
/// the point is inside the rect; `None` otherwise.
fn frac_in_rect(r: Rect, col: u16, row: u16) -> Option<f64> {
    if r.width < 3 || r.height < 2 {
        return None;
    }
    if row < r.y || row >= r.y + r.height || col <= r.x || col >= r.x + r.width - 1 {
        return None;
    }
    let left = r.x + 1;
    let span = (r.width - 2) as f64;
    Some(((col - left) as f64 / span).clamp(0.0, 1.0))
}
