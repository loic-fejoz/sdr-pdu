pub struct Scrambler {
    state: u32,
    poly: u32,
}

impl Scrambler {
    pub const G3RUH_POLY: u32 = (1 << 11) | (1 << 16); // 0x10800
    pub const G3RUH_SEED: u32 = 0x1FFFF;

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

pub struct Descrambler {
    state: u32,
    poly: u32,
}

impl Descrambler {
    pub fn new(poly: u32, seed: u32) -> Self {
        Self { state: seed, poly }
    }

    pub fn descramble_bit(&mut self, bit: u8) -> u8 {
        let xor_sum = (self.state & self.poly).count_ones() % 2;
        let out_bit = (bit as u32 ^ xor_sum) as u8 & 1;
        self.state = (self.state << 1) | (bit as u32);
        out_bit
    }

    pub fn reset(&mut self, seed: u32) {
        self.state = seed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

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
    fn test_scrambler_g3ruh() {
        // G3RUH: bits 12 and 17 (1-indexed) -> bits 11 and 16 (0-indexed)
        let poly = Scrambler::G3RUH_POLY;
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
        // Bit 11 of state will be 1 on the next bit
        scr.scramble_bit(0);

        // Now bit 11 of state is 1.
        // Input 0: out = 0 ^ (ones(state & poly)%2) = 0 ^ 1 = 1.
        assert_eq!(scr.scramble_bit(0), 1);
    }
}
