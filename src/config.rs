//! Theme + (future) config. Colors live here so the "elegant" polish is one place.

use ratatui::style::Color;

pub struct Theme {
    pub accent: Color,
    pub accent2: Color,
    pub dir: Color,
    pub media: Color,
    pub dim: Color,
    pub scope: Color,
    pub wave: Color,
    pub playing: Color,
    pub bg_sel: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Rgb(0x8b, 0xe9, 0xfd),  // cyan
            accent2: Color::Rgb(0xbd, 0x93, 0xf9), // purple
            dir: Color::Rgb(0x82, 0xaa, 0xff),
            media: Color::Rgb(0xc8, 0xd3, 0xf5),
            dim: Color::Rgb(0x6c, 0x73, 0x94),
            scope: Color::Rgb(0x50, 0xfa, 0x7b),  // green
            wave: Color::Rgb(0x6a, 0x84, 0xc4),
            playing: Color::Rgb(0xff, 0xb8, 0x6c), // amber
            bg_sel: Color::Rgb(0x2a, 0x2e, 0x42),
        }
    }
}
