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

/// Tempo (BPM + beat phase) estimated from full-band onset times. Intervals
/// between onsets are octave-folded into a plausible beat range and averaged
/// around their median; a light phase-locked loop keeps a beat grid anchored
/// to onsets that land near predictions, so effects can *anticipate* beats
/// (swing a pendulum, brighten just before the hit) instead of reacting late.
/// All time arguments are seconds on one monotonic clock, passed explicitly so
/// the tracker is testable with synthetic onsets.
pub struct TempoTracker {
    /// Recent octave-folded onset intervals (secs), newest last.
    intervals: Vec<f64>,
    last_onset: Option<f64>,
    /// Estimated beat period (secs); 0.0 = unknown.
    period: f64,
    /// Time of a beat on the locked grid (phase reference).
    anchor: f64,
    /// Fraction of recent intervals agreeing with `period` (0..1).
    confidence: f64,
    /// Cached per-tick outputs.
    phase: f32,
    /// Phase over a TWO-beat cycle (0..1) — a pendulum's full swing period,
    /// so alternating left/right beats fall out of one continuous value.
    phase2: f32,
}

/// Octave-fold plausible beat periods into this range (secs): ~60-180 BPM.
const FOLD_LO: f64 = 0.333;
const FOLD_HI: f64 = 1.0;
/// How many folded intervals we remember / need before trusting an estimate.
const HISTORY: usize = 24;
const MIN_INTERVALS: usize = 6;

impl TempoTracker {
    fn new() -> Self {
        Self {
            intervals: Vec::new(),
            last_onset: None,
            period: 0.0,
            anchor: 0.0,
            confidence: 0.0,
            phase: 0.0,
            phase2: 0.0,
        }
    }

    /// Advance one tick at time `now`; `onset` marks a fresh full-band hit.
    pub fn on_tick(&mut self, now: f64, onset: bool) {
        if onset {
            if let Some(prev) = self.last_onset {
                let delta = now - prev;
                // Ignore double-fires and gaps too long to be adjacent beats.
                if (0.20..=2.2).contains(&delta) {
                    let mut d = delta;
                    while d > FOLD_HI {
                        d /= 2.0;
                    }
                    while d < FOLD_LO {
                        d *= 2.0;
                    }
                    self.intervals.push(d);
                    if self.intervals.len() > HISTORY {
                        self.intervals.remove(0);
                    }
                    self.reestimate();
                }
            }
            self.last_onset = Some(now);
            if self.period > 0.0 {
                // PLL: an onset near a predicted beat pulls the grid toward it;
                // off-grid onsets (syncopation) leave the phase alone.
                let beats = ((now - self.anchor) / self.period).round();
                let target = self.anchor + beats * self.period;
                let err = now - target;
                if err.abs() < 0.18 * self.period {
                    self.anchor += err * 0.5;
                } else if self.confidence < 0.5 {
                    // Not locked yet: re-anchor freely so we can converge.
                    self.anchor = now;
                }
            } else {
                self.anchor = now;
            }
        }
        // Stale music (or silence): decay trust until the estimate goes away.
        if let Some(prev) = self.last_onset
            && now - prev > 4.0 * self.period.max(0.5)
        {
            self.confidence *= 0.985;
        }
        if self.period > 0.0 {
            let beats = (now - self.anchor) / self.period;
            self.phase = beats.rem_euclid(1.0) as f32;
            self.phase2 = (beats / 2.0).rem_euclid(1.0) as f32;
        } else {
            self.phase = 0.0;
            self.phase2 = 0.0;
        }
    }

    /// Median-anchored average: estimate the period from intervals within 12%
    /// of the median, and let agreement drive confidence.
    fn reestimate(&mut self) {
        if self.intervals.len() < MIN_INTERVALS {
            return;
        }
        let mut sorted = self.intervals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        let close: Vec<f64> = self
            .intervals
            .iter()
            .copied()
            .filter(|d| (d - median).abs() < median * 0.12)
            .collect();
        if close.is_empty() {
            return;
        }
        self.period = close.iter().sum::<f64>() / close.len() as f64;
        self.confidence = close.len() as f64 / self.intervals.len() as f64;
    }

    fn locked(&self) -> bool {
        self.period > 0.0 && self.confidence >= 0.5 && self.intervals.len() >= MIN_INTERVALS
    }

    /// Beat phase 0..1 (0 = on the beat), or `None` until the lock is trusted.
    pub fn phase(&self) -> Option<f32> {
        self.locked().then_some(self.phase)
    }

    /// Phase over a two-beat cycle (a pendulum's full swing), when locked.
    pub fn phase2(&self) -> Option<f32> {
        self.locked().then_some(self.phase2)
    }

    /// Estimated tempo, or `None` until the lock is trusted. The octave fold
    /// means this can be half/double the "felt" tempo on extreme material.
    pub fn bpm(&self) -> Option<f32> {
        self.locked().then_some((60.0 / self.period) as f32)
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
    /// Tempo estimation over full-band onsets.
    tempo: TempoTracker,
    /// Monotonic clock epoch for the tracker's time axis.
    epoch: std::time::Instant,
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
            tempo: TempoTracker::new(),
            epoch: std::time::Instant::now(),
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
            // Keep the tempo clock advancing so the lock decays over a pause
            // instead of freezing mid-phase.
            self.tempo
                .on_tick(self.epoch.elapsed().as_secs_f64(), false);
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
        let before = self.full.pulse;
        self.full.feed((rms(ef) * 3.2).min(1.0));
        self.bass.feed((rms(eb) * 4.0).min(1.0));
        self.mid.feed((rms(em) * 6.0).min(1.0));
        self.treble.feed((rms(et) * 10.0).min(1.0));
        // A meaningful jump of the full-band pulse is an onset EVENT for the
        // tempo tracker (the pulse itself is a decaying envelope, not events).
        let onset = self.full.pulse > before + 0.15 && self.full.pulse > 0.35;
        self.tempo
            .on_tick(self.epoch.elapsed().as_secs_f64(), onset);
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

    /// Locked beat phase 0..1 (0 = on the beat), `None` until trusted.
    pub fn tempo_phase(&self) -> Option<f32> {
        self.tempo.phase()
    }

    /// Locked two-beat (pendulum swing) phase 0..1, `None` until trusted.
    pub fn tempo_phase2(&self) -> Option<f32> {
        self.tempo.phase2()
    }

    /// Estimated tempo in BPM, `None` until trusted.
    pub fn bpm(&self) -> Option<f32> {
        self.tempo.bpm()
    }
}

#[cfg(test)]
mod tests {
    use super::TempoTracker;

    /// Feed a steady 120 BPM click (0.5s period) with 60Hz ticks and check the
    /// tracker locks to it with phase ~0 right after each onset.
    #[test]
    fn tempo_locks_to_steady_beat() {
        let mut t = TempoTracker::new();
        let dt = 1.0 / 60.0;
        let mut now = 0.0;
        let mut next_beat = 0.5;
        for _ in 0..(60 * 20) {
            now += dt;
            let onset = now >= next_beat;
            if onset {
                next_beat += 0.5;
            }
            t.on_tick(now, onset);
        }
        let bpm = t.bpm().expect("should lock on a steady click");
        assert!((bpm - 120.0).abs() < 4.0, "bpm={bpm}");
        // Right after the last processed beat the phase should be small.
        let phase = t.phase().unwrap();
        assert!(phase < 0.2 || phase > 0.8, "phase={phase}");
    }

    /// Jittered onsets (+-20ms) still lock near the true tempo.
    #[test]
    fn tempo_tolerates_jitter() {
        let mut t = TempoTracker::new();
        let dt = 1.0 / 60.0;
        let mut now = 0.0;
        let mut k = 1u32;
        let mut next_beat = 0.5;
        for _ in 0..(60 * 30) {
            now += dt;
            if now >= next_beat {
                t.on_tick(now, true);
                k = k.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let jitter = ((k >> 16) % 40) as f64 / 1000.0 - 0.02;
                next_beat += 0.5 + jitter;
            } else {
                t.on_tick(now, false);
            }
        }
        let bpm = t.bpm().expect("should lock despite jitter");
        assert!((bpm - 120.0).abs() < 8.0, "bpm={bpm}");
    }

    /// No onsets at all: never claims a lock.
    #[test]
    fn tempo_stays_unlocked_on_silence() {
        let mut t = TempoTracker::new();
        for i in 0..600 {
            t.on_tick(i as f64 / 60.0, false);
        }
        assert!(t.bpm().is_none());
        assert!(t.phase().is_none());
    }
}
