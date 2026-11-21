use std::f64::consts::PI;

/// Root Raised Cosine (RRC) filter generator and pulse shaper.
///
/// The continuous RRC impulse response $h(t)$ normalized by symbol period $T$ (where $t = n \cdot T_s$ and $T_s = T / \text{sps}$):
///
/// For $t = 0$:
///   $$h(0) = 1.0 - \alpha + \frac{4\alpha}{\pi}$$
///
/// For $t = \pm \frac{T}{4\alpha}$:
///   $$h\left(\pm \frac{T}{4\alpha}\right) = \frac{\alpha}{\sqrt{2}} \left[ \left(1 + \frac{2}{\pi}\right)\sin\left(\frac{\pi}{4\alpha}\right) + \left(1 - \frac{2}{\pi}\right)\cos\left(\frac{\pi}{4\alpha}\right) \right]$$
///
/// For other $t$:
///   $$h(t) = \frac{\sin\left(\pi \frac{t}{T}(1-\alpha)\right) + 4\alpha \frac{t}{T} \cos\left(\pi \frac{t}{T}(1+\alpha)\right)}{\pi \frac{t}{T} \left(1 - \left(4\alpha \frac{t}{T}\right)^2\right)}$$
#[derive(Debug, Clone)]
pub struct RrcFilter {
    alpha: f64,
    sps: usize,
    ntaps: usize,
    taps: Vec<f64>,
}

impl RrcFilter {
    /// Creates a new Root Raised Cosine filter.
    ///
    /// # Arguments
    /// * `alpha` - Roll-off factor, typically in $[0.0, 1.0]$ (e.g. 0.35, 0.5)
    /// * `sps` - Samples per symbol (must be $\ge 1$)
    /// * `span` - Filter span in symbols (number of symbols spanned by the filter, typically 6 to 12)
    ///
    /// The total number of taps will be `span * sps + 1` (odd length for linear phase symmetric FIR).
    pub fn new(alpha: f64, sps: usize, span: usize) -> Self {
        assert!(sps >= 1, "samples per symbol must be >= 1");
        assert!(span >= 1, "filter span must be >= 1");
        assert!(
            (0.0..=1.0).contains(&alpha),
            "alpha (roll-off) must be in [0.0, 1.0]"
        );

        let ntaps = span * sps + 1;
        let taps = Self::design_taps(alpha, sps, ntaps);

        Self {
            alpha,
            sps,
            ntaps,
            taps,
        }
    }

    /// Design normalized RRC filter taps with unit energy ($\sum h[n]^2 = 1$).
    pub fn design_taps(alpha: f64, sps: usize, ntaps: usize) -> Vec<f64> {
        let mut taps = Vec::with_capacity(ntaps);
        let center = (ntaps as f64 - 1.0) / 2.0;
        let sps_f = sps as f64;

        for i in 0..ntaps {
            // Normalized time t / T
            let t = (i as f64 - center) / sps_f;
            let val = Self::rrc_response(t, alpha);
            taps.push(val);
        }

        // Normalize filter so that energy is 1.0 (standard for TX/RX matched filter pair)
        let energy: f64 = taps.iter().map(|&x| x * x).sum();
        if energy > 0.0 {
            let scale = 1.0 / energy.sqrt();
            for tap in &mut taps {
                *tap *= scale;
            }
        }

        taps
    }

    /// Evaluates continuous RRC impulse response $h(t / T)$.
    #[inline]
    fn rrc_response(t: f64, alpha: f64) -> f64 {
        if t.abs() < 1e-9 {
            // t = 0
            1.0 - alpha + 4.0 * alpha / PI
        } else if alpha > 1e-6 && (t.abs() - 1.0 / (4.0 * alpha)).abs() < 1e-6 {
            // t = +/- T / (4*alpha) singularity
            (alpha / std::f64::consts::SQRT_2)
                * ((1.0 + 2.0 / PI) * (PI / (4.0 * alpha)).sin()
                    + (1.0 - 2.0 / PI) * (PI / (4.0 * alpha)).cos())
        } else {
            let num =
                (PI * t * (1.0 - alpha)).sin() + 4.0 * alpha * t * (PI * t * (1.0 + alpha)).cos();
            let denom = PI * t * (1.0 - (4.0 * alpha * t).powi(2));
            num / denom
        }
    }

    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    pub fn sps(&self) -> usize {
        self.sps
    }

    pub fn ntaps(&self) -> usize {
        self.ntaps
    }

    pub fn taps(&self) -> &[f64] {
        &self.taps
    }

    /// Pulse shape an upsampled symbol impulse train using convolution.
    ///
    /// Given impulses `symbols` (e.g. $[+1.0, 0, 0, ..., -1.0, 0, 0]$), computes the linear convolution with the filter taps.
    pub fn filter(&self, input: &[f64]) -> Vec<f64> {
        let mut output = vec![0.0; input.len() + self.ntaps - 1];
        for (i, &x) in input.iter().enumerate() {
            if x.abs() < 1e-12 {
                continue;
            }
            for (j, &tap) in self.taps.iter().enumerate() {
                output[i + j] += x * tap;
            }
        }
        output
    }

    /// Pulse shape an impulse train of symbols directly:
    /// Each input symbol in `symbols` is placed at offset `k * sps` followed by `sps - 1` zeros.
    /// Returns the filtered waveform.
    pub fn shape_symbols(&self, symbols: &[f64]) -> Vec<f64> {
        let total_samples = symbols.len() * self.sps + self.ntaps - 1;
        let mut output = vec![0.0; total_samples];

        for (sym_idx, &sym) in symbols.iter().enumerate() {
            if sym.abs() < 1e-12 {
                continue;
            }
            let offset = sym_idx * self.sps;
            for (j, &tap) in self.taps.iter().enumerate() {
                output[offset + j] += sym * tap;
            }
        }

        output
    }

    /// Apply direct FIR filtering over a continuous signal (standard convolution).
    pub fn convolve(&self, input: &[f64]) -> Vec<f64> {
        self.filter(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrc_symmetry() {
        let rrc = RrcFilter::new(0.35, 4, 8);
        let taps = rrc.taps();
        assert_eq!(taps.len(), 8 * 4 + 1);

        // FIR taps should be strictly symmetric
        for i in 0..taps.len() / 2 {
            let left = taps[i];
            let right = taps[taps.len() - 1 - i];
            assert!(
                (left - right).abs() < 1e-9,
                "Tap {} ({}) != tap {} ({})",
                i,
                left,
                taps.len() - 1 - i,
                right
            );
        }
    }

    #[test]
    fn test_rrc_unit_energy() {
        let rrc = RrcFilter::new(0.5, 8, 6);
        let energy: f64 = rrc.taps().iter().map(|&x| x * x).sum();
        assert!(
            (energy - 1.0).abs() < 1e-9,
            "Energy must be normalized to 1.0, got {}",
            energy
        );
    }

    #[test]
    fn test_rrc_singularity_continuity() {
        // Test near singularity point t = 1 / (4 * alpha)
        let alpha = 0.25;
        let t_sing = 1.0 / (4.0 * alpha);
        let val_sing = RrcFilter::rrc_response(t_sing, alpha);

        // Approximate limit with epsilon
        let eps = 1e-7;
        let val_before = RrcFilter::rrc_response(t_sing - eps, alpha);
        let val_after = RrcFilter::rrc_response(t_sing + eps, alpha);

        assert!(
            (val_sing - val_before).abs() < 1e-4,
            "singularity discontinuity: {} vs {}",
            val_sing,
            val_before
        );
        assert!(
            (val_sing - val_after).abs() < 1e-4,
            "singularity discontinuity: {} vs {}",
            val_sing,
            val_after
        );
    }

    #[test]
    fn test_rrc_matched_filter_nyquist_isi() {
        // Convolving an RRC filter with another identical RRC filter produces a Raised Cosine (RC) filter.
        // A Raised Cosine filter satisfies the Nyquist ISI criterion:
        // h_rc[n * sps] = 0 for all n != 0 (zero Inter-Symbol Interference at symbol sample points).
        let sps = 8;
        let span = 10;
        let alpha = 0.35;
        let rrc = RrcFilter::new(alpha, sps, span);

        // Convolve RRC taps with themselves
        let rc = rrc.convolve(rrc.taps());
        let center = (rc.len() - 1) / 2;
        let peak = rc[center];

        // Check that at multiples of sps away from center, the ISI is near 0
        for k in 1..(span - 1) {
            let left_sample = rc[center - k * sps];
            let right_sample = rc[center + k * sps];

            let left_isi = (left_sample / peak).abs();
            let right_isi = (right_sample / peak).abs();

            assert!(
                left_isi < 0.05,
                "ISI at -{} symbols too high: {} (peak {})",
                k,
                left_sample,
                peak
            );
            assert!(
                right_isi < 0.05,
                "ISI at +{} symbols too high: {} (peak {})",
                k,
                right_sample,
                peak
            );
        }
    }
}
