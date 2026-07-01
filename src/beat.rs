//! Lightweight beat / energy tracker that drives beat-reactive theme effects
//! (currently the Glitch theme). It watches the raw scope window every tick and
//! derives two cheap signals with no FFT:
//!
//! * `pulse` — a fast onset spike (0..1) that jumps when energy leaps above its
//!   running baseline and decays over a few frames, i.e. "the beat".
//!
//! When nothing is playing the pulse bleeds to zero, so effects gated on it go
//! quiet on silence for free.
pub struct BeatState {
    /// Slow running baseline of instantaneous energy (the noise floor to beat).
    avg: f32,
    /// Onset pulse, 0..1: spikes on a beat, decays each tick.
    pulse: f32,
}

impl BeatState {
    pub fn new() -> Self {
        Self {
            avg: 0.0,
            pulse: 0.0,
        }
    }

    /// Feed one interleaved-stereo scope window. `playing` gates the analysis:
    /// while paused/stopped everything decays toward silence.
    pub fn update(&mut self, scope: &[f32], playing: bool) {
        // Decay the pulse first so a fresh onset this tick lands at full height.
        self.pulse *= 0.80;

        if !playing || scope.is_empty() {
            self.avg *= 0.95;
            return;
        }

        // RMS of the window as the energy estimate. Music RMS sits ~0.03..0.4,
        // so scale up and clamp to roughly fill 0..1.
        let sum: f32 = scope.iter().map(|&s| s * s).sum();
        let rms = (sum / scope.len() as f32).sqrt();
        let inst = (rms * 3.2).min(1.0);

        // Onset detection: energy jumping well above its slow baseline is a beat.
        let thresh = self.avg * 1.25 + 0.02;
        if inst > thresh {
            let hit = ((inst - self.avg) / self.avg.max(0.05)).min(1.5) / 1.5;
            self.pulse = self.pulse.max(hit);
        }

        // Update the baseline last, slowly, so a loud hit doesn't instantly hide
        // the next one.
        self.avg = self.avg * 0.98 + inst * 0.02;
    }

    /// Onset pulse, 0..1 — full on a fresh beat, decaying over a few frames.
    pub fn pulse(&self) -> f32 {
        self.pulse
    }
}
