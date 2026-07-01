//! Particle simulation shared by animated theme effects. Owns the live `Spark`
//! pool and the primitive spawn/integrate operations; the *which-spawn-when*
//! policy lives per-theme in `effects`.

use ratatui::layout::Rect;

use crate::util::noise;

/// One particle (flame ember / electric spark / petal / ...): float position +
/// velocity, integrated each tick until it ages out or leaves the screen.
pub struct Spark {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub age: u16,
    pub life: u16,
    pub seed: u32,
}

/// Hard cap on live particles, to bound work on busy themes (rave/matrix).
const MAX: usize = 700;

#[derive(Default)]
pub struct ParticleSim {
    pub sparks: Vec<Spark>,
}

impl ParticleSim {
    pub fn new() -> Self {
        Self { sparks: Vec::new() }
    }

    pub fn push(&mut self, s: Spark) {
        self.sparks.push(s);
    }

    /// Drop the oldest particles once the pool exceeds `MAX`.
    pub fn cap(&mut self) {
        if self.sparks.len() > MAX {
            let drop = self.sparks.len() - MAX;
            self.sparks.drain(0..drop);
        }
    }

    /// Generic radial burst at a point.
    pub fn burst(&mut self, frame: u64, cx: u16, cy: u16, count: u32, spd: f32, life: u16) {
        for i in 0..count {
            let seed = noise(frame as u32 + i, (cx as u32) * 131 + cy as u32);
            let ang = (i as f32 / count.max(1) as f32) * std::f32::consts::TAU
                + (seed % 100) as f32 * 0.02;
            let s = spd * (0.4 + (seed % 6) as f32 * 0.13);
            self.sparks.push(Spark {
                x: cx as f32,
                y: cy as f32,
                vx: ang.cos() * s,
                vy: ang.sin() * s,
                age: 0,
                life,
                seed,
            });
        }
        self.cap();
    }

    /// Warp burst of particles radiating from the screen center.
    pub fn warp(&mut self, frame: u64, screen: Rect, count: u32) {
        let (cx, cy) = (
            (screen.x + screen.width / 2) as f32,
            (screen.y + screen.height / 2) as f32,
        );
        for i in 0..count {
            let seed = noise(frame as u32 + i, 0x5747);
            let ang = (seed % 360) as f32 * 0.0174;
            let spd = 0.5 + (seed % 8) as f32 * 0.12;
            self.sparks.push(Spark {
                x: cx,
                y: cy,
                vx: ang.cos() * spd,
                vy: ang.sin() * spd,
                age: 0,
                life: 70,
                seed,
            });
        }
        self.cap();
    }

    /// Integrate every particle one tick; cull when aged or off-screen. `wind`
    /// (a target column) nudges horizontal velocity — used by the Sakura gust.
    pub fn update(&mut self, screen: Rect, wind: Option<f32>) {
        if self.sparks.is_empty() {
            return;
        }
        let (w, h) = (screen.width as f32, screen.height as f32);
        self.sparks.retain_mut(|p| {
            if let Some(hx) = wind {
                p.vx = (p.vx + (hx - p.x).signum() * 0.03).clamp(-0.6, 0.6);
            }
            p.x += p.vx;
            p.y += p.vy;
            p.age += 1;
            p.x >= 0.0 && p.x < w && p.y >= 0.0 && p.y < h && p.age < p.life
        });
    }

    /// Whether the particle pool is empty.
    pub fn is_empty(&self) -> bool {
        self.sparks.is_empty()
    }
}
