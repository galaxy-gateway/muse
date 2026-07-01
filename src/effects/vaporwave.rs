//! Vaporwave: the 80s-dusk palette with an animated neon border, but no screen
//! overlay — a clean, readable theme (the sun/grid backdrop was removed). Borders
//! drift pink↔cyan↔purple and flare on the beat.

use ratatui::style::Color;

use super::{Knob, ThemeEffect, Tuning};
use crate::color::mix;

const PINK: Color = Color::Rgb(0xff, 0x6a, 0xd5);
const PURPLE: Color = Color::Rgb(0x8a, 0x2b, 0xe2);
const CYAN: Color = Color::Rgb(0x2d, 0xe6, 0xff);

pub struct Vaporwave;

impl ThemeEffect for Vaporwave {
    fn knobs(&self) -> &'static [Knob] {
        &[Knob::BeatSync]
    }

    fn default_tuning(&self) -> Tuning {
        Tuning {
            beat_sync: 0.5,
            ..Default::default()
        }
    }

    fn border(&self, _base: Color, frame: u64, offset: f64, beat: f32) -> Color {
        let t = ((frame as f64 * 0.02 + offset).sin() * 0.5 + 0.5 + beat as f64 * 0.3).min(1.0);
        mix(PURPLE, if beat > 0.5 { CYAN } else { PINK }, t)
    }
}
