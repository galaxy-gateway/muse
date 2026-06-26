use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

pub const FFT_SIZE: usize = 512;
pub const BANDS: usize = 96;

const MIN_FREQ_HZ: f32 = 40.0;
const VISIBLE_DB: f32 = 70.0;
const ATTACK: f32 = 0.90;
const RELEASE: f32 = 0.82;
const IDLE_DECAY: f32 = 0.34;

pub struct SpectrumState {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    input: Vec<Complex<f32>>,
    window: Vec<f32>,
    ranges: Vec<(usize, usize)>,
    bands: Vec<f32>,
}

impl SpectrumState {
    pub fn new(sample_rate: u32) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        Self {
            scratch: vec![Complex::default(); fft.get_inplace_scratch_len()],
            fft,
            input: vec![Complex::default(); FFT_SIZE],
            window: hann_window(),
            ranges: band_ranges(sample_rate),
            bands: vec![0.0; BANDS],
        }
    }

    pub fn clear(&mut self) {
        self.bands.fill(0.0);
    }

    pub fn update_scope(&mut self, scope: &[f32]) {
        self.fill_input(scope);
        self.fft
            .process_with_scratch(&mut self.input, &mut self.scratch);

        for (band, &(lo, hi)) in self.bands.iter_mut().zip(&self.ranges) {
            let peak = self.input[lo..hi]
                .iter()
                .map(|c| c.norm_sqr())
                .fold(0.0, f32::max);
            let target = power_to_unit(peak / FFT_SIZE as f32);
            let coeff = if target > *band { ATTACK } else { RELEASE };
            *band += (target - *band) * coeff;
            if *band < 0.001 {
                *band = 0.0;
            }
        }
    }

    pub fn decay(&mut self) {
        for band in &mut self.bands {
            *band *= IDLE_DECAY;
            if *band < 0.001 {
                *band = 0.0;
            }
        }
    }

    pub fn bands(&self) -> &[f32] {
        &self.bands
    }

    fn fill_input(&mut self, samples: &[f32]) {
        self.input.fill(Complex::default());
        let frames = samples.len() / 2;
        let take = frames.min(FFT_SIZE);
        let start = frames.saturating_sub(take);
        let pad = FFT_SIZE - take;
        for i in 0..take {
            let src = (start + i) * 2;
            let mono = 0.5 * (samples[src] + samples[src + 1]);
            self.input[pad + i].re = mono * self.window[pad + i];
        }
    }
}

fn hann_window() -> Vec<f32> {
    (0..FFT_SIZE)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos())
        .collect()
}

fn band_ranges(sample_rate: u32) -> Vec<(usize, usize)> {
    let nyquist = sample_rate as f32 * 0.5;
    let min_freq = MIN_FREQ_HZ.min(nyquist.max(1.0));
    let max_bin = FFT_SIZE / 2;
    (0..BANDS)
        .map(|band| {
            let lo_hz = mel_lerp(min_freq, nyquist, band as f32 / BANDS as f32);
            let hi_hz = mel_lerp(min_freq, nyquist, (band + 1) as f32 / BANDS as f32);
            let mut lo = hz_to_bin(lo_hz, sample_rate).clamp(1, max_bin);
            let mut hi = hz_to_bin(hi_hz, sample_rate).clamp(1, max_bin + 1);
            if hi <= lo {
                hi = (lo + 1).min(max_bin + 1);
            }
            if hi <= lo {
                lo = lo.saturating_sub(1);
            }
            (lo, hi)
        })
        .collect()
}

fn mel_lerp(lo: f32, hi: f32, t: f32) -> f32 {
    let hz_to_mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
    let mel_to_hz = |mel: f32| 700.0 * (10.0f32.powf(mel / 2595.0) - 1.0);
    if hi <= lo {
        hi
    } else {
        let lo_mel = hz_to_mel(lo);
        mel_to_hz(lo_mel + (hz_to_mel(hi) - lo_mel) * t)
    }
}

fn hz_to_bin(freq: f32, sample_rate: u32) -> usize {
    ((freq * FFT_SIZE as f32) / sample_rate as f32).round() as usize
}

fn power_to_unit(power: f32) -> f32 {
    if power <= 1.0e-12 {
        0.0
    } else {
        ((10.0 * power.log10() + VISIBLE_DB) / VISIBLE_DB).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: u32 = 48_000;

    #[test]
    fn spectrum_analysis_behaviors() {
        let mut state = SpectrumState::new(SAMPLE_RATE);
        state.update_scope(&vec![0.0; FFT_SIZE * 2]);
        assert!(state.bands().iter().all(|v| *v == 0.0));

        let freq = 1_000.0f32;
        state.update_scope(&sine_samples(freq));
        let peak_band = state
            .bands()
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        let signal_bin = hz_to_bin(freq, SAMPLE_RATE);
        let expected_band = state
            .ranges
            .iter()
            .position(|&(lo, hi)| lo <= signal_bin && signal_bin < hi)
            .unwrap();
        assert!((peak_band as isize - expected_band as isize).abs() <= 1);

        let one_k_bin = hz_to_bin(1_000.0, SAMPLE_RATE);
        let below_1k = band_ranges(SAMPLE_RATE)
            .iter()
            .filter(|&&(_, hi)| hi <= one_k_bin)
            .count();
        assert!(below_1k < BANDS / 3);

        let before = peak(&state);
        state.update_scope(&vec![0.0; FFT_SIZE * 2]);
        assert!(peak(&state) < before * 0.25);
        let after_release = peak(&state);
        state.decay();
        assert!(peak(&state) < after_release);

        state.clear();
        assert!(state.bands().iter().all(|v| *v == 0.0));
    }

    fn peak(state: &SpectrumState) -> f32 {
        state.bands().iter().copied().fold(0.0, f32::max)
    }

    fn sine_samples(freq: f32) -> Vec<f32> {
        (0..FFT_SIZE)
            .flat_map(|i| {
                let s = 0.1 * (std::f32::consts::TAU * freq * i as f32 / SAMPLE_RATE as f32).sin();
                [s, s]
            })
            .collect()
    }
}
