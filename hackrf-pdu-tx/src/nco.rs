use std::f64::consts::PI;

pub struct TableNco {
    phase: u32,
    lut: Vec<u16>, // Packed (I, Q) as (q << 8) | (i & 0xFF)
    lut_mask: u32,
    lut_shift: u32,
}

impl TableNco {
    pub fn new(lut_size_bits: u32) -> Self {
        let size = 1 << lut_size_bits;
        let mut lut = Vec::with_capacity(size);
        for i in 0..size {
            let angle = 2.0 * PI * (i as f64) / (size as f64);
            let s = (angle.sin() * 127.0) as i8;
            let c = (angle.cos() * 127.0) as i8;
            // Pack I and Q into a single u16.
            // In little-endian, (I as u8) will be at the lower address.
            let packed = (c as u8 as u16) | ((s as u8 as u16) << 8);
            lut.push(packed);
        }

        Self {
            phase: 0,
            lut,
            lut_mask: (size - 1) as u32,
            lut_shift: 32 - lut_size_bits,
        }
    }

    #[allow(dead_code)]
    pub fn next(&mut self, phase_inc: u32) -> (i8, i8) {
        let idx = ((self.phase >> self.lut_shift) & self.lut_mask) as usize;
        let packed = self.lut[idx];
        let i = packed as i8;
        let q = (packed >> 8) as i8;
        self.phase = self.phase.wrapping_add(phase_inc);
        (i, q)
    }

    pub fn fill_buffer(&mut self, phase_inc: u32, buffer: &mut [i8]) {
        let num_samples = buffer.len() / 2;
        if num_samples == 0 {
            return;
        }

        let out_ptr = buffer.as_mut_ptr() as *mut u16;
        let lut_ptr = self.lut.as_ptr();

        let mut p = self.phase;
        for i in 0..num_samples {
            let idx = ((p >> self.lut_shift) & self.lut_mask) as usize;
            unsafe {
                *out_ptr.add(i) = *lut_ptr.add(idx);
            }
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
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn prop_nco_fill_buffer_equivalent(lut_bits: u8, phase_inc: u32, num_samples: u8) -> bool {
        let lut_bits = (lut_bits % 12) + 4; // 4 to 15 bits
        let num_samples = num_samples as usize;
        if num_samples == 0 {
            return true;
        }

        let mut nco1 = TableNco::new(lut_bits as u32);
        let mut nco2 = TableNco::new(lut_bits as u32);

        // Advance nco1 via next()
        let mut samples_next = Vec::with_capacity(num_samples * 2);
        for _ in 0..num_samples {
            let (i, q) = nco1.next(phase_inc);
            samples_next.push(i);
            samples_next.push(q);
        }

        // Advance nco2 via fill_buffer()
        let mut samples_fill = vec![0i8; num_samples * 2];
        nco2.fill_buffer(phase_inc, &mut samples_fill);

        samples_next == samples_fill
    }

    #[test]
    fn test_nco_dc() {
        let mut nco = TableNco::new(10);
        let (i, q) = nco.next(0);
        assert_eq!(i, 127);
        assert_eq!(q, 0);

        let (i, q) = nco.next(0);
        assert_eq!(i, 127);
        assert_eq!(q, 0);
    }

    #[test]
    fn test_nco_quarter_cycle() {
        let mut nco = TableNco::new(10);
        // Phase increment for 1/4 cycle: 2^32 / 4 = 2^30
        let inc = 1u32 << 30;

        let (i, q) = nco.next(inc);
        assert_eq!(i, 127);
        assert_eq!(q, 0);

        let (i, q) = nco.next(inc);
        // At 90 degrees
        assert!(i.abs() < 2);
        assert!(q > 125);

        let (i, q) = nco.next(inc);
        // At 180 degrees
        assert!(i < -125);
        assert!(q.abs() < 2);

        let (i, q) = nco.next(inc);
        // At 270 degrees
        assert!(i.abs() < 2);
        assert!(q < -125);
    }
}
