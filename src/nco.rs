use std::f64::consts::PI;

pub struct TableNco {
    phase: u32,
    lut: Vec<i16>,
    lut_mask: u32,
    lut_shift: u32,
}

impl TableNco {
    pub fn new(lut_size_bits: u32) -> Self {
        let size = 1 << lut_size_bits;
        let mut lut = Vec::with_capacity(size * 2); // I and Q
        for i in 0..size {
            let angle = 2.0 * PI * (i as f64) / (size as f64);
            // Scale to i16, avoid full range to prevent overflow in some DSP ops if needed,
            // but for pure NCO 32767 is fine.
            let s = (angle.sin() * 32767.0) as i16;
            let c = (angle.cos() * 32767.0) as i16;
            lut.push(c); // I
            lut.push(s); // Q
        }

        Self {
            phase: 0,
            lut,
            lut_mask: (size - 1) as u32,
            lut_shift: 32 - lut_size_bits,
        }
    }

    #[allow(dead_code)]
    pub fn next(&mut self, phase_inc: u32) -> (i16, i16) {
        let (i, q) = self.get_at_phase(self.phase);
        self.phase = self.phase.wrapping_add(phase_inc);
        (i, q)
    }

    #[allow(dead_code)]
    #[inline(always)]
    fn get_at_phase(&self, phase: u32) -> (i16, i16) {
        let idx = ((phase >> self.lut_shift) & self.lut_mask) as usize;
        let i = self.lut[idx * 2];
        let q = self.lut[idx * 2 + 1];
        (i, q)
    }

    pub fn fill_buffer(&mut self, phase_inc: u32, buffer: &mut [i16]) {
        // buffer is expected to be [I0, Q0, I1, Q1, ...]
        // Using a local phase variable to help the optimizer
        let mut p = self.phase;
        for chunk in buffer.chunks_exact_mut(2) {
            let idx = ((p >> self.lut_shift) & self.lut_mask) as usize;
            chunk[0] = self.lut[idx * 2];
            chunk[1] = self.lut[idx * 2 + 1];
            p = p.wrapping_add(phase_inc);
        }
        self.phase = p;
    }
    #[allow(dead_code)]
    pub fn set_phase(&mut self, phase: u32) {
        self.phase = phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nco_dc() {
        let mut nco = TableNco::new(10);
        let (i, q) = nco.next(0);
        assert_eq!(i, 32767);
        assert_eq!(q, 0);

        let (i, q) = nco.next(0);
        assert_eq!(i, 32767);
        assert_eq!(q, 0);
    }

    #[test]
    fn test_nco_quarter_cycle() {
        let mut nco = TableNco::new(10);
        // Phase increment for 1/4 cycle: 2^32 / 4 = 2^30
        let inc = 1u32 << 30;

        let (i, q) = nco.next(inc);
        assert_eq!(i, 32767);
        assert_eq!(q, 0);

        let (i, q) = nco.next(inc);
        // At 90 degrees
        assert!(i.abs() < 100);
        assert!(q > 32600);

        let (i, q) = nco.next(inc);
        // At 180 degrees
        assert!(i < -32600);
        assert!(q.abs() < 100);

        let (i, q) = nco.next(inc);
        // At 270 degrees
        assert!(i.abs() < 100);
        assert!(q < -32600);
    }
}
