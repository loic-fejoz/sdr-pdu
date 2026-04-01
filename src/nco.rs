use std::f64::consts::PI;
use std::simd::prelude::*;

pub struct TableNco {
    phase: u32,
    lut: Vec<u32>, // Packed (I, Q) as (q << 16) | (i & 0xFFFF)
    lut_mask: u32,
    lut_shift: u32,
}

impl TableNco {
    pub fn new(lut_size_bits: u32) -> Self {
        let size = 1 << lut_size_bits;
        let mut lut = Vec::with_capacity(size);
        for i in 0..size {
            let angle = 2.0 * PI * (i as f64) / (size as f64);
            let s = (angle.sin() * 32767.0) as i16;
            let c = (angle.cos() * 32767.0) as i16;
            // Pack I and Q into a single u32.
            // In little-endian, (I as u16) will be at the lower address.
            let packed = (c as u16 as u32) | ((s as u16 as u32) << 16);
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
    pub fn next(&mut self, phase_inc: u32) -> (i16, i16) {
        let idx = ((self.phase >> self.lut_shift) & self.lut_mask) as usize;
        let packed = self.lut[idx];
        let i = packed as i16;
        let q = (packed >> 16) as i16;
        self.phase = self.phase.wrapping_add(phase_inc);
        (i, q)
    }

    pub fn fill_buffer(&mut self, phase_inc: u32, buffer: &mut [i16]) {
        let num_samples = buffer.len() / 2;
        if num_samples == 0 {
            return;
        }

        let p = self.phase;
        let mut i = 0;

        // Use 4-lane SIMD for phase accumulation
        let v_inc4 = u32x4::splat(phase_inc.wrapping_mul(4));
        let mut v_phase = u32x4::from_array([
            p,
            p.wrapping_add(phase_inc),
            p.wrapping_add(phase_inc.wrapping_mul(2)),
            p.wrapping_add(phase_inc.wrapping_mul(3)),
        ]);

        let lut_ptr = self.lut.as_ptr();
        let out_ptr = buffer.as_mut_ptr() as *mut u32;

        while i + 3 < num_samples {
            // Extract indices
            let v_idx = v_phase >> Simd::splat(self.lut_shift);
            let idxs = v_idx.to_array();

            unsafe {
                // Gather IQ pairs from LUT (scalar loads as A9 has no gather)
                let q0 = *lut_ptr.add(idxs[0] as usize & self.lut_mask as usize);
                let q1 = *lut_ptr.add(idxs[1] as usize & self.lut_mask as usize);
                let q2 = *lut_ptr.add(idxs[2] as usize & self.lut_mask as usize);
                let q3 = *lut_ptr.add(idxs[3] as usize & self.lut_mask as usize);

                // Store 4 IQ pairs
                let v_out = u32x4::from_array([q0, q1, q2, q3]);
                v_out.copy_to_slice(std::slice::from_raw_parts_mut(out_ptr.add(i), 4));
            }

            v_phase += v_inc4;
            i += 4;
        }

        // Final phase update from the first lane of v_phase
        self.phase = v_phase.to_array()[0];

        // Remainder
        while i < num_samples {
            let idx = ((self.phase >> self.lut_shift) & self.lut_mask) as usize;
            unsafe {
                *out_ptr.add(i) = *lut_ptr.add(idx);
            }
            self.phase = self.phase.wrapping_add(phase_inc);
            i += 1;
        }
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
