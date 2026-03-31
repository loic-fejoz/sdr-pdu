use crate::nco::TableNco;

pub struct FskModulator {
    nco: TableNco,
    samples_per_symbol: f64,
    phase_inc_pos: u32,
    phase_inc_neg: u32,
    preamble_syncword_iq: Vec<i16>,
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
        }
    }

    pub fn set_preamble_and_syncword(
        &mut self,
        preamble_str: &str,
        preamble_repetition: u32,
        syncword_str: &str,
    ) -> anyhow::Result<()> {
        let preamble_byte = self.parse_hex_byte(preamble_str)?;
        let syncword = self.parse_hex_bytes(syncword_str)?;

        let mut data = Vec::with_capacity(preamble_repetition as usize + syncword.len());
        for _ in 0..preamble_repetition {
            data.push(preamble_byte);
        }
        data.extend_from_slice(&syncword);

        self.preamble_syncword_iq = self.modulate(&data);
        Ok(())
    }

    pub fn get_preamble_syncword_iq(&self) -> &[i16] {
        &self.preamble_syncword_iq
    }

    fn parse_hex_byte(&self, s: &str) -> anyhow::Result<u8> {
        let s = s.trim_start_matches("0x").trim_start_matches("0X");
        if s.len() > 2 {
            anyhow::bail!("Invalid hex byte: {}", s);
        }
        u8::from_str_radix(s, 16).map_err(|e| anyhow::anyhow!("Hex parse error: {}", e))
    }

    fn parse_hex_bytes(&self, s: &str) -> anyhow::Result<Vec<u8>> {
        let s = s.trim_start_matches("0x").trim_start_matches("0X");
        if s.is_empty() {
            return Ok(Vec::new());
        }
        if s.len() % 2 != 0 {
            // Prepend a zero if length is odd, e.g., "7E" -> "7E", but "7" -> "07"
            let padded = format!("0{}", s);
            self.parse_hex_bytes_even(&padded)
        } else {
            self.parse_hex_bytes_even(s)
        }
    }

    fn parse_hex_bytes_even(&self, s: &str) -> anyhow::Result<Vec<u8>> {
        let mut res = Vec::with_capacity(s.len() / 2);
        for i in (0..s.len()).step_by(2) {
            let byte = u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| anyhow::anyhow!("Hex parse error at {}: {}", &s[i..i + 2], e))?;
            res.push(byte);
        }
        Ok(res)
    }

    pub fn modulate(&mut self, data: &[u8]) -> Vec<i16> {
        let total_bits = data.len() * 8;
        let total_samples_expected = (total_bits as f64 * self.samples_per_symbol).ceil() as usize;
        let mut buffer = Vec::with_capacity(total_samples_expected * 2);

        // Local reusable buffer for samples within a symbol
        let mut symbol_buffer = vec![0i16; (self.samples_per_symbol.ceil() as usize + 8) * 2];

        let mut bit_counter = 1.0;
        for &byte in data {
            for bit_idx in 0..8 {
                let bit = (byte >> (7 - bit_idx)) & 1;
                let phase_inc = if bit == 1 {
                    self.phase_inc_pos
                } else {
                    self.phase_inc_neg
                };

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

    #[test]
    fn test_hex_parsing() {
        let modulat = FskModulator::new(1000000, 9600, 2400);
        assert_eq!(modulat.parse_hex_byte("0x55").unwrap(), 0x55);
        assert_eq!(modulat.parse_hex_byte("55").unwrap(), 0x55);
        assert_eq!(modulat.parse_hex_bytes("0x1ACFFC1D").unwrap(), vec![0x1A, 0xCF, 0xFC, 0x1D]);
        assert_eq!(modulat.parse_hex_bytes("7E").unwrap(), vec![0x7E]);
        assert_eq!(modulat.parse_hex_bytes("7").unwrap(), vec![0x07]);
    }
}
