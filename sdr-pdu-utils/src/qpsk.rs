use crate::scrambler::Scrambler;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModulationType {
    #[default]
    Fsk2,
    Qpsk,
    Oqpsk,
}

pub struct QpskModulator {
    samples_per_symbol: f64,
    oqpsk: bool,
    preamble_syncword_iq_i16: Vec<i16>,
    preamble_syncword_iq_i8: Vec<i8>,
    scrambler: Option<Scrambler>,
    scrambler_seed: u32,
    amplitude_i16: i16,
    amplitude_i8: i8,
}

impl QpskModulator {
    pub fn new(sample_rate: u32, baud_rate: u32, oqpsk: bool) -> Self {
        Self {
            samples_per_symbol: (sample_rate as f64) / (baud_rate as f64),
            oqpsk,
            preamble_syncword_iq_i16: Vec::new(),
            preamble_syncword_iq_i8: Vec::new(),
            scrambler: None,
            scrambler_seed: 0,
            amplitude_i16: 23170, // ~32767 / sqrt(2)
            amplitude_i8: 90,     // ~127 / sqrt(2)
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
        let preamble_byte = crate::utils::parse_hex_byte(preamble_str)?;
        let syncword = crate::utils::parse_hex_bytes(syncword_str)?;

        let mut data = Vec::with_capacity(preamble_repetition as usize + syncword.len());
        for _ in 0..preamble_repetition {
            data.push(preamble_byte);
        }
        data.extend_from_slice(&syncword);

        self.preamble_syncword_iq_i16 = self.modulate_i16_internal(&data, false);
        self.preamble_syncword_iq_i8 = self.modulate_i8_internal(&data, false);
        Ok(())
    }

    pub fn get_preamble_syncword_iq_i16(&self) -> &[i16] {
        &self.preamble_syncword_iq_i16
    }

    pub fn get_preamble_syncword_iq_i8(&self) -> &[i8] {
        &self.preamble_syncword_iq_i8
    }

    pub fn modulate_i16(&mut self, data: &[u8]) -> Vec<i16> {
        self.modulate_i16_internal(data, true)
    }

    pub fn modulate_i8(&mut self, data: &[u8]) -> Vec<i8> {
        self.modulate_i8_internal(data, true)
    }

    fn extract_bits(&mut self, data: &[u8], reset_scrambler: bool) -> Vec<u8> {
        if let Some(ref mut scr) = self.scrambler
            && reset_scrambler
        {
            scr.reset(self.scrambler_seed);
        }

        let mut bits = Vec::with_capacity(data.len() * 8);
        for &byte in data {
            for bit_idx in 0..8 {
                let mut bit = (byte >> (7 - bit_idx)) & 1;
                if let Some(ref mut scr) = self.scrambler {
                    bit = scr.scramble_bit(bit);
                }
                bits.push(bit);
            }
        }
        bits
    }

    fn modulate_i16_internal(&mut self, data: &[u8], reset_scrambler: bool) -> Vec<i16> {
        let bits = self.extract_bits(data, reset_scrambler);
        let num_symbols = bits.len().div_ceil(2);
        let total_samples = (num_symbols as f64 * self.samples_per_symbol).ceil() as usize;

        // In OQPSK, Q is delayed by half a symbol: samples_per_symbol / 2.0
        let q_delay_samples = if self.oqpsk {
            (self.samples_per_symbol * 0.5).round() as usize
        } else {
            0
        };

        let buffer_samples = total_samples + q_delay_samples;
        let mut i_waveform = vec![0i16; buffer_samples];
        let mut q_waveform = vec![0i16; buffer_samples];

        let amp = self.amplitude_i16;

        for (sym_idx, chunk) in bits.chunks(2).enumerate() {
            let b0 = chunk[0];
            let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };

            // Gray-coded mapping:
            // bit 0 -> I (+amp for 0, -amp for 1)
            // bit 1 -> Q (+amp for 0, -amp for 1)
            let i_val = if b0 == 0 { amp } else { -amp };
            let q_val = if b1 == 0 { amp } else { -amp };

            let start_sample_i = (sym_idx as f64 * self.samples_per_symbol).floor() as usize;
            let end_sample_i = ((sym_idx + 1) as f64 * self.samples_per_symbol).floor() as usize;
            let slice_end_i = std::cmp::min(end_sample_i, buffer_samples);

            if start_sample_i < slice_end_i {
                i_waveform[start_sample_i..slice_end_i].fill(i_val);
            }

            let start_sample_q = start_sample_i + q_delay_samples;
            let end_sample_q = end_sample_i + q_delay_samples;
            let slice_end_q = std::cmp::min(end_sample_q, buffer_samples);

            if start_sample_q < slice_end_q {
                q_waveform[start_sample_q..slice_end_q].fill(q_val);
            }
        }

        // Interleave I and Q
        let mut iq = Vec::with_capacity(buffer_samples * 2);
        for s in 0..buffer_samples {
            iq.push(i_waveform[s]);
            iq.push(q_waveform[s]);
        }
        iq
    }

    fn modulate_i8_internal(&mut self, data: &[u8], reset_scrambler: bool) -> Vec<i8> {
        let bits = self.extract_bits(data, reset_scrambler);
        let num_symbols = bits.len().div_ceil(2);
        let total_samples = (num_symbols as f64 * self.samples_per_symbol).ceil() as usize;

        let q_delay_samples = if self.oqpsk {
            (self.samples_per_symbol * 0.5).round() as usize
        } else {
            0
        };

        let buffer_samples = total_samples + q_delay_samples;
        let mut i_waveform = vec![0i8; buffer_samples];
        let mut q_waveform = vec![0i8; buffer_samples];

        let amp = self.amplitude_i8;

        for (sym_idx, chunk) in bits.chunks(2).enumerate() {
            let b0 = chunk[0];
            let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };

            let i_val = if b0 == 0 { amp } else { -amp };
            let q_val = if b1 == 0 { amp } else { -amp };

            let start_sample_i = (sym_idx as f64 * self.samples_per_symbol).floor() as usize;
            let end_sample_i = ((sym_idx + 1) as f64 * self.samples_per_symbol).floor() as usize;
            let slice_end_i = std::cmp::min(end_sample_i, buffer_samples);

            if start_sample_i < slice_end_i {
                i_waveform[start_sample_i..slice_end_i].fill(i_val);
            }

            let start_sample_q = start_sample_i + q_delay_samples;
            let end_sample_q = end_sample_i + q_delay_samples;
            let slice_end_q = std::cmp::min(end_sample_q, buffer_samples);

            if start_sample_q < slice_end_q {
                q_waveform[start_sample_q..slice_end_q].fill(q_val);
            }
        }

        let mut iq = Vec::with_capacity(buffer_samples * 2);
        for s in 0..buffer_samples {
            iq.push(i_waveform[s]);
            iq.push(q_waveform[s]);
        }
        iq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qpsk_modulate_length() {
        let mut modulat = QpskModulator::new(1000000, 100000, false);
        // 1 byte = 8 bits = 4 symbols. At 1 MSPS and 100 kbaud (symbol rate), 10 samples per symbol = 40 samples = 80 interleaved values
        let data = vec![0xAA];
        let samples_i16 = modulat.modulate_i16(&data);
        assert_eq!(samples_i16.len(), 80);

        let samples_i8 = modulat.modulate_i8(&data);
        assert_eq!(samples_i8.len(), 80);
    }

    #[test]
    fn test_oqpsk_q_delay() {
        let mut modulat_qpsk = QpskModulator::new(1000000, 100000, false);
        let mut modulat_oqpsk = QpskModulator::new(1000000, 100000, true);
        let data = vec![0xAA];

        let qpsk_samples = modulat_qpsk.modulate_i16(&data);
        let oqpsk_samples = modulat_oqpsk.modulate_i16(&data);

        // OQPSK delay is 10 * 0.5 = 5 samples.
        // Total samples should be 40 + 5 = 45 samples = 90 interleaved i16 values
        assert_eq!(qpsk_samples.len(), 80);
        assert_eq!(oqpsk_samples.len(), 90);

        // Q channel for the first 5 samples in OQPSK should be 0 (delay)
        for s in 0..5 {
            let q_val = oqpsk_samples[s * 2 + 1];
            assert_eq!(q_val, 0);
        }
        // After delay, Q should have non-zero amplitude
        assert_ne!(oqpsk_samples[5 * 2 + 1], 0);
    }

    #[test]
    fn test_qpsk_scrambled() {
        let mut mod_plain = QpskModulator::new(1000000, 100000, false);
        let mut mod_scr = QpskModulator::new(1000000, 100000, false);
        mod_scr.set_scrambler(Scrambler::G3RUH_POLY, Scrambler::G3RUH_SEED);

        let data = vec![0x00, 0x00];
        let plain = mod_plain.modulate_i16(&data);
        let scr = mod_scr.modulate_i16(&data);

        assert_eq!(plain.len(), scr.len());
        assert_ne!(plain, scr);
    }
}
