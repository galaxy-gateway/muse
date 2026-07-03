//! Theme + (future) config. Colors live here so the "elegant" polish is one place.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::effects::{ThemeEffect, Tuning};

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
        name: "vaporwave",
        accent: rgb(0xff, 0x6a, 0xd5),
        accent2: rgb(0x2d, 0xe6, 0xff),
        dir: rgb(0x9a, 0x6c, 0xff),
        media: rgb(0xff, 0xe6, 0xf7),
        dim: rgb(0x6a, 0x50, 0x80),
        scope: rgb(0x2d, 0xe6, 0xff),
        wave: rgb(0xff, 0x6a, 0xd5),
        playing: rgb(0xff, 0xd0, 0x66),
        bg_sel: rgb(0x1a, 0x08, 0x28),
        effect: &crate::effects::VAPORWAVE,
    },
    Theme {
        name: "america",
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
        name: "monster",
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
        name: "datamosh",
        accent: rgb(0x00, 0xff, 0xcc),
        accent2: rgb(0xff, 0x00, 0xcc),
        dir: rgb(0x2a, 0xd1, 0x4f),
        media: rgb(0xb9, 0xff, 0xc8),
        dim: rgb(0x2f, 0x7d, 0x44),
        scope: rgb(0x33, 0xff, 0x88),
        wave: rgb(0x1f, 0x9d, 0x3a),
        playing: rgb(0x66, 0xff, 0xaa),
        bg_sel: rgb(0x05, 0x1a, 0x10),
        effect: &crate::effects::DATAMOSH,
    },
    Theme {
        name: "meltdown",
        accent: rgb(0xff, 0x00, 0xcc),
        accent2: rgb(0x00, 0xff, 0xcc),
        dir: rgb(0x2a, 0xd1, 0x4f),
        media: rgb(0xb9, 0xff, 0xc8),
        dim: rgb(0x2f, 0x7d, 0x44),
        scope: rgb(0x33, 0xff, 0x88),
        wave: rgb(0x1f, 0x9d, 0x3a),
        playing: rgb(0x66, 0xff, 0xaa),
        bg_sel: rgb(0x05, 0x1a, 0x10),
        effect: &crate::effects::MELTDOWN,
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
    Theme {
        name: "plasma",
        accent: rgb(0xff, 0x3d, 0xd6),
        accent2: rgb(0x3d, 0xe6, 0xff),
        dir: rgb(0xb0, 0x5c, 0xff),
        media: rgb(0xf0, 0xe6, 0xff),
        dim: rgb(0x6a, 0x50, 0x80),
        scope: rgb(0xff, 0x3d, 0xd6),
        wave: rgb(0x9a, 0x4d, 0xff),
        playing: rgb(0x3d, 0xe6, 0xff),
        bg_sel: rgb(0x16, 0x0a, 0x22),
        effect: &crate::effects::PLASMA,
    },
    Theme {
        name: "aurora",
        accent: rgb(0x3d, 0xff, 0xa6),
        accent2: rgb(0x9a, 0x4d, 0xff),
        dir: rgb(0x2d, 0xe6, 0xff),
        media: rgb(0xe6, 0xff, 0xf2),
        dim: rgb(0x50, 0x70, 0x68),
        scope: rgb(0x3d, 0xff, 0xa6),
        wave: rgb(0x2d, 0xc0, 0xd0),
        playing: rgb(0x9a, 0x4d, 0xff),
        bg_sel: rgb(0x08, 0x18, 0x14),
        effect: &crate::effects::AURORA,
    },
    Theme {
        name: "blackhole",
        accent: rgb(0x9a, 0x5c, 0xff),
        accent2: rgb(0xff, 0xc4, 0x6a),
        dir: rgb(0x6a, 0x8c, 0xff),
        media: rgb(0xe6, 0xe0, 0xf5),
        dim: rgb(0x55, 0x48, 0x6a),
        scope: rgb(0xff, 0xc4, 0x6a),
        wave: rgb(0x7a, 0x3d, 0xd6),
        playing: rgb(0xff, 0xd8, 0x9a),
        bg_sel: rgb(0x10, 0x06, 0x18),
        effect: &crate::effects::BLACKHOLE,
    },
    Theme {
        name: "fireplace",
        accent: rgb(0xff, 0x8a, 0x2b),
        accent2: rgb(0xff, 0xd8, 0x6a),
        dir: rgb(0xe0, 0x6a, 0x30),
        media: rgb(0xff, 0xe8, 0xd0),
        dim: rgb(0x8a, 0x5a, 0x40),
        scope: rgb(0xff, 0x8a, 0x2b),
        wave: rgb(0xc4, 0x3a, 0x1e),
        playing: rgb(0xff, 0xd8, 0x6a),
        bg_sel: rgb(0x22, 0x10, 0x08),
        effect: &crate::effects::FIREPLACE,
    },
    Theme {
        name: "thunderstorm",
        accent: rgb(0x8a, 0xa6, 0xc8),
        accent2: rgb(0xf0, 0xf4, 0xff),
        dir: rgb(0x6a, 0x84, 0xa8),
        media: rgb(0xdc, 0xe4, 0xf0),
        dim: rgb(0x4a, 0x5e, 0x78),
        scope: rgb(0x8a, 0xa6, 0xc8),
        wave: rgb(0x4a, 0x5e, 0x78),
        playing: rgb(0xf0, 0xf4, 0xff),
        bg_sel: rgb(0x10, 0x16, 0x20),
        effect: &crate::effects::THUNDERSTORM,
    },
    Theme {
        name: "circuit",
        accent: rgb(0x2d, 0xff, 0xd0),
        accent2: rgb(0x2d, 0xa6, 0xff),
        dir: rgb(0x1f, 0xc0, 0xa0),
        media: rgb(0xd0, 0xff, 0xf4),
        dim: rgb(0x2a, 0x60, 0x58),
        scope: rgb(0x2d, 0xff, 0xd0),
        wave: rgb(0x2d, 0xa6, 0xff),
        playing: rgb(0xe6, 0xff, 0xff),
        bg_sel: rgb(0x04, 0x1a, 0x16),
        effect: &crate::effects::CIRCUIT,
    },
    Theme {
        name: "constellations",
        accent: rgb(0xcf, 0xd6, 0xff),
        accent2: rgb(0x6a, 0x8c, 0xd0),
        dir: rgb(0x9a, 0xa6, 0xe0),
        media: rgb(0xe8, 0xec, 0xff),
        dim: rgb(0x4a, 0x54, 0x78),
        scope: rgb(0xcf, 0xd6, 0xff),
        wave: rgb(0x6a, 0x8c, 0xd0),
        playing: rgb(0xff, 0xff, 0xff),
        bg_sel: rgb(0x0a, 0x0e, 0x1e),
        effect: &crate::effects::CONSTELLATIONS,
    },
    Theme {
        name: "crt",
        accent: rgb(0xff, 0xb0, 0x00),
        accent2: rgb(0xff, 0xd7, 0x66),
        dir: rgb(0xd8, 0x92, 0x1a),
        media: rgb(0xff, 0xe4, 0xb0),
        dim: rgb(0x7a, 0x5c, 0x20),
        scope: rgb(0xff, 0xb0, 0x00),
        wave: rgb(0xa8, 0x78, 0x18),
        playing: rgb(0xff, 0xe1, 0x4a),
        bg_sel: rgb(0x20, 0x14, 0x00),
        effect: &crate::effects::CRT,
    },
    Theme {
        name: "tape deck",
        accent: rgb(0xc8, 0x8a, 0x4a),
        accent2: rgb(0xe8, 0xd0, 0xa8),
        dir: rgb(0xa8, 0x74, 0x40),
        media: rgb(0xe8, 0xdc, 0xc8),
        dim: rgb(0x6a, 0x54, 0x3a),
        scope: rgb(0xd8, 0xa8, 0x60),
        wave: rgb(0x8a, 0x64, 0x38),
        playing: rgb(0xff, 0xd9, 0x8a),
        bg_sel: rgb(0x1e, 0x14, 0x0a),
        effect: &crate::effects::TAPE,
    },
    Theme {
        name: "seismograph",
        accent: rgb(0xc8, 0xbc, 0xa4),
        accent2: rgb(0xd8, 0x3a, 0x2a),
        dir: rgb(0xa8, 0x9c, 0x84),
        media: rgb(0xe0, 0xd8, 0xc4),
        dim: rgb(0x5e, 0x58, 0x4a),
        scope: rgb(0xd8, 0x3a, 0x2a),
        wave: rgb(0x8a, 0x80, 0x68),
        playing: rgb(0xf0, 0xe6, 0xd0),
        bg_sel: rgb(0x1a, 0x18, 0x12),
        effect: &crate::effects::SEISMOGRAPH,
    },
    Theme {
        name: "metronome",
        accent: rgb(0xe8, 0xc8, 0x7a),
        accent2: rgb(0xb0, 0x98, 0x6a),
        dir: rgb(0xc0, 0xa8, 0x78),
        media: rgb(0xe6, 0xde, 0xcc),
        dim: rgb(0x6a, 0x5c, 0x40),
        scope: rgb(0xe8, 0xc8, 0x7a),
        wave: rgb(0x90, 0x7c, 0x54),
        playing: rgb(0xff, 0xf2, 0xc8),
        bg_sel: rgb(0x18, 0x14, 0x0c),
        effect: &crate::effects::METRONOME,
    },
    Theme {
        name: "tide",
        accent: rgb(0x7a, 0xd8, 0xe8),
        accent2: rgb(0x3a, 0x74, 0xa0),
        dir: rgb(0x5a, 0xa8, 0xc0),
        media: rgb(0xd8, 0xf0, 0xf4),
        dim: rgb(0x3e, 0x5c, 0x6e),
        scope: rgb(0x7a, 0xd8, 0xe8),
        wave: rgb(0x4a, 0x8a, 0xa8),
        playing: rgb(0xe8, 0xff, 0xff),
        bg_sel: rgb(0x08, 0x18, 0x22),
        effect: &crate::effects::TIDE,
    },
    Theme {
        name: "fireflies",
        accent: rgb(0xd8, 0xf0, 0x6a),
        accent2: rgb(0x8a, 0xa0, 0x3a),
        dir: rgb(0x9a, 0xb0, 0x60),
        media: rgb(0xdc, 0xe4, 0xc8),
        dim: rgb(0x52, 0x5e, 0x3e),
        scope: rgb(0xd8, 0xf0, 0x6a),
        wave: rgb(0x6e, 0x80, 0x40),
        playing: rgb(0xff, 0xff, 0xc0),
        bg_sel: rgb(0x12, 0x16, 0x08),
        effect: &crate::effects::FIREFLIES,
    },
    Theme {
        name: "radar",
        accent: rgb(0x39, 0xff, 0x8a),
        accent2: rgb(0x1a, 0x66, 0x40),
        dir: rgb(0x2a, 0xc0, 0x6a),
        media: rgb(0xc8, 0xf0, 0xd8),
        dim: rgb(0x2a, 0x54, 0x3c),
        scope: rgb(0x39, 0xff, 0x8a),
        wave: rgb(0x28, 0x8a, 0x52),
        playing: rgb(0xd8, 0xff, 0xe8),
        bg_sel: rgb(0x04, 0x16, 0x0c),
        effect: &crate::effects::RADAR,
    },
    Theme {
        name: "neon sign",
        accent: rgb(0xff, 0x5a, 0xd8),
        accent2: rgb(0x5a, 0xd8, 0xff),
        dir: rgb(0xc0, 0x6a, 0xd8),
        media: rgb(0xf0, 0xd8, 0xec),
        dim: rgb(0x6e, 0x44, 0x66),
        scope: rgb(0x5a, 0xd8, 0xff),
        wave: rgb(0x9a, 0x4a, 0x8a),
        playing: rgb(0xff, 0xd8, 0xf4),
        bg_sel: rgb(0x1c, 0x08, 0x18),
        effect: &crate::effects::NEON,
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

/// Inspector middle-panel views, cycled with `W`. The list grows as views
/// land; persisted by name so reordering never breaks saved settings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WaveView {
    Waveform,
    Spectrogram,
    Bands,
    Bitrate,
    Dynamics,
    BeatMap,
    UpNext,
    ArtCard,
    Lyrics,
}

/// Cycle order + display names for the wave-pane views.
pub const WAVE_VIEWS: &[(WaveView, &str)] = &[
    (WaveView::Waveform, "waveform"),
    (WaveView::Spectrogram, "spectrogram"),
    (WaveView::Bands, "bands"),
    (WaveView::Bitrate, "bitrate"),
    (WaveView::Dynamics, "dynamics"),
    (WaveView::BeatMap, "beat map"),
    (WaveView::UpNext, "up next"),
    (WaveView::ArtCard, "art"),
    (WaveView::Lyrics, "lyrics"),
];

/// Persisted user settings (TOML at the platform config dir).
#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    /// Name of the last-used scope preset, restored on next launch.
    pub scope_preset: Option<String>,
    /// Name of the last-used color theme.
    pub theme: Option<String>,
    /// Retint the active theme from the now-playing cover art. Defaults off.
    pub adaptive_palette: Option<bool>,
    /// Name of the last-used inspector wave-pane view.
    pub wave_view: Option<String>,
    /// Master effect level on top of per-theme tunings (0.5 = neutral).
    pub master_intensity: Option<f32>,
    /// Accessibility: force the static effect while keeping any palette.
    pub no_motion: Option<bool>,
    /// Gapless cross-track playback (prefetch + sample-accurate splice).
    /// `None` (absent in the file) defaults to on.
    pub gapless: Option<bool>,
    /// Shuffle mode (no-repeat bag). `None` defaults to off.
    pub shuffle: Option<bool>,

    // --- last session: restored on next launch (playback resumes paused) ---
    /// Absolute path of the track that was playing at exit.
    pub session_track: Option<String>,
    /// Playhead position (seconds) of that track at exit.
    pub session_pos: Option<f64>,
    /// Output volume (0.0..1.0) at exit.
    pub session_volume: Option<f32>,
    /// Loop mode at exit ("off" / "all" / "one").
    pub session_loop: Option<String>,
    /// Absolute path of the tree-cursor selection at exit.
    pub session_cursor: Option<String>,

    /// Per-theme knob overrides, keyed by theme name. Only configurable themes
    /// (glitch family) appear here; absent knobs fall back to the effect default.
    pub theme_tuning: Option<HashMap<String, TuningCfg>>,
}

/// Serialized knob overrides for one theme. Each field is optional so partially
/// tuned themes still round-trip and new knobs stay backward-compatible.
#[derive(Serialize, Deserialize, Clone, Copy, Default)]
pub struct TuningCfg {
    pub intensity: Option<f32>,
    pub speed: Option<f32>,
    pub density: Option<f32>,
    pub beat_sync: Option<f32>,
    pub wind: Option<f32>,
    pub persistence: Option<f32>,
    pub disruption: Option<f32>,
    pub strobe: Option<f32>,
    pub follow_mouse: Option<f32>,
    pub beat_source: Option<f32>,
    pub palette: Option<f32>,
}

impl TuningCfg {
    /// Overlay any set fields onto a live `Tuning`.
    pub fn apply(&self, t: &mut Tuning) {
        if let Some(v) = self.intensity {
            t.intensity = v;
        }
        if let Some(v) = self.speed {
            t.speed = v;
        }
        if let Some(v) = self.density {
            t.density = v;
        }
        if let Some(v) = self.beat_sync {
            t.beat_sync = v;
        }
        if let Some(v) = self.wind {
            t.wind = v;
        }
        if let Some(v) = self.persistence {
            t.persistence = v;
        }
        if let Some(v) = self.disruption {
            t.disruption = v;
        }
        if let Some(v) = self.strobe {
            t.strobe = v;
        }
        if let Some(v) = self.follow_mouse {
            t.follow_mouse = v;
        }
        if let Some(v) = self.beat_source {
            t.beat_source = v;
        }
        if let Some(v) = self.palette {
            t.palette = v;
        }
    }

    /// Snapshot a live `Tuning` for persistence.
    pub fn from(t: &Tuning) -> Self {
        Self {
            intensity: Some(t.intensity),
            speed: Some(t.speed),
            density: Some(t.density),
            beat_sync: Some(t.beat_sync),
            wind: Some(t.wind),
            persistence: Some(t.persistence),
            disruption: Some(t.disruption),
            strobe: Some(t.strobe),
            follow_mouse: Some(t.follow_mouse),
            beat_source: Some(t.beat_source),
            palette: Some(t.palette),
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.config_dir().join("config.toml"))
}

/// Path of the panic log. Background-thread panics (e.g. a decoder choking on a
/// malformed file) are written here instead of stderr, which would otherwise
/// corrupt the alternate-screen TUI.
pub fn log_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "muse").map(|d| d.data_dir().join("muse.log"))
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

/// An 8-bit RGB anchor pair used to retint the UI for a colorway.
pub type AnchorPair = ((u8, u8, u8), (u8, u8, u8));

/// A theme's colorway set: three names (index 0 is always the theme's own
/// palette) and the UI anchor pairs for variants 1 and 2. Anchors run through
/// the same `retint` machinery as the adaptive palette, so a colorway
/// recolors borders, waveform, and accents coherently. Effects with their own
/// drawn colors (crt, radar, ...) additionally index internal tables with the
/// same knob value.
pub struct Colorways {
    pub names: [&'static str; 3],
    pub anchors: [AnchorPair; 2],
}

/// The colorway set for a theme, if it has one. Every selectable theme should
/// have an entry — static themes get UI-only variants, which is the whole
/// effect for them.
pub fn colorways(theme: &str) -> Option<&'static Colorways> {
    macro_rules! cw {
        ($n0:literal, $n1:literal, $n2:literal, $p1:expr, $s1:expr, $p2:expr, $s2:expr) => {{
            static C: Colorways = Colorways {
                names: [$n0, $n1, $n2],
                anchors: [($p1, $s1), ($p2, $s2)],
            };
            Some(&C)
        }};
    }
    match theme {
        // -- originals: UI-anchor colorways (overlay identity colors stay) --
        "midnight" => cw!(
            "midnight",
            "harbor",
            "dusk",
            (0x4a, 0xd8, 0xc8),
            (0x8a, 0xe8, 0xe0),
            (0xff, 0x9a, 0x6a),
            (0xc8, 0x8a, 0xe8)
        ),
        "prismatic" => cw!(
            "spectrum",
            "warm",
            "mono",
            (0xff, 0xa0, 0x5a),
            (0xff, 0xd0, 0x8a),
            (0xc8, 0xd0, 0xdc),
            (0x9a, 0xa4, 0xb4)
        ),
        "trans flow" => cw!(
            "classic",
            "pastel",
            "bold",
            (0x9a, 0xe0, 0xff),
            (0xff, 0xc8, 0xd8),
            (0x2a, 0xb8, 0xf0),
            (0xf0, 0x7a, 0x9a)
        ),
        "ripple" => cw!(
            "teal",
            "coral",
            "violet",
            (0xff, 0x7a, 0x6a),
            (0xff, 0xb8, 0x9a),
            (0x9a, 0x7a, 0xff),
            (0xc8, 0xb0, 0xff)
        ),
        "snow" => cw!(
            "arctic",
            "alpenglow",
            "polar night",
            (0xff, 0xb8, 0xc8),
            (0xff, 0xe0, 0xd0),
            (0x6a, 0x8a, 0xe8),
            (0xa8, 0xc0, 0xff)
        ),
        "cmyk" => cw!(
            "cmyk",
            "rgb",
            "duotone",
            (0xff, 0x3a, 0x3a),
            (0x3a, 0x8a, 0xff),
            (0xff, 0x00, 0x88),
            (0x00, 0xe8, 0xd0)
        ),
        "vaporwave" => cw!(
            "sunset",
            "miami",
            "laserdisc",
            (0x00, 0xe8, 0xc8),
            (0xff, 0x8a, 0xd8),
            (0xff, 0xd0, 0x66),
            (0x9a, 0x6c, 0xff)
        ),
        "america" => cw!(
            "old glory",
            "vintage",
            "neon",
            (0xc8, 0x6a, 0x5a),
            (0x8a, 0x9a, 0xc8),
            (0xff, 0x4a, 0x6a),
            (0x4a, 0x8a, 0xff)
        ),
        "hacker" => cw!(
            "green",
            "amber",
            "paper",
            (0xff, 0xb0, 0x00),
            (0xff, 0xd7, 0x66),
            (0xe8, 0xf0, 0xe8),
            (0xc0, 0xd0, 0xc0)
        ),
        "monster" => cw!(
            "lime",
            "toxic",
            "blood",
            (0xc8, 0xff, 0x2a),
            (0x8a, 0xe8, 0x00),
            (0xff, 0x3a, 0x3a),
            (0xc0, 0x2a, 0x2a)
        ),
        "glitch" => cw!(
            "green",
            "cyberpunk",
            "vhs",
            (0xff, 0x2a, 0x8a),
            (0x2a, 0xe8, 0xff),
            (0xe8, 0xe0, 0xd0),
            (0x9a, 0x8a, 0xff)
        ),
        "datamosh" => cw!(
            "signalcross",
            "acid",
            "shortwave",
            (0xc8, 0xff, 0x2a),
            (0xff, 0x8a, 0x2a),
            (0x4a, 0x9a, 0xff),
            (0xff, 0x4a, 0x4a)
        ),
        "meltdown" => cw!(
            "magenta",
            "lava",
            "ultraviolet",
            (0xff, 0x6a, 0x2a),
            (0xff, 0xc8, 0x4a),
            (0x8a, 0x4a, 0xff),
            (0xd0, 0x8a, 0xff)
        ),
        "electric" => cw!(
            "cyan",
            "pink",
            "storm",
            (0xff, 0x5a, 0xc8),
            (0xff, 0xa8, 0xe8),
            (0xb8, 0xc8, 0xe0),
            (0x7a, 0x9a, 0xc8)
        ),
        "matrix" => cw!(
            "green",
            "amber term",
            "ghost",
            (0xff, 0xb0, 0x00),
            (0xff, 0xd7, 0x66),
            (0xc8, 0xd8, 0xd0),
            (0x8a, 0xa0, 0x96)
        ),
        "aqua" => cw!(
            "aqua",
            "kelp",
            "abyssal",
            (0x4a, 0xc8, 0x6a),
            (0xa8, 0xe8, 0x9a),
            (0x3a, 0x5a, 0xd8),
            (0x8a, 0xa8, 0xff)
        ),
        "cosmic" => cw!(
            "cosmic",
            "nebula",
            "supernova",
            (0xc8, 0x6a, 0xff),
            (0xff, 0x9a, 0xd8),
            (0xff, 0xc8, 0x4a),
            (0xff, 0x8a, 0x5a)
        ),
        "sakura" => cw!(
            "sakura",
            "plum",
            "snowbloom",
            (0xd8, 0x5a, 0x9a),
            (0xff, 0xa8, 0xc8),
            (0xf0, 0xf0, 0xff),
            (0xd8, 0xc8, 0xe8)
        ),
        "rave" => cw!(
            "club",
            "laser",
            "blacklight",
            (0x2a, 0xff, 0x2a),
            (0xff, 0x2a, 0x2a),
            (0x8a, 0x2a, 0xff),
            (0x2a, 0xd8, 0xff)
        ),
        "plasma" => cw!(
            "plasma",
            "inferno",
            "borealis",
            (0xff, 0x8a, 0x2a),
            (0xff, 0xd0, 0x4a),
            (0x4a, 0xe8, 0xa8),
            (0x8a, 0xd8, 0xff)
        ),
        "aurora" => cw!(
            "aurora",
            "solar",
            "ion",
            (0xff, 0xa0, 0x4a),
            (0xff, 0xe0, 0x8a),
            (0x8a, 0x9a, 0xff),
            (0xd0, 0xa8, 0xff)
        ),
        "blackhole" => cw!(
            "void",
            "accretion",
            "hawking",
            (0xff, 0x9a, 0x3a),
            (0xff, 0xd8, 0x8a),
            (0x9a, 0xe8, 0xff),
            (0xe0, 0xf8, 0xff)
        ),
        "fireplace" => cw!(
            "hearth",
            "embers",
            "blue flame",
            (0xd8, 0x3a, 0x1a),
            (0xff, 0x8a, 0x3a),
            (0x5a, 0x9a, 0xff),
            (0xa8, 0xd0, 0xff)
        ),
        "thunderstorm" => cw!(
            "storm",
            "monsoon",
            "heat",
            (0x4a, 0xc8, 0xb0),
            (0xa8, 0xe8, 0xd8),
            (0xff, 0xb0, 0x5a),
            (0xff, 0xe0, 0xa0)
        ),
        "circuit" => cw!(
            "teal",
            "copper",
            "fiber",
            (0xff, 0x9a, 0x4a),
            (0xd8, 0x6a, 0x2a),
            (0xff, 0x5a, 0xd8),
            (0x5a, 0xd8, 0xff)
        ),
        "constellations" => cw!(
            "starlight",
            "zodiac",
            "dawn",
            (0xd8, 0xb0, 0xff),
            (0xff, 0xd8, 0x8a),
            (0xff, 0xb8, 0x9a),
            (0xff, 0xe8, 0xc8)
        ),
        // -- newer themes: anchors match their effect-internal color tables --
        "crt" => cw!(
            "amber",
            "green",
            "ice",
            (0x33, 0xff, 0x66),
            (0x99, 0xff, 0xbb),
            (0xa8, 0xd8, 0xff),
            (0xe0, 0xf0, 0xff)
        ),
        "radar" => cw!(
            "green",
            "amber",
            "arctic",
            (0xff, 0xb0, 0x30),
            (0xd8, 0x8a, 0x1a),
            (0x8a, 0xd8, 0xff),
            (0xd0, 0xf0, 0xff)
        ),
        "seismograph" => cw!(
            "classic",
            "blueprint",
            "thermal",
            (0x5a, 0xc8, 0xff),
            (0xe8, 0xf0, 0xff),
            (0xff, 0x8a, 0x2a),
            (0xff, 0xd0, 0x4a)
        ),
        "tide" => cw!(
            "ocean",
            "lagoon",
            "abyss",
            (0x6a, 0xe8, 0xb8),
            (0xe0, 0xff, 0xf0),
            (0x8a, 0x7a, 0xe8),
            (0xc8, 0xc0, 0xff)
        ),
        "neon sign" => cw!(
            "magenta",
            "cyan",
            "gold",
            (0x3a, 0xe8, 0xff),
            (0xff, 0x5a, 0xd8),
            (0xff, 0xc8, 0x3a),
            (0xff, 0x8a, 0x5a)
        ),
        "tape deck" => cw!(
            "oxide",
            "chrome",
            "gold",
            (0xa8, 0xb8, 0xc8),
            (0xd8, 0xe4, 0xf0),
            (0xe8, 0xc0, 0x4a),
            (0xff, 0xe0, 0x8a)
        ),
        "fireflies" => cw!(
            "meadow",
            "ember",
            "ghost",
            (0xff, 0xa0, 0x3a),
            (0xff, 0xd8, 0xa0),
            (0xa8, 0xe8, 0xe8),
            (0xf0, 0xff, 0xff)
        ),
        "metronome" => cw!(
            "brass",
            "silver",
            "rosewood",
            (0xd0, 0xd8, 0xe0),
            (0xf0, 0xf4, 0xff),
            (0xd8, 0x8a, 0x6a),
            (0xff, 0xd8, 0xc0)
        ),
        _ => None,
    }
}

/// Anchor pair for colorway `k` (1 or 2) of a theme; `None` keeps the base.
pub fn variant_anchors(theme: &str, k: usize) -> Option<AnchorPair> {
    colorways(theme).and_then(|c| (1..=2).contains(&k).then(|| c.anchors[k - 1]))
}

/// Colorway names for the Palette knob, empty when a theme has none.
pub fn variant_names(theme: &str) -> &'static [&'static str] {
    colorways(theme).map(|c| &c.names[..]).unwrap_or(&[])
}

/// The knob list for a theme: a Palette knob whenever the theme has
/// colorways (even for shared/static effects, whose colorway is purely the
/// UI retint), followed by the effect's own knobs.
pub fn theme_knobs(idx: usize) -> Vec<crate::effects::Knob> {
    let th = &THEMES[idx];
    let mut v = Vec::new();
    if colorways(th.name).is_some() {
        v.push(crate::effects::Knob::Palette);
    }
    for k in th.effect.knobs() {
        if *k != crate::effects::Knob::Palette {
            v.push(*k);
        }
    }
    v
}
