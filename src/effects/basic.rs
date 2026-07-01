//! Static and border-only effects: no particles, no overlay.
//!
//! The animated border themes pulse to the beat: `beat` arrives already scaled
//! by the theme's `BeatSync` knob (via `App::beat_pulse`), so it is the only knob
//! they need — `BeatSync = 0` ⇒ a steady, non-reactive border.

use ratatui::style::Color;

use super::{Knob, ThemeEffect, Tuning};
use crate::color::{CMY_STOPS, TRANS_STOPS, glow, gradient, hue, scale};

/// Beat-driven brightness envelope: dips slightly between beats, flares on a hit.
fn beat_bright(c: Color, beat: f32, base: f64, amp: f64) -> Color {
    scale(c, base + amp * beat.clamp(0.0, 1.0) as f64)
}

/// Every animated border theme exposes just the beat-sync knob.
fn beat_knobs() -> &'static [Knob] {
    &[Knob::BeatSync]
}

/// Fully static theme: no animation, no particles, no overlay.
pub struct Static;
impl ThemeEffect for Static {
    fn is_animated(&self) -> bool {
        false
    }
}

/// Borders shift through the full rainbow, jumping + flaring on the beat.
pub struct Prismatic;
impl ThemeEffect for Prismatic {
    fn knobs(&self) -> &'static [Knob] {
        beat_knobs()
    }
    fn default_tuning(&self) -> Tuning {
        Tuning {
            beat_sync: 0.5,
            ..Default::default()
        }
    }
    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let c = hue(frame as f64 * 0.012 + offset + beat as f64 * 0.12);
        beat_bright(c, beat, 0.8, 0.2)
    }
}

/// Borders drift slowly through the trans-flag palette, brightening on the beat.
pub struct TransSlow;
impl ThemeEffect for TransSlow {
    fn knobs(&self) -> &'static [Knob] {
        beat_knobs()
    }
    fn default_tuning(&self) -> Tuning {
        Tuning {
            beat_sync: 0.3,
            ..Default::default()
        }
    }
    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let c = gradient(TRANS_STOPS, frame as f64 * 0.0035 + offset * 0.25);
        beat_bright(c, beat, 0.78, 0.22)
    }
}

/// Borders pulse/glow, the pulse rippling across panels — kicked by the beat.
pub struct Ripple;
impl ThemeEffect for Ripple {
    fn knobs(&self) -> &'static [Knob] {
        beat_knobs()
    }
    fn default_tuning(&self) -> Tuning {
        Tuning {
            beat_sync: 0.6,
            ..Default::default()
        }
    }
    fn border(&self, base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        beat_bright(glow(base, frame, offset), beat, 0.75, 0.25)
    }
}

/// Vivid CMY oscillation across borders, with a saturation flare on the beat.
pub struct Cmyk;
impl ThemeEffect for Cmyk {
    fn knobs(&self) -> &'static [Knob] {
        beat_knobs()
    }
    fn default_tuning(&self) -> Tuning {
        Tuning {
            beat_sync: 0.5,
            ..Default::default()
        }
    }
    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let c = gradient(CMY_STOPS, frame as f64 * 0.006 + offset + beat as f64 * 0.1);
        beat_bright(c, beat, 0.8, 0.2)
    }
}
