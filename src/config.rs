//! Theme + (future) config. Colors live here so the "elegant" polish is one place.

use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub accent: Color,
    pub accent2: Color,
    pub dir: Color,
    pub media: Color,
    pub dim: Color,
    pub scope: Color,
    pub wave: Color,
    pub playing: Color,
    pub bg_sel: Color,
    /// Animate every panel border through a shifting rainbow.
    pub prismatic: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

/// Selectable color themes, cycled with `t`. Includes pride-flag palettes and a
/// prismatic mode whose borders shift through the spectrum over time.
pub const THEMES: &[Theme] = &[
    Theme {
        name: "midnight",
        accent: rgb(0x8b, 0xe9, 0xfd),
        accent2: rgb(0xbd, 0x93, 0xf9),
        dir: rgb(0x82, 0xaa, 0xff),
        media: rgb(0xc8, 0xd3, 0xf5),
        dim: rgb(0x6c, 0x73, 0x94),
        scope: rgb(0x50, 0xfa, 0x7b),
        wave: rgb(0x6a, 0x84, 0xc4),
        playing: rgb(0xff, 0xb8, 0x6c),
        bg_sel: rgb(0x2a, 0x2e, 0x42),
        prismatic: false,
    },
    Theme {
        name: "pride",
        accent: rgb(0xff, 0x5d, 0x5d),
        accent2: rgb(0xb0, 0x6c, 0xff),
        dir: rgb(0xff, 0xd9, 0x3d),
        media: rgb(0xf5, 0xf5, 0xf5),
        dim: rgb(0x8a, 0x8a, 0x9a),
        scope: rgb(0x50, 0xfa, 0x7b),
        wave: rgb(0x5d, 0x8c, 0xff),
        playing: rgb(0xff, 0x8c, 0x00),
        bg_sel: rgb(0x24, 0x1b, 0x2f),
        prismatic: false,
    },
    Theme {
        name: "trans",
        accent: rgb(0x5b, 0xce, 0xfa),
        accent2: rgb(0xf5, 0xa9, 0xb8),
        dir: rgb(0x5b, 0xce, 0xfa),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x7d, 0x8a, 0xa0),
        scope: rgb(0xf5, 0xa9, 0xb8),
        wave: rgb(0x5b, 0xce, 0xfa),
        playing: rgb(0xf5, 0xa9, 0xb8),
        bg_sel: rgb(0x1b, 0x26, 0x30),
        prismatic: false,
    },
    Theme {
        name: "bi",
        accent: rgb(0xff, 0x4f, 0x9b),
        accent2: rgb(0x9b, 0x6f, 0xd6),
        dir: rgb(0x6f, 0x8c, 0xff),
        media: rgb(0xec, 0xec, 0xf5),
        dim: rgb(0x7a, 0x7a, 0x90),
        scope: rgb(0xb0, 0x6c, 0xff),
        wave: rgb(0x5d, 0x6c, 0xff),
        playing: rgb(0xff, 0x4f, 0x9b),
        bg_sel: rgb(0x1e, 0x1a, 0x2e),
        prismatic: false,
    },
    Theme {
        name: "lesbian",
        accent: rgb(0xff, 0x6d, 0x3a),
        accent2: rgb(0xff, 0x9e, 0xc4),
        dir: rgb(0xff, 0x9a, 0x56),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x8a, 0x7d, 0x80),
        scope: rgb(0xff, 0x6d, 0x3a),
        wave: rgb(0xff, 0x9e, 0xc4),
        playing: rgb(0xff, 0x5e, 0x8a),
        bg_sel: rgb(0x2a, 0x1c, 0x1c),
        prismatic: false,
    },
    Theme {
        name: "pan",
        accent: rgb(0xff, 0x21, 0x8c),
        accent2: rgb(0x21, 0xb1, 0xff),
        dir: rgb(0xff, 0xd8, 0x00),
        media: rgb(0xf5, 0xf5, 0xf5),
        dim: rgb(0x8a, 0x8a, 0x9a),
        scope: rgb(0xff, 0xd8, 0x00),
        wave: rgb(0x21, 0xb1, 0xff),
        playing: rgb(0xff, 0x21, 0x8c),
        bg_sel: rgb(0x22, 0x18, 0x26),
        prismatic: false,
    },
    Theme {
        name: "nonbinary",
        accent: rgb(0xfc, 0xf4, 0x34),
        accent2: rgb(0x9c, 0x59, 0xd1),
        dir: rgb(0xb8, 0x84, 0xe8),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x7a, 0x7a, 0x85),
        scope: rgb(0xfc, 0xf4, 0x34),
        wave: rgb(0x9c, 0x59, 0xd1),
        playing: rgb(0xd6, 0xa8, 0xff),
        bg_sel: rgb(0x23, 0x20, 0x26),
        prismatic: false,
    },
    Theme {
        name: "ace",
        accent: rgb(0xb1, 0x5c, 0xff),
        accent2: rgb(0xa4, 0xa4, 0xa4),
        dir: rgb(0xc8, 0xc8, 0xc8),
        media: rgb(0xec, 0xec, 0xec),
        dim: rgb(0x6a, 0x6a, 0x6a),
        scope: rgb(0xb1, 0x5c, 0xff),
        wave: rgb(0x8a, 0x8a, 0x8a),
        playing: rgb(0xd0, 0xa0, 0xff),
        bg_sel: rgb(0x1c, 0x1c, 0x20),
        prismatic: false,
    },
    Theme {
        name: "prismatic",
        accent: rgb(0xc8, 0xd3, 0xf5),
        accent2: rgb(0xc8, 0xd3, 0xf5),
        dir: rgb(0x82, 0xaa, 0xff),
        media: rgb(0xe8, 0xec, 0xf8),
        dim: rgb(0x6c, 0x73, 0x94),
        scope: rgb(0x50, 0xfa, 0x7b),
        wave: rgb(0x6a, 0x84, 0xc4),
        playing: rgb(0xff, 0xb8, 0x6c),
        bg_sel: rgb(0x24, 0x28, 0x3b),
        prismatic: true,
    },
];

impl Default for Theme {
    fn default() -> Self {
        THEMES[0]
    }
}

/// How the live oscilloscope trace is drawn.
#[derive(Clone, Copy, PartialEq)]
pub enum ScopeStyle {
    Line,
    Mirror,
    Dots,
    Bars,
}

/// What signal the scope plots.
#[derive(Clone, Copy, PartialEq)]
pub enum ScopeMode {
    /// Time-domain mono trace (L+R folded).
    Mono,
    /// Stereo vectorscope: X = left, Y = right (Lissajous).
    StereoXy,
}

/// A named bundle of scope settings, cycled with one key. Bundles the four
/// tunables the user can vary: render style, signal mode, time window, gain.
#[derive(Clone, Copy)]
pub struct ScopePreset {
    pub name: &'static str,
    pub style: ScopeStyle,
    pub mode: ScopeMode,
    /// Frames of the most-recent scope window to display (<= audio::SCOPE_LEN).
    /// Smaller = shorter window = a snappier, more responsive trace.
    pub window: usize,
    /// Normalize the window so its peak fills the panel (quiet passages stay
    /// visible). Off keeps absolute amplitude.
    pub auto_gain: bool,
}

/// Persisted user settings (TOML at the platform config dir).
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    /// Name of the last-used scope preset, restored on next launch.
    pub scope_preset: Option<String>,
    /// Name of the last-used color theme.
    pub theme: Option<String>,
}

fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.config_dir().join("config.toml"))
}

pub fn load_settings() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(txt) = toml::to_string(s) {
        let _ = std::fs::write(path, txt);
    }
}

/// Cycle order. Covers all four axes: styles, stereo XY, window sizes, gain.
pub const SCOPE_PRESETS: &[ScopePreset] = &[
    ScopePreset {
        name: "line",
        style: ScopeStyle::Line,
        mode: ScopeMode::Mono,
        window: 1024,
        auto_gain: false,
    },
    ScopePreset {
        name: "fast",
        style: ScopeStyle::Line,
        mode: ScopeMode::Mono,
        window: 384,
        auto_gain: true,
    },
    ScopePreset {
        name: "mirror",
        style: ScopeStyle::Mirror,
        mode: ScopeMode::Mono,
        window: 1024,
        auto_gain: true,
    },
    ScopePreset {
        name: "dots",
        style: ScopeStyle::Dots,
        mode: ScopeMode::Mono,
        window: 768,
        auto_gain: true,
    },
    ScopePreset {
        name: "bars",
        style: ScopeStyle::Bars,
        mode: ScopeMode::Mono,
        window: 1024,
        auto_gain: false,
    },
    ScopePreset {
        name: "stereo xy",
        style: ScopeStyle::Line,
        mode: ScopeMode::StereoXy,
        window: 2048,
        auto_gain: true,
    },
];
