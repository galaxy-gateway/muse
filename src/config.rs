//! Theme + (future) config. Colors live here so the "elegant" polish is one place.

use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::effects::ThemeEffect;

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
    /// The theme's animated behavior (border color, overlay, particle reactions).
    /// Static themes point at `effects::NONE`.
    pub effect: &'static dyn ThemeEffect,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::NONE,
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
        effect: &crate::effects::PRISMATIC,
    },
    Theme {
        name: "trans flow",
        accent: rgb(0x5b, 0xce, 0xfa),
        accent2: rgb(0xf5, 0xa9, 0xb8),
        dir: rgb(0x9a, 0xd8, 0xf0),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x7d, 0x8a, 0xa0),
        scope: rgb(0xf5, 0xa9, 0xb8),
        wave: rgb(0x5b, 0xce, 0xfa),
        playing: rgb(0xf5, 0xa9, 0xb8),
        bg_sel: rgb(0x1b, 0x26, 0x30),
        effect: &crate::effects::TRANS_SLOW,
    },
    Theme {
        name: "ripple",
        accent: rgb(0x3a, 0xe0, 0xd0),
        accent2: rgb(0x4d, 0xc8, 0xff),
        dir: rgb(0x4d, 0xc8, 0xff),
        media: rgb(0xd8, 0xf0, 0xf5),
        dim: rgb(0x5a, 0x77, 0x82),
        scope: rgb(0x3a, 0xe0, 0xd0),
        wave: rgb(0x4d, 0xc8, 0xff),
        playing: rgb(0x8a, 0xf0, 0xe0),
        bg_sel: rgb(0x12, 0x22, 0x26),
        effect: &crate::effects::RIPPLE,
    },
    Theme {
        name: "snow",
        accent: rgb(0xcf, 0xe4, 0xff),
        accent2: rgb(0x9e, 0xc4, 0xe8),
        dir: rgb(0x8f, 0xb6, 0xde),
        media: rgb(0xe8, 0xf2, 0xff),
        dim: rgb(0x5c, 0x6b, 0x80),
        scope: rgb(0xbf, 0xe0, 0xff),
        wave: rgb(0x7d, 0x9c, 0xc4),
        playing: rgb(0xe8, 0xf2, 0xff),
        bg_sel: rgb(0x16, 0x1d, 0x2c),
        effect: &crate::effects::SNOW,
    },
    Theme {
        name: "cmyk",
        accent: rgb(0x00, 0xe5, 0xe5),
        accent2: rgb(0xe5, 0x00, 0xe5),
        dir: rgb(0xe5, 0xe5, 0x00),
        media: rgb(0xf2, 0xf2, 0xf2),
        dim: rgb(0x6a, 0x6a, 0x6a),
        scope: rgb(0xe5, 0xe5, 0x00),
        wave: rgb(0x00, 0xe5, 0xe5),
        playing: rgb(0xe5, 0x00, 0xe5),
        bg_sel: rgb(0x18, 0x18, 0x1c),
        effect: &crate::effects::CMYK,
    },
    Theme {
        name: "straight",
        accent: rgb(0xe0, 0x3b, 0x4a),
        accent2: rgb(0x3b, 0x5b, 0xdb),
        dir: rgb(0x6f, 0x8c, 0xff),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x8a, 0x8a, 0xa0),
        scope: rgb(0xe0, 0x3b, 0x4a),
        wave: rgb(0x3b, 0x5b, 0xdb),
        playing: rgb(0xff, 0xff, 0xff),
        bg_sel: rgb(0x16, 0x1c, 0x38),
        effect: &crate::effects::FLAG,
    },
    Theme {
        name: "hacker",
        accent: rgb(0x33, 0xff, 0x66),
        accent2: rgb(0x00, 0xcc, 0x44),
        dir: rgb(0x2a, 0xd1, 0x4f),
        media: rgb(0xc8, 0xff, 0xce),
        dim: rgb(0x2f, 0x7d, 0x44),
        scope: rgb(0x33, 0xff, 0x66),
        wave: rgb(0x1f, 0x9d, 0x3a),
        playing: rgb(0xaa, 0xff, 0x66),
        bg_sel: rgb(0x06, 0x20, 0x0c),
        effect: &crate::effects::NONE,
    },
    Theme {
        name: "not gay",
        accent: rgb(0x39, 0xff, 0x14),
        accent2: rgb(0x00, 0xe6, 0x76),
        dir: rgb(0x2a, 0xd1, 0x4f),
        media: rgb(0xb8, 0xff, 0xc0),
        dim: rgb(0x2f, 0x7d, 0x44),
        scope: rgb(0x39, 0xff, 0x14),
        wave: rgb(0x1f, 0x9d, 0x3a),
        playing: rgb(0xaa, 0xff, 0x33),
        bg_sel: rgb(0x06, 0x20, 0x0c),
        effect: &crate::effects::FLAME,
    },
    Theme {
        name: "glitch",
        accent: rgb(0x33, 0xff, 0x66),
        accent2: rgb(0x00, 0xff, 0xcc),
        dir: rgb(0x2a, 0xd1, 0x4f),
        media: rgb(0xb9, 0xff, 0xc8),
        dim: rgb(0x2f, 0x7d, 0x44),
        scope: rgb(0x33, 0xff, 0x88),
        wave: rgb(0x1f, 0x9d, 0x3a),
        playing: rgb(0x66, 0xff, 0xaa),
        bg_sel: rgb(0x05, 0x1a, 0x10),
        effect: &crate::effects::GLITCH,
    },
    Theme {
        name: "electric",
        accent: rgb(0x00, 0xea, 0xff),
        accent2: rgb(0x7d, 0xf9, 0xff),
        dir: rgb(0x4d, 0xc8, 0xff),
        media: rgb(0xd6, 0xf6, 0xff),
        dim: rgb(0x3a, 0x5a, 0x78),
        scope: rgb(0x00, 0xea, 0xff),
        wave: rgb(0x5a, 0x9a, 0xff),
        playing: rgb(0xbf, 0xe9, 0xff),
        bg_sel: rgb(0x0a, 0x10, 0x24),
        effect: &crate::effects::ELECTRIC,
    },
    Theme {
        name: "matrix",
        accent: rgb(0x00, 0xff, 0x41),
        accent2: rgb(0x00, 0xc8, 0x33),
        dir: rgb(0x1f, 0xd0, 0x4a),
        media: rgb(0x9d, 0xff, 0xae),
        dim: rgb(0x1a, 0x66, 0x33),
        scope: rgb(0x00, 0xff, 0x41),
        wave: rgb(0x12, 0x9a, 0x3a),
        playing: rgb(0xb6, 0xff, 0x6a),
        bg_sel: rgb(0x03, 0x16, 0x09),
        effect: &crate::effects::MATRIX,
    },
    Theme {
        name: "aqua",
        accent: rgb(0x46, 0xe0, 0xff),
        accent2: rgb(0x6a, 0xf0, 0xd8),
        dir: rgb(0x52, 0xc8, 0xff),
        media: rgb(0xcf, 0xf3, 0xff),
        dim: rgb(0x35, 0x6a, 0x82),
        scope: rgb(0x46, 0xe0, 0xff),
        wave: rgb(0x2f, 0x9a, 0xc8),
        playing: rgb(0x9a, 0xf0, 0xff),
        bg_sel: rgb(0x06, 0x18, 0x24),
        effect: &crate::effects::BUBBLES,
    },
    Theme {
        name: "cosmic",
        accent: rgb(0xb0, 0xa8, 0xff),
        accent2: rgb(0xff, 0x9d, 0xe0),
        dir: rgb(0x8a, 0x9a, 0xff),
        media: rgb(0xe6, 0xe2, 0xff),
        dim: rgb(0x55, 0x52, 0x80),
        scope: rgb(0xc8, 0xb8, 0xff),
        wave: rgb(0x6a, 0x6a, 0xc8),
        playing: rgb(0xff, 0xd0, 0xf0),
        bg_sel: rgb(0x0c, 0x0a, 0x1e),
        effect: &crate::effects::STARFIELD,
    },
    Theme {
        name: "sakura",
        accent: rgb(0xff, 0x9e, 0xcf),
        accent2: rgb(0xff, 0xc4, 0xe0),
        dir: rgb(0xe8, 0x9a, 0xc8),
        media: rgb(0xff, 0xe6, 0xf2),
        dim: rgb(0x8a, 0x66, 0x7a),
        scope: rgb(0xff, 0x9e, 0xcf),
        wave: rgb(0xd8, 0x7d, 0xb0),
        playing: rgb(0xff, 0xc4, 0xe0),
        bg_sel: rgb(0x22, 0x12, 0x1c),
        effect: &crate::effects::SAKURA,
    },
    Theme {
        name: "rave",
        accent: rgb(0xff, 0x2d, 0xd4),
        accent2: rgb(0x2d, 0xff, 0xe6),
        dir: rgb(0xff, 0xe0, 0x2d),
        media: rgb(0xff, 0xff, 0xff),
        dim: rgb(0x70, 0x50, 0x90),
        scope: rgb(0x2d, 0xff, 0xe6),
        wave: rgb(0xff, 0x2d, 0xd4),
        playing: rgb(0xff, 0xe0, 0x2d),
        bg_sel: rgb(0x14, 0x06, 0x1e),
        effect: &crate::effects::RAVE,
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
    Spectrum,
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
    ScopePreset {
        name: "spectrum",
        style: ScopeStyle::Spectrum,
        mode: ScopeMode::Mono,
        window: 1024,
        auto_gain: false,
    },
];
