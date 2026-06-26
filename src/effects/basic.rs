//! Static and border-only effects: no particles, no overlay.

use ratatui::style::Color;

use super::ThemeEffect;
use crate::color::{CMY_STOPS, TRANS_STOPS, glow, gradient, hue};

/// Fully static theme: no animation, no particles, no overlay.
pub struct Static;
impl ThemeEffect for Static {
    fn is_animated(&self) -> bool {
        false
    }
}

/// Borders shift through the full rainbow.
pub struct Prismatic;
impl ThemeEffect for Prismatic {
    fn border(&self, _base: Color, frame: u64, offset: f64) -> Color {
        hue(frame as f64 * 0.012 + offset)
    }
}

/// Borders drift slowly through the trans-flag palette.
pub struct TransSlow;
impl ThemeEffect for TransSlow {
    fn border(&self, _base: Color, frame: u64, offset: f64) -> Color {
        gradient(TRANS_STOPS, frame as f64 * 0.0035 + offset * 0.25)
    }
}

/// Borders pulse/glow, the pulse rippling across panels.
pub struct Ripple;
impl ThemeEffect for Ripple {
    fn border(&self, base: Color, frame: u64, offset: f64) -> Color {
        glow(base, frame, offset)
    }
}

/// Vivid CMY oscillation across borders.
pub struct Cmyk;
impl ThemeEffect for Cmyk {
    fn border(&self, _base: Color, frame: u64, offset: f64) -> Color {
        gradient(CMY_STOPS, frame as f64 * 0.006 + offset)
    }
}
