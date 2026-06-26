//! Shared color math used by panel borders and theme effects: hue wheel,
//! gradient interpolation, and a brightness pulse. No state, no allocation.

use ratatui::style::Color;

/// Map a phase (any real; wraps at 1.0) to a fully-saturated rainbow color.
pub fn hue(phase: f64) -> Color {
    let h = phase.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn rgb_of(c: Color) -> (f64, f64, f64) {
    if let Color::Rgb(r, g, b) = c {
        (r as f64, g as f64, b as f64)
    } else {
        (200.0, 200.0, 200.0)
    }
}

/// Interpolate around a looping list of color stops at `phase` (wraps at 1.0).
pub fn gradient(stops: &[(f64, f64, f64)], phase: f64) -> Color {
    let n = stops.len();
    let p = phase.rem_euclid(1.0) * n as f64;
    let i = p.floor() as usize % n;
    let frac = p - p.floor();
    let a = stops[i];
    let b = stops[(i + 1) % n];
    Color::Rgb(
        (a.0 + (b.0 - a.0) * frac) as u8,
        (a.1 + (b.1 - a.1) * frac) as u8,
        (a.2 + (b.2 - a.2) * frac) as u8,
    )
}

/// Pulse `base`'s brightness; `offset` phase-shifts so the pulse ripples panel
/// to panel.
pub fn glow(base: Color, frame: u64, offset: f64) -> Color {
    let (r, g, b) = rgb_of(base);
    let t = frame as f64 * 0.025 - offset * 1.4;
    let f = 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f64::consts::TAU).sin());
    Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
}

/// Scale a color's channels by `f` (a flicker/dim factor).
pub fn scale(base: Color, f: f64) -> Color {
    let (r, g, b) = rgb_of(base);
    Color::Rgb((r * f) as u8, (g * f) as u8, (b * f) as u8)
}

/// Linear blend from `a` to `b` at `t` (clamped to 0..1).
pub fn mix(a: Color, b: Color, t: f64) -> Color {
    let (ar, ag, ab) = rgb_of(a);
    let (br, bg, bb) = rgb_of(b);
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (ar + (br - ar) * t) as u8,
        (ag + (bg - ag) * t) as u8,
        (ab + (bb - ab) * t) as u8,
    )
}

pub const TRANS_STOPS: &[(f64, f64, f64)] = &[
    (91.0, 206.0, 250.0),
    (245.0, 169.0, 184.0),
    (255.0, 255.0, 255.0),
    (245.0, 169.0, 184.0),
];

pub const CMY_STOPS: &[(f64, f64, f64)] = &[
    (0.0, 229.0, 229.0),
    (229.0, 0.0, 229.0),
    (229.0, 229.0, 0.0),
];
