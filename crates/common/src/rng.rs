//! Deterministic PCG32 random number generator (O'Neill, pcg-random.org).
//!
//! Everything that must be reproducible (training, track generation, sensor
//! noise) draws from one of these, seeded explicitly. Never use OS randomness
//! inside the sim.

use std::f64::consts::TAU;

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    /// Create a generator from a seed. Equivalent to `with_stream(seed, 0)`.
    pub fn new(seed: u64) -> Self {
        Self::with_stream(seed, 0)
    }

    /// Create a generator from a seed and a stream id. Different streams with
    /// the same seed give independent sequences.
    pub fn with_stream(seed: u64, stream: u64) -> Self {
        let mut rng = Self { state: 0, inc: (stream << 1) | 1 };
        rng.step();
        rng.state = rng.state.wrapping_add(seed);
        rng.step();
        rng
    }

    /// Derive an independent child generator, e.g. one per episode, without
    /// disturbing other users of the parent beyond this one draw.
    pub fn fork(&mut self, stream: u64) -> Self {
        Self::with_stream(self.next_u64(), stream)
    }

    fn step(&mut self) {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(self.inc);
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn next_u64(&mut self) -> u64 {
        (u64::from(self.next_u32()) << 32) | u64::from(self.next_u32())
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        // 53 random bits -> exact multiple of 2^-53.
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform integer in [0, n). Uses rejection to avoid modulo bias.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "below(0)");
        let threshold = n.wrapping_neg() % n;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % n;
            }
        }
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }

    /// Standard normal sample (mean 0, std dev 1), via Box-Muller.
    pub fn gaussian(&mut self) -> f64 {
        // 1 - u keeps the log argument in (0, 1].
        let u1 = 1.0 - self.next_f64();
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pcg32_reference_output() {
        // First outputs of the reference pcg32-demo, seeded with (42, 54).
        let mut rng = Rng::with_stream(42, 54);
        let expected = [0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e];
        for want in expected {
            assert_eq!(rng.next_u32(), want);
        }
    }

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn streams_differ() {
        let mut a = Rng::with_stream(7, 1);
        let mut b = Rng::with_stream(7, 2);
        let same = (0..32).filter(|_| a.next_u32() == b.next_u32()).count();
        assert!(same < 2);
    }

    #[test]
    fn f64_in_unit_range() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            let x = rng.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn below_in_range() {
        let mut rng = Rng::new(3);
        for _ in 0..10_000 {
            assert!(rng.below(7) < 7);
        }
    }

    #[test]
    fn gaussian_moments() {
        let mut rng = Rng::new(99);
        let n = 100_000;
        let samples: Vec<f64> = (0..n).map(|_| rng.gaussian()).collect();
        let mean = samples.iter().sum::<f64>() / n as f64;
        let var = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.02, "mean {mean}");
        assert!((var - 1.0).abs() < 0.02, "var {var}");
    }
}
