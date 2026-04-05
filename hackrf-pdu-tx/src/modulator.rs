use crate::nco::TableNco;

pub struct Scrambler {
    state: u32,
    poly: u32,
}

impl Scrambler {
    pub fn new(poly: u32, seed: u32) -> Self {
        Self { state: seed, poly }
    }

    /// Process a single bit through the multiplicative scrambler.
    /// Standard G3RUH: poly = 1 + x^12 + x^17.
    /// In bitmask form, bit 12 and 17 are set.
    pub fn scramble_bit(&mut self, bit: u8) -> u8 {
        // Calculate the XOR sum of bits defined by the polynomial
        // state & poly picks the bits to XOR.
        let xor_sum = (self.state & self.poly).count_ones() % 2;
        let out_bit = (bit as u32 ^ xor_sum) as u8 & 1;

        // Shift state and insert the NEW bit (multiplicative scrambler)
        self.state = (self.state << 1) | (out_bit as u32);
        out_bit
    }

    pub fn reset(&mut self, seed: u32) {
        self.state = seed;
    }
}

pub struct FskModulator {
    nco: TableNco,
    samples_per_symbol: f64,
    phase_inc_pos: u32,
    phase_inc_neg: u32,
    preamble_syncword_iq: Vec<i8>,
    scrambler: Option<Scrambler>,
    scrambler_seed: u32,
}

impl FskModulator {
    pub fn new(sample_rate: u32, baud_rate: u32, deviation: u32) -> Self {
        let phase_inc_pos = (((deviation as u64) << 32) / (sample_rate as u64)) as u32;
        let phase_inc_neg = phase_inc_pos.wrapping_neg();

        Self {
            nco: TableNco::new(10),
            samples_per_symbol: (sample_rate as f64) / (baud_rate as f64),
            phase_inc_pos,
            phase_inc_neg,
            preamble_syncword_iq: Vec::new(),
            scrambler: None,
            scrambler_seed: 0,
        }
    }

    pub fn set_scrambler(&mut self, poly: u32, seed: u32) {
        self.scrambler = Some(Scrambler::new(poly, seed));
        self.scrambler_seed = seed;
    }

    pub fn set_preamble_and_syncword(
        &mut self,
        preamble_str: &str,
        preamble_repetition: u32,
        syncword_str: &str,
    ) -> anyhow::Result<()> {
        let preamble_byte = sdr_pdu_utils::utils::parse_hex_byte(preamble_str)?;
        let syncword = sdr_pdu_utils::utils::parse_hex_bytes(syncword_str)?;

        let mut data = Vec::with_capacity(preamble_repetition as usize + syncword.len());
        for _ in 0..preamble_repetition {
            data.push(preamble_byte);
        }
        data.extend_from_slice(&syncword);

        // Modulate preamble/syncword WITHOUT resetting scrambler?
        // Usually, the scrambler is NOT reset between preamble and data in G3RUH.
        // But for the preamble_syncword_iq cache, we must be careful.
        // We modulate it once and cache it.
        self.preamble_syncword_iq = self.modulate_internal(&data, false);
        Ok(())
    }

    pub fn get_preamble_syncword_iq(&self) -> &[i8] {
        &self.preamble_syncword_iq
    }

    pub fn modulate(&mut self, data: &[u8]) -> Vec<i8> {
        self.modulate_internal(data, true)
    }

    fn modulate_internal(&mut self, data: &[u8], reset_scrambler: bool) -> Vec<i8> {
        if let Some(ref mut scr) = self.scrambler
            && reset_scrambler
        {
            scr.reset(self.scrambler_seed);
        }

        let total_bits = data.len() * 8;
        let total_samples_expected = (total_bits as f64 * self.samples_per_symbol).ceil() as usize;
        let mut buffer = Vec::with_capacity(total_samples_expected * 2);

        let mut symbol_buffer = vec![0i8; (self.samples_per_symbol.ceil() as usize + 8) * 2];

        let mut bit_counter = 1.0;
        for &byte in data {
            for bit_idx in 0..8 {
                let mut bit = (byte >> (7 - bit_idx)) & 1;

                if let Some(ref mut scr) = self.scrambler {
                    bit = scr.scramble_bit(bit);
                }

                let phase_inc = if bit == 1 {
                    self.phase_inc_pos
                } else {
                    self.phase_inc_neg
                };

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
    use quickcheck_macros::quickcheck;

    struct Descrambler {
        state: u32,
        poly: u32,
    }

    impl Descrambler {
        fn new(poly: u32, seed: u32) -> Self {
            Self { state: seed, poly }
        }

        fn descramble_bit(&mut self, bit: u8) -> u8 {
            let xor_sum = (self.state & self.poly).count_ones() % 2;
            let out_bit = (bit as u32 ^ xor_sum) as u8 & 1;
            self.state = (self.state << 1) | (bit as u32);
            out_bit
        }
    }

    #[quickcheck]
    fn prop_scrambler_roundtrip(data: Vec<u8>, poly: u32, seed: u32) -> bool {
        let poly = poly & 0x1FFFF; // Mask to 17 bits for G3RUH-like
        let mut scr = Scrambler::new(poly, seed);
        let mut descr = Descrambler::new(poly, seed);

        for byte in data {
            for i in 0..8 {
                let bit = (byte >> (7 - i)) & 1;
                let scrambled = scr.scramble_bit(bit);
                let descrambled = descr.descramble_bit(scrambled);
                if bit != descrambled {
                    return false;
                }
            }
        }
        true
    }

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

    #[test]
    fn test_scrambler_g3ruh() {
        // G3RUH: bits 12 and 17 (1-indexed) -> bits 11 and 16 (0-indexed)
        let poly = (1 << 11) | (1 << 16);
        let mut scr = Scrambler::new(poly, 0);

        // Input: all zeros
        assert_eq!(scr.scramble_bit(0), 0);

        // Input: 1
        // out = 1 ^ (ones(0 & poly)%2) = 1. state = 1 (bit 0 set)
        assert_eq!(scr.scramble_bit(1), 1);

        // After 11 MORE shifts (total 12 bits processed), the first '1' reaches bit 11
        for _ in 0..10 {
            scr.scramble_bit(0);
        }
        // The state before the next call has bit 10 set.
        // Wait, if state is (state << 1) | bit.
        // Bit 0: 1. state = 1.
        // Bit 1: 0. state = 10 (binary).
        // ...
        // Bit 11: 0. state = 100000000000 (binary) -> bit 11 is 1.
        scr.scramble_bit(0);

        // Now bit 11 of state is 1.
        // Input 0: out = 0 ^ (ones(state & poly)%2) = 0 ^ 1 = 1.
        assert_eq!(scr.scramble_bit(0), 1);
    }
}
