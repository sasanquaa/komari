use std::cell::RefCell;

use noise::{NoiseFn, Perlin};
use rand::{Rng as RandRng, SeedableRng, rngs::StdRng};
use rand_distr::{
    Distribution, Normal,
    uniform::{SampleRange, SampleUniform},
};

pub type RngSeed = [u8; 32];

/// A wrapper around `StdRng`.
#[derive(Debug)]
pub struct Rng {
    inner: RefCell<StdRng>,
    perlin: Perlin,
    seed: RngSeed,
}

impl Rng {
    pub fn new(seed: RngSeed) -> Self {
        Self {
            inner: RefCell::new(StdRng::from_seed(seed)),
            perlin: Perlin::new(u32::from_le_bytes([seed[0], seed[1], seed[2], seed[3]])),
            seed,
        }
    }

    /// Retrieves the `RngSeed` used by this `Rng`.
    #[inline]
    pub fn seed(&self) -> &RngSeed {
        &self.seed
    }

    /// Returns true if Perlin noise at the given coordinates and tick exceeds the threshold.
    ///
    /// `threshold` is in the range `0..1` and used as a cut-off so that values in the top portion
    /// of the noise range will return true. For example, if `threshold` is `0.35`, then only
    /// values in the range `0.65..1` return true. Perlin noise has a distribution similar to
    /// a Normal distribution so `threshold` like `0.65` will more likely to return true
    /// than `0.35`.
    #[inline]
    pub fn random_perlin_bool(&self, x: i32, y: i32, tick: u64, threshold: f64) -> bool {
        let noise = self
            .perlin
            .get([x as f64 * 0.1, y as f64 * 0.1, tick as f64]);
        let norm = (noise + 1.0) / 2.0;
        norm >= 1.0 - threshold
    }

    #[inline]
    pub fn random_bool(&self, probability: f64) -> bool {
        self.inner.borrow_mut().random_bool(probability)
    }

    #[inline]
    pub fn random_range<T, R>(&self, range: R) -> T
    where
        T: SampleUniform,
        R: SampleRange<T>,
    {
        self.inner.borrow_mut().random_range(range)
    }

    /// Samples a random `(delay, tick count)` pair.
    ///
    /// The delay is sampled from a normal distribution with mean `mean_ms` and
    /// standard deviation `std_ms`. These two paramters are in milliseconds. The sampled
    /// delay milliseconds is then clamped to `(min_ms, max_ms)` range, divided by `tick_ms` and
    /// rounded to get the tick count.
    pub fn random_delay_tick_count(
        &self,
        mean_ms: f32,
        std_ms: f32,
        tick_ms: f32,
        min_ms: f32,
        max_ms: f32,
    ) -> (f32, u32) {
        debug_assert!(std_ms >= 0.0 && tick_ms > 0.0);

        let mut rng = self.inner.borrow_mut();
        let normal = Normal::new(mean_ms, std_ms).unwrap();
        let ms = normal.sample(&mut rng).max(min_ms).min(max_ms);
        let tick_count = (ms / tick_ms).round() as u32;
        (ms, tick_count)
    }

    /// Generates a pair of mean and standard deviation from the provided parameters using
    /// Ornstein-Uhlenbeck process.
    ///
    /// Delta time is 1.
    pub fn random_mean_std_pair(
        &self,
        base_mean: f32,
        current_mean: f32,
        base_std: f32,
        current_std: f32,
        reversion_rate: f32,
        volatility: f32,
    ) -> (f32, f32) {
        // I do not have enough authority to speak on the math. It seems cool and work so good
        // enough for me. Consult ChatGPT, DeepSeek, Claude, ... senseis for more details.
        let normal = Normal::new(0.0, 1.0).unwrap();

        let mut rng = self.inner.borrow_mut();
        let next_mean_normal_sample = normal.sample(&mut rng);
        let next_mean = current_mean
            + reversion_rate * (base_mean - current_mean)
            + volatility * next_mean_normal_sample;

        let next_std_normal_sample = normal.sample(&mut rng);
        let next_std = (current_std
            + reversion_rate * (base_std - current_std)
            + volatility * next_std_normal_sample)
            .abs();

        (next_mean, next_std)
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    const SEED: [u8; 32] = [
        64, 241, 206, 219, 49, 21, 218, 145, 254, 152, 68, 176, 242, 238, 152, 14, 176, 241, 153,
        64, 44, 192, 172, 191, 191, 157, 107, 206, 193, 55, 115, 68,
    ];

    #[test]
    fn random_tick_count_seeded() {
        let rng = Rng::new(SEED);
        let (_, count) =
            rng.random_delay_tick_count(83.99979, 28.149803, 1000.0 / 30.0, 80.0, 120.0);
        assert_eq!(count, 2);
    }

    #[test]
    fn random_mu_std_pair_seeded() {
        let rng = Rng::new(SEED);
        let (mean, std) = rng.random_mean_std_pair(85.0, 85.0, 30.0, 30.0, 0.05, 0.1);

        assert!(mean - 84.88451 < 0.01);
        assert!(std - 29.786358 < 0.01);
    }
}
