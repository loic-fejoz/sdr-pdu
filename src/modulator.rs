use crate::nco::TableNco;

pub struct FskModulator {
    nco: TableNco,
    sample_rate: u32,
    deviation: u32, // in Hz
    samples_per_symbol: f64,
}

impl FskModulator {
    pub fn new(sample_rate: u32, baud_rate: u32, deviation: u32) -> Self {
        Self {
            nco: TableNco::new(10),
            sample_rate,
            deviation,
            samples_per_symbol: (sample_rate as f64) / (baud_rate as f64),
        }
    }

    pub fn modulate(&mut self, data: &[u8]) -> Vec<i16> {
        let total_bits = data.len() * 8;
        let total_samples_expected = (total_bits as f64 * self.samples_per_symbol).ceil() as usize;
        let mut buffer = Vec::with_capacity(total_samples_expected * 2);

        // Local reusable buffer for samples within a symbol to allow vectorized fill_buffer
        let mut symbol_buffer = vec![0i16; (self.samples_per_symbol.ceil() as usize + 2) * 2];

        let mut bit_counter = 1.0;
        for &byte in data {
            for bit_idx in 0..8 {
                let bit = (byte >> (7 - bit_idx)) & 1;

                let freq = if bit == 1 {
                    self.deviation as i32
                } else {
                    -(self.deviation as i32)
                };

                let phase_inc = if freq >= 0 {
                    ((freq as u64) << 32) / (self.sample_rate as u64)
                } else {
                    (((freq.unsigned_abs() as u64) << 32) / (self.sample_rate as u64))
                        .wrapping_neg()
                } as u32;

                // Absolute sample position for this bit end
                let end_sample_idx = (bit_counter * self.samples_per_symbol).floor() as usize;
                let current_sample_count = buffer.len() / 2;
                let samples_to_gen = end_sample_idx.saturating_sub(current_sample_count);

                if samples_to_gen > 0 {
                    let bit_buffer = &mut symbol_buffer[..(samples_to_gen * 2)];
                    self.nco.fill_buffer(phase_inc, bit_buffer);
                    buffer.extend_from_slice(bit_buffer);
                }

                bit_counter += 1.0;
            }
        }

        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulator_length() {
        let mut modulat = FskModulator::new(1000000, 9600, 2400);
        let data = vec![0xAA]; // 10101010
        let samples = modulat.modulate(&data);

        // 8 bits * (1000000/9600) = 8 * 104.166... = 833.333...
        // Ceil should give 834 or something similar depending on rounding
        assert!(samples.len() / 2 >= 833);
        assert!(samples.len() / 2 <= 835);
    }
}
