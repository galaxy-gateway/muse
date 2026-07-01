//! Lightweight beat / energy tracker that drives beat-reactive theme effects
//! (the Glitch themes). It watches the raw scope window every tick and derives
//! onset "pulses" with no FFT:
//!
//! * `pulse()` — a full-band onset spike (0..1) that jumps when energy leaps
//!   above its running baseline and decays over a few frames, i.e. "the beat".
//! * `bands()` — the same onset detector run separately on cheap bass / mid /
//!   treble splits of the signal, so an effect can react differently to a kick
//!   than to a hi-hat.
//!
//! When nothing is playing the pulses bleed to zero, so effects gated on them go
//! quiet on silence for free.

/// One onset detector: an energy baseline plus a decaying pulse.
struct Band {
    avg: f32,
    pulse: f32,
}

impl Band {
    fn new() -> Self {
        Self {
            avg: 0.0,
            pulse: 0.0,
        }
    }

    /// Decay the pulse one tick (called before any onset so a fresh hit lands
    /// at full height).
    fn decay(&mut self) {
        self.pulse *= 0.80;
    }

    /// While idle, let the baseline sag toward zero.
    fn idle(&mut self) {
        self.avg *= 0.95;
    }

    /// Feed this tick's instantaneous energy (0..1): fire an onset if it jumps
    /// well above the slow baseline, then nudge the baseline.
    fn feed(&mut self, inst: f32) {
        let thresh = self.avg * 1.25 + 0.02;
        if inst > thresh {
            let hit = ((inst - self.avg) / self.avg.max(0.05)).min(1.5) / 1.5;
            self.pulse = self.pulse.max(hit);
        }
        self.avg = self.avg * 0.98 + inst * 0.02;
    }
}

pub struct BeatState {
    full: Band,
    bass: Band,
    mid: Band,
    treble: Band,
    /// One-pole low-pass states used to split the signal into bands.
    lp_bass: f32,
    lp_mid: f32,
}

impl BeatState {
    pub fn new() -> Self {
        Self {
            full: Band::new(),
            bass: Band::new(),
            mid: Band::new(),
            treble: Band::new(),
            lp_bass: 0.0,
            lp_mid: 0.0,
        }
    }

    /// Feed one interleaved-stereo scope window. `playing` gates the analysis:
    /// while paused/stopped everything decays toward silence.
    pub fn update(&mut self, scope: &[f32], playing: bool) {
        // Decay every pulse first so a fresh onset this tick lands at full.
        self.full.decay();
        self.bass.decay();
        self.mid.decay();
        self.treble.decay();

        if !playing || scope.len() < 2 {
            self.full.idle();
            self.bass.idle();
            self.mid.idle();
            self.treble.idle();
            return;
        }

        // Split each mono frame into bass / mid / treble with two cascaded
        // one-pole low-passes, accumulating band energy across the window.
        // Coefficients are aesthetic, not tuned to an exact crossover Hz.
        let mut ef = 0.0f32;
        let mut eb = 0.0f32;
        let mut em = 0.0f32;
        let mut et = 0.0f32;
        let frames = scope.len() / 2;
        for i in 0..frames {
            let mono = 0.5 * (scope[i * 2] + scope[i * 2 + 1]);
            self.lp_bass += (mono - self.lp_bass) * 0.025; // low cutoff -> bass
            let bass = self.lp_bass;
            let hp = mono - bass; // everything above the bass
            self.lp_mid += (hp - self.lp_mid) * 0.20; // mid band
            let mid = self.lp_mid;
            let treble = hp - self.lp_mid; // fast residual -> treble
            ef += mono * mono;
            eb += bass * bass;
            em += mid * mid;
            et += treble * treble;
        }
        let n = frames as f32;
        let rms = |e: f32| (e / n).sqrt();
        // Per-band gains: treble energy is smaller, so it gets a bigger boost.
        self.full.feed((rms(ef) * 3.2).min(1.0));
        self.bass.feed((rms(eb) * 4.0).min(1.0));
        self.mid.feed((rms(em) * 6.0).min(1.0));
        self.treble.feed((rms(et) * 10.0).min(1.0));
    }

    /// Full-band onset pulse, 0..1 — full on a fresh beat, decaying over a few
    /// frames. Used by the original Glitch theme.
    pub fn pulse(&self) -> f32 {
        self.full.pulse
    }

    /// Per-band onset pulses `[bass, mid, treble]`, each 0..1.
    pub fn bands(&self) -> [f32; 3] {
        [self.bass.pulse, self.mid.pulse, self.treble.pulse]
    }
}
