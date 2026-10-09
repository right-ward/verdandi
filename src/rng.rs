//! Tiny deterministic PRNG (SplitMix64). No dependencies, same output on every platform.

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    /// Uniform integer in [lo, hi). Returns `lo` if the range is empty.
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % u64::from(hi - lo)) as u32
    }

    /// Uniform pick from a non-empty slice (panics on an empty slice).
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.range(0, items.len() as u32) as usize]
    }

    /// Index chosen with probability proportional to `weights`.
    pub fn weighted(&mut self, weights: &[f32]) -> usize {
        let total: f32 = weights.iter().sum();
        let mut roll = self.next_f32() * total;
        for (i, w) in weights.iter().enumerate() {
            if roll < *w {
                return i;
            }
            roll -= *w;
        }
        weights.len().saturating_sub(1)
    }
}

/// Derive an independent sub-seed from a master seed, a purpose tag and an index.
/// This is what keeps each layer/loop stream independent but reproducible.
pub fn derive_seed(master: u64, tag: u64, index: u64) -> u64 {
    let a = Rng::new(master ^ tag.wrapping_mul(0xD6E8_FEB8_6659_FD93)).next_u64();
    Rng::new(a ^ index.wrapping_mul(0xA076_1D64_78BD_642F)).next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = Rng::new(123);
        let mut b = Rng::new(123);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_and_float_bounds() {
        let mut r = Rng::new(9);
        for _ in 0..1000 {
            let v = r.range(3, 7);
            assert!((3..7).contains(&v));
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }

    #[test]
    fn derived_seeds_differ() {
        assert_ne!(derive_seed(1, 2, 3), derive_seed(1, 2, 4));
        assert_ne!(derive_seed(1, 2, 3), derive_seed(1, 3, 3));
    }
}
