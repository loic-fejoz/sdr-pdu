//! Channel simulation models (AWGN noise, carrier frequency offset, phase offset, and timing offset)
//! and soft/hard demodulator implementations for verification of modulated signals.

use std::f64::consts::PI;

pub struct ChannelSimulator {
    rng_state: u64,
}

impl ChannelSimulator {
    pub fn new(seed: u64) -> Self {
        Self {
            rng_state: if seed == 0 {
                0xCAFE_BABE_DEAD_BEEF
            } else {
                seed
            },
        }
    }

    /// Linear congruential generator / Xorshift64*
    fn next_f64(&mut self) -> f64 {
        let mut x = self.rng_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng_state = x;
        let val = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (val as f64) / (u64::MAX as f64)
    }

    /// Box-Muller transform for generating standard normal random variables
    fn next_gaussian(&mut self) -> (f64, f64) {
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * PI * u2;
        (r * theta.cos(), r * theta.sin())
    }

    /// Add Additive White Gaussian Noise (AWGN), Carrier Frequency Offset (CFO), and initial phase offset.
    ///
    /// - `snr_db`: Signal-to-noise ratio in dB
    /// - `cfo_norm`: Normalized carrier frequency offset (delta_f / sample_rate)
    /// - `phase_offset_rad`: Initial phase offset in radians
    pub fn apply_channel(
        &mut self,
        samples_i: &[f64],
        samples_q: &[f64],
        snr_db: f64,
        cfo_norm: f64,
        phase_offset_rad: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        assert_eq!(samples_i.len(), samples_q.len());
        let n = samples_i.len();

        // Calculate signal power
        let mut p_signal = 0.0;
        for k in 0..n {
            p_signal += samples_i[k] * samples_i[k] + samples_q[k] * samples_q[k];
        }
        p_signal /= n as f64;

        // Noise variance sigma^2 = P_signal / (10^(SNR/10))
        // Half variance per dimension (I and Q)
        let snr_linear = 10.0_f64.powf(snr_db / 10.0);
        let noise_var_total = p_signal / snr_linear;
        let sigma = (noise_var_total / 2.0).sqrt();

        let mut out_i = Vec::with_capacity(n);
        let mut out_q = Vec::with_capacity(n);

        let mut phase = phase_offset_rad;
        let phase_inc = 2.0 * PI * cfo_norm;

        for k in 0..n {
            // Frequency / phase rotation
            let cos_p = phase.cos();
            let sin_p = phase.sin();
            let rot_i = samples_i[k] * cos_p - samples_q[k] * sin_p;
            let rot_q = samples_i[k] * sin_p + samples_q[k] * cos_p;

            // AWGN noise
            let (n_i, n_q) = self.next_gaussian();
            out_i.push(rot_i + sigma * n_i);
            out_q.push(rot_q + sigma * n_q);

            phase = (phase + phase_inc) % (2.0 * PI);
        }

        (out_i, out_q)
    }
}

/// Demodulator for 2FSK and QPSK/OQPSK signals
pub struct SignalDemodulator;

impl SignalDemodulator {
    /// Demodulate 2FSK baseband complex samples using discrete instantaneous frequency / phase differentiation.
    ///
    /// f_inst[k] = arg( s[k] * conj(s[k-1]) )
    pub fn demodulate_2fsk(samples: &[i16], sample_rate: u32, baud_rate: u32) -> Vec<u8> {
        let n_samples = samples.len() / 2;
        if n_samples < 2 {
            return Vec::new();
        }

        let samples_per_symbol = (sample_rate as f64) / (baud_rate as f64);

        // Convert to f64 complex and compute instantaneous frequency
        let mut freq_inst = Vec::with_capacity(n_samples);
        let mut prev_i = samples[0] as f64;
        let mut prev_q = samples[1] as f64;

        for &[cur_i_raw, cur_q_raw] in samples[2..].as_chunks::<2>().0 {
            let cur_i = cur_i_raw as f64;
            let cur_q = cur_q_raw as f64;

            // cur * conj(prev) = (cur_i + j cur_q) * (prev_i - j prev_q)
            let diff_i = cur_i * prev_i + cur_q * prev_q;
            let diff_q = cur_q * prev_i - cur_i * prev_q;

            let phase_diff = diff_q.atan2(diff_i);
            freq_inst.push(phase_diff);

            prev_i = cur_i;
            prev_q = cur_q;
        }

        // Integrate over each symbol period to detect bit (positive phase diff = 1, negative = 0)
        let total_symbols = ((n_samples as f64) / samples_per_symbol).floor() as usize;
        let mut bits = Vec::with_capacity(total_symbols);

        for sym_idx in 0..total_symbols {
            let start = (sym_idx as f64 * samples_per_symbol).floor() as usize;
            let end = (((sym_idx + 1) as f64) * samples_per_symbol).floor() as usize;
            let end = end.min(freq_inst.len());

            if start >= end {
                continue;
            }

            let mut sum_freq = 0.0;
            for &f in &freq_inst[start..end] {
                sum_freq += f;
            }

            let bit = if sum_freq > 0.0 { 1u8 } else { 0u8 };
            bits.push(bit);
        }

        // Pack bits into bytes (MSB first)
        let mut bytes = Vec::with_capacity(bits.len() / 8);
        for byte_bits in bits.as_chunks::<8>().0 {
            let mut byte = 0u8;
            for (idx, &b) in byte_bits.iter().enumerate() {
                byte |= b << (7 - idx);
            }
            bytes.push(byte);
        }

        bytes
    }

    /// Demodulate QPSK / OQPSK baseband samples.
    ///
    /// For standard QPSK, I and Q are sampled synchronously at symbol centers.
    /// For OQPSK, Q is staggered by half a symbol: sample_rate / (2 * baud_rate).
    pub fn demodulate_qpsk(
        samples: &[i16],
        sample_rate: u32,
        baud_rate: u32,
        oqpsk: bool,
    ) -> Vec<u8> {
        let n_samples = samples.len() / 2;
        let samples_per_symbol = (sample_rate as f64) / (baud_rate as f64);
        let q_delay_samples = if oqpsk {
            (samples_per_symbol * 0.5).round() as usize
        } else {
            0
        };

        let total_symbols = if oqpsk {
            (((n_samples.saturating_sub(q_delay_samples)) as f64) / samples_per_symbol).floor()
                as usize
        } else {
            ((n_samples as f64) / samples_per_symbol).floor() as usize
        };

        let mut bits = Vec::with_capacity(total_symbols * 2);

        for sym_idx in 0..total_symbols {
            // Integrate over symbol period for maximum likelihood / matched filter
            let start_i = (sym_idx as f64 * samples_per_symbol).floor() as usize;
            let end_i = (((sym_idx + 1) as f64) * samples_per_symbol).floor() as usize;
            let end_i = end_i.min(n_samples);

            let mut sum_i = 0.0;
            for idx in start_i..end_i {
                sum_i += samples[idx * 2] as f64;
            }

            let start_q = (sym_idx as f64 * samples_per_symbol).floor() as usize + q_delay_samples;
            let end_q =
                (((sym_idx + 1) as f64) * samples_per_symbol).floor() as usize + q_delay_samples;
            let end_q = end_q.min(n_samples);

            let mut sum_q = 0.0;
            for idx in start_q..end_q {
                sum_q += samples[idx * 2 + 1] as f64;
            }

            // Gray map: positive amplitude = 0, negative = 1
            let b0 = if sum_i >= 0.0 { 0u8 } else { 1u8 };
            let b1 = if sum_q >= 0.0 { 0u8 } else { 1u8 };

            bits.push(b0);
            bits.push(b1);
        }

        let mut bytes = Vec::with_capacity(bits.len() / 8);
        for byte_bits in bits.as_chunks::<8>().0 {
            let mut byte = 0u8;
            for (idx, &b) in byte_bits.iter().enumerate() {
                byte |= b << (7 - idx);
            }
            bytes.push(byte);
        }

        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpsk::QpskModulator;
    use crate::scrambler::Scrambler;

    #[test]
    fn test_qpsk_loopback_clean() {
        let sample_rate = 1_000_000;
        let baud_rate = 50_000; // 20 samples per symbol
        let mut modulat = QpskModulator::new(sample_rate, baud_rate, false);

        let data = vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let samples = modulat.modulate_i16(&data);

        let decoded = SignalDemodulator::demodulate_qpsk(&samples, sample_rate, baud_rate, false);
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_oqpsk_loopback_clean() {
        let sample_rate = 1_000_000;
        let baud_rate = 50_000;
        let mut modulat = QpskModulator::new(sample_rate, baud_rate, true);

        let data = vec![0xAA, 0x55, 0xC3, 0x3C, 0x0F, 0xF0];
        let samples = modulat.modulate_i16(&data);

        let decoded = SignalDemodulator::demodulate_qpsk(&samples, sample_rate, baud_rate, true);
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_qpsk_scrambled_loopback() {
        let sample_rate = 1_000_000;
        let baud_rate = 50_000;
        let mut modulat = QpskModulator::new(sample_rate, baud_rate, false);
        modulat.set_scrambler(Scrambler::G3RUH_POLY, Scrambler::G3RUH_SEED);

        let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
        let samples = modulat.modulate_i16(&data);

        // Demodulator recovers scrambled bits
        let scrambled_decoded =
            SignalDemodulator::demodulate_qpsk(&samples, sample_rate, baud_rate, false);

        // Descramble
        let mut descr =
            crate::scrambler::Descrambler::new(Scrambler::G3RUH_POLY, Scrambler::G3RUH_SEED);
        let mut descrambled = Vec::new();
        for byte in scrambled_decoded {
            let mut out_byte = 0u8;
            for idx in 0..8 {
                let bit = (byte >> (7 - idx)) & 1;
                let descr_bit = descr.descramble_bit(bit);
                out_byte |= descr_bit << (7 - idx);
            }
            descrambled.push(out_byte);
        }

        assert_eq!(descrambled, data);
    }

    #[test]
    fn test_qpsk_loopback_under_awgn_noise() {
        let sample_rate = 1_000_000;
        let baud_rate = 50_000;
        let mut modulat = QpskModulator::new(sample_rate, baud_rate, false);

        let data = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x20, 0x53, 0x44, 0x52, 0x21]; // "Hello SDR!"
        let samples = modulat.modulate_i16(&data);

        // Split I and Q to f64 for channel simulation
        let n_samples = samples.len() / 2;
        let mut sig_i = Vec::with_capacity(n_samples);
        let mut sig_q = Vec::with_capacity(n_samples);
        for &[i, q] in samples.as_chunks::<2>().0 {
            sig_i.push(i as f64);
            sig_q.push(q as f64);
        }

        let mut channel = ChannelSimulator::new(42);
        // Test with 12 dB SNR (very reasonable for QPSK, error rate should be 0 for small packets)
        let (noisy_i, noisy_q) = channel.apply_channel(&sig_i, &sig_q, 12.0, 0.0, 0.0);

        // Convert back to i16 samples
        let mut noisy_samples = Vec::with_capacity(n_samples * 2);
        for k in 0..n_samples {
            noisy_samples.push(noisy_i[k].round().clamp(-32768.0, 32767.0) as i16);
            noisy_samples.push(noisy_q[k].round().clamp(-32768.0, 32767.0) as i16);
        }

        let decoded =
            SignalDemodulator::demodulate_qpsk(&noisy_samples, sample_rate, baud_rate, false);
        assert_eq!(
            decoded, data,
            "Demodulated data with 12 dB SNR must match original payload"
        );
    }

    #[test]
    fn test_oqpsk_loopback_under_awgn_noise() {
        let sample_rate = 1_000_000;
        let baud_rate = 50_000;
        let mut modulat = QpskModulator::new(sample_rate, baud_rate, true);

        let data = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34];
        let samples = modulat.modulate_i16(&data);

        let n_samples = samples.len() / 2;
        let mut sig_i = Vec::with_capacity(n_samples);
        let mut sig_q = Vec::with_capacity(n_samples);
        for &[i, q] in samples.as_chunks::<2>().0 {
            sig_i.push(i as f64);
            sig_q.push(q as f64);
        }

        let mut channel = ChannelSimulator::new(12345);
        let (noisy_i, noisy_q) = channel.apply_channel(&sig_i, &sig_q, 14.0, 0.0, 0.0);

        let mut noisy_samples = Vec::with_capacity(n_samples * 2);
        for k in 0..n_samples {
            noisy_samples.push(noisy_i[k].round().clamp(-32768.0, 32767.0) as i16);
            noisy_samples.push(noisy_q[k].round().clamp(-32768.0, 32767.0) as i16);
        }

        let decoded =
            SignalDemodulator::demodulate_qpsk(&noisy_samples, sample_rate, baud_rate, true);
        assert_eq!(decoded, data);
    }
}
