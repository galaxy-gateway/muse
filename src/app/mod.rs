//! App state and the single `update` surface. The UI thread only mutates state
//! here; rendering elsewhere only reads. The `App` struct lives here; its
//! behavior is split across sibling modules (input, nav, playback, mouse,
//! mediakeys) whose methods are `pub(super)` so they compose within `app`.

mod input;
mod mediakeys;
mod mouse;
mod nav;
mod playback;
mod queue;
mod shuffle;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossbeam_channel::Sender;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use souvlaki::MediaControls;

use crate::audio::{AudioEngine, TransportCmd};
use crate::config::{
    SCOPE_PRESETS, ScopePreset, ScopeStyle, Settings, THEMES, Theme, load_settings, save_settings,
};
use crate::effects::FrameCtx;
use crate::event::AppEvent;
use crate::media::{Meta, Registry};
use crate::model::{NodeId, TreeModel};
use crate::particles::ParticleSim;
use crate::spectrum::SpectrumState;

use mediakeys::init_media;

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
    pub(super) fn next(self) -> Self {
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

    fn as_str(self) -> &'static str {
        match self {
            LoopMode::Off => "off",
            LoopMode::All => "all",
            LoopMode::One => "one",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "off" => Some(LoopMode::Off),
            "all" => Some(LoopMode::All),
            "one" => Some(LoopMode::One),
            _ => None,
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
    /// Persistent on-disk tag cache (path+size+mtime keyed); backs `meta_cache`
    /// so re-launches skip re-parsing unchanged files.
    pub(super) meta_disk: crate::metacache::MetaCache,
    pub wave_cache: HashMap<PathBuf, Vec<(f32, f32)>>,
    pub wave_pending: Option<PathBuf>,
    pub wave_gen: u64,

    /// Decoded embedded cover art per track (`None` = no art); built off-thread.
    pub wave_art: HashMap<PathBuf, Option<image::RgbImage>>,
    pub(super) art_pending: Option<PathBuf>,
    /// Show album art instead of the static waveform in the inspector (`i`).
    pub show_art: bool,

    pub now_playing: Option<PathBuf>,
    /// The track playing before the current one, with its playhead at the moment
    /// we switched away — lets `u` jump back and resume (undo an accidental click).
    pub(super) prev_track: Option<(PathBuf, f64)>,
    /// Explicit play queue (absolute paths). When the now-playing track is in it,
    /// next/prev and the gapless prefetch walk the queue instead of the tree list.
    pub queue: Vec<PathBuf>,
    /// Queue-manager modal: open flag + its cursor row.
    pub show_queue: bool,
    pub(super) queue_sel: usize,
    pub loop_mode: LoopMode,
    /// Shuffle mode: pick the next track from a no-repeat bag. Persisted.
    pub shuffle: bool,
    /// Upcoming shuffled tracks (excludes the now-playing one); refilled from the
    /// active pool when drained. `play_history` backs shuffle's `previous`.
    pub(super) shuffle_bag: Vec<PathBuf>,
    pub(super) play_history: Vec<PathBuf>,
    /// xorshift64 state for shuffle (seeded at startup; no `rand` dependency).
    pub(super) rng: u64,
    /// Held-nav acceleration: the last j/k/↑/↓ direction + when it fired, and how
    /// many consecutive same-direction repeats have stacked. Drives `move_cursor_accel`.
    pub(super) nav_last: Option<(i32, Instant)>,
    pub(super) nav_streak: u32,
    /// Gapless cross-track playback: prefetch the predicted-next track so the
    /// decode thread can splice it in with no boundary gap. Persisted.
    pub gapless: bool,
    /// Frame at which each panel's path was last copied to the clipboard, so its
    /// copy button can flash a checkmark briefly. `None` = idle.
    pub copy_flash_np: Option<u64>,
    pub copy_flash_sel: Option<u64>,
    /// Previous engine play-state, to detect the end-of-track falling edge.
    pub(super) prev_playing: bool,
    pub scope_buf: Vec<f32>,
    pub scope_idx: usize,
    /// FFT band state for the live spectrum visualizer preset.
    pub spectrum: SpectrumState,

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
    pub tree_rect: Rect,
    pub scope_rect: Rect,
    /// Now-playing panel rect (inspector's top row), recorded each render for
    /// the flag overlay.
    pub np_rect: Rect,
    /// Selection detail panel rect (left column, top), for its copy button.
    pub sel_rect: Rect,
    pub screen: Rect,
    /// While a left-drag started on a seek bar, the rect being scrubbed.
    pub(super) seeking_rect: Option<Rect>,
    /// Playhead position when the current scrub began, for the drag tooltip's delta.
    pub(super) seek_start_secs: Option<f64>,
    /// Horizontal pan offset for the tree (mouse horizontal scroll).
    pub tree_hscroll: u16,
    /// Last mouse position, for hover highlighting of interactable parts.
    pub hover: Option<(u16, u16)>,
    /// Live particle simulation, driven by the active theme's effect.
    pub sim: ParticleSim,
    pub show_help: bool,
    /// Theme-picker modal: open flag, highlighted row, and the theme to restore
    /// on cancel.
    pub show_theme: bool,
    pub theme_sel: usize,
    pub(super) theme_prev: usize,
    pub should_quit: bool,

    /// OS media-key integration (now-playing controls). `None` if unavailable.
    pub(super) media: Option<MediaControls>,
    /// Last play-state pushed to the OS, to avoid redundant updates.
    pub(super) media_playing: bool,

    pub(super) tx: Sender<AppEvent>,
}

impl App {
    pub fn new(root: &Path, tx: Sender<AppEvent>) -> anyhow::Result<Self> {
        let registry = Registry::new();
        let tree = TreeModel::new(root, &registry);
        let engine = AudioEngine::new()?;
        let spectrum = SpectrumState::new(engine.sample_rate());
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
            meta_disk: crate::metacache::MetaCache::load(),
            wave_cache: HashMap::new(),
            wave_pending: None,
            wave_gen: 0,
            wave_art: HashMap::new(),
            art_pending: None,
            show_art: false,
            now_playing: None,
            prev_track: None,
            queue: Vec::new(),
            show_queue: false,
            queue_sel: 0,
            loop_mode: LoopMode::Off,
            shuffle: settings.shuffle.unwrap_or(false),
            shuffle_bag: Vec::new(),
            play_history: Vec::new(),
            // Seed the PRNG from wall-clock nanos (any nonzero value works).
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E3779B97F4A7C15)
                | 1,
            nav_last: None,
            nav_streak: 0,
            gapless: settings.gapless.unwrap_or(true),
            copy_flash_np: None,
            copy_flash_sel: None,
            prev_playing: false,
            scope_buf: vec![0.0; crate::audio::SCOPE_LEN * 2],
            scope_idx,
            spectrum,
            filter: String::new(),
            filtering: false,
            filtered: Vec::new(),
            index: Vec::new(),
            wave_rect: Rect::default(),
            transport_rect: Rect::default(),
            tree_rect: Rect::default(),
            scope_rect: Rect::default(),
            np_rect: Rect::default(),
            sel_rect: Rect::default(),
            screen: Rect::default(),
            seeking_rect: None,
            seek_start_secs: None,
            tree_hscroll: 0,
            hover: None,
            sim: ParticleSim::new(),
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
        app.restore_session(&settings);
        Ok(app)
    }

    /// Restore the last session: loop mode, volume, tree cursor, and the track
    /// that was playing (reopened paused at its saved offset). Best-effort — a
    /// missing/moved file or a cursor under a collapsed dir is simply skipped.
    fn restore_session(&mut self, s: &Settings) {
        if let Some(lm) = s.session_loop.as_deref().and_then(LoopMode::from_str) {
            self.loop_mode = lm;
        }
        if let Some(v) = s.session_volume {
            self.engine.send(TransportCmd::SetVol(v));
        }
        if let Some(cur) = &s.session_cursor {
            let cur = PathBuf::from(cur);
            if let Some(pos) = self
                .tree
                .visible
                .iter()
                .position(|&id| self.tree.node(id).path == cur)
            {
                self.list_state.select(Some(pos));
                self.on_selection_changed();
            }
        }
        if let Some(track) = &s.session_track {
            let path = PathBuf::from(track);
            if path.exists() && self.registry.is_supported(&path) {
                self.engine.send(TransportCmd::Open(path.clone()));
                self.begin_now_playing(path);
                if let Some(pos) = s.session_pos {
                    self.engine.send(TransportCmd::SeekTo(pos));
                }
                // Resume paused, so reopening muse never blasts audio unexpectedly.
                self.engine.send(TransportCmd::Pause);
                self.prev_playing = false;
            }
        }
    }

    /// Move the tree cursor onto the now-playing track if it is in the current
    /// view (filtered results or the visible tree). Best-effort.
    pub(super) fn jump_to_now_playing(&mut self) {
        let Some(p) = self.now_playing.clone() else {
            return;
        };
        let pos = if self.filter_active() {
            self.filtered.iter().position(|x| x == &p)
        } else {
            self.tree
                .visible
                .iter()
                .position(|&id| self.tree.node(id).path == p)
        };
        if let Some(pos) = pos {
            self.list_state.select(Some(pos));
            self.on_selection_changed();
        }
    }

    pub fn scope_preset(&self) -> ScopePreset {
        SCOPE_PRESETS[self.scope_idx % SCOPE_PRESETS.len()]
    }

    /// Build the full settings snapshot (preferences + last-session state).
    fn current_settings(&self) -> Settings {
        let p2s = |p: &PathBuf| p.to_string_lossy().into_owned();
        Settings {
            scope_preset: Some(self.scope_preset().name.to_string()),
            theme: Some(self.theme.name.to_string()),
            gapless: Some(self.gapless),
            shuffle: Some(self.shuffle),
            session_track: self.now_playing.as_ref().map(p2s),
            session_pos: self
                .now_playing
                .as_ref()
                .map(|_| self.engine.position_secs()),
            session_volume: Some(self.engine.volume()),
            session_loop: Some(self.loop_mode.as_str().to_string()),
            session_cursor: self.cursor_path().as_ref().map(p2s),
        }
    }

    pub(super) fn persist(&self) {
        save_settings(&self.current_settings());
    }

    /// Persist the full state on exit (call from `main` after the event loop).
    pub fn save_state(&self) {
        self.persist();
        self.meta_disk.save();
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

    /// Exact text shown on the now-playing panel's title row (tag title, else
    /// filename). `None` when nothing is playing. The copy button is positioned
    /// from this string's width, so it must match what `draw_now_playing` renders.
    pub fn now_playing_title_text(&self) -> Option<String> {
        let p = self.now_playing.as_ref()?;
        Some(tag_title_or_filename(self.meta_cache.get(p), p))
    }

    /// Exact text on the selection panel's title row — mirrors `draw_selection`:
    /// tag title, else "reading tags…" while a supported file's tags load, else
    /// the filename. `None` when the cursor has no path.
    pub fn selection_title_text(&self) -> Option<String> {
        let p = self.cursor_path()?;
        Some(match self.meta_cache.get(&p) {
            Some(m) if !m.title.is_empty() => m.title.clone(),
            Some(_) => file_name_str(&p),
            None if self.registry.is_supported(&p) => "reading tags…".to_string(),
            None => file_name_str(&p),
        })
    }

    pub(super) fn list_len(&self) -> usize {
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
                // Only run the FFT when the spectrum visualizer is the active
                // preset — it is the sole reader of the band state.
                if self.scope_preset().style == ScopeStyle::Spectrum {
                    if self.engine.is_playing() {
                        self.spectrum.update_scope(&self.scope_buf);
                    } else {
                        self.spectrum.decay();
                    }
                }
                // Decode-thread gapless advance happens before the UI knows;
                // adopt the new now-playing first, then run the stop-edge
                // detector (which only fires when no preload was spliced).
                while let Some(path) = self.engine.poll_advance() {
                    self.on_auto_advanced(path);
                }
                self.check_track_end();
                self.sync_media_playback();
                let ctx = self.frame_ctx();
                let fx = self.theme.effect;
                fx.ambient(&mut self.sim, &ctx);
                let wind = fx.wind(&ctx);
                self.sim.update(ctx.screen, wind);
                self.frame = self.frame.wrapping_add(1);
            }
            AppEvent::Wave(path, _token, bins) => {
                // Cache by path regardless of generation — bins are always
                // correct for their path, and a now-playing request must not be
                // dropped just because the tree selection moved on.
                self.wave_cache.insert(path.clone(), bins);
                if self.wave_pending.as_ref() == Some(&path) {
                    self.wave_pending = None;
                }
            }
            AppEvent::Art(path, art) => {
                self.wave_art.insert(path.clone(), art);
                if self.art_pending.as_ref() == Some(&path) {
                    self.art_pending = None;
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

    /// Snapshot the per-frame geometry + playback state the active theme effect
    /// reads when spawning/rendering particles.
    pub(crate) fn frame_ctx(&self) -> FrameCtx {
        let scope_peak = self.scope_buf.iter().fold(0f32, |m, &s| m.max(s.abs()));
        let cursor_row = self
            .list_state
            .selected()
            .map(|s| s.saturating_sub(self.list_state.offset()) as u16);
        let cursor_index = self.list_state.selected().map(|s| s as u32);
        let (dur, pos, playing) = (
            self.engine.duration_secs(),
            self.engine.position_secs(),
            self.engine.is_playing(),
        );
        let play_frac = (dur > 0.0 && self.now_playing.is_some() && playing)
            .then(|| (pos / dur).clamp(0.0, 1.0));
        FrameCtx {
            frame: self.frame,
            screen: self.screen,
            tree_rect: self.tree_rect,
            scope_rect: self.scope_rect,
            wave_rect: self.wave_rect,
            np_rect: self.np_rect,
            hover: self.hover,
            scope_peak,
            cursor_row,
            cursor_index,
            play_frac,
        }
    }
}

/// A file's final path component as a lossy `String` (empty if it has none).
fn file_name_str(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Tag title if present and non-empty, else the filename — the now-playing
/// title-row text.
fn tag_title_or_filename(meta: Option<&crate::media::Meta>, p: &Path) -> String {
    meta.filter(|m| !m.title.is_empty())
        .map(|m| m.title.clone())
        .unwrap_or_else(|| file_name_str(p))
}
