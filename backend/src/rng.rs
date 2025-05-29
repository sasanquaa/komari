use rand::{SeedableRng, rngs::StdRng};
use rand_distr::{Distribution, Normal};

pub type RngSeed = [u8; 32];

#[derive(Debug)]
pub struct Rng {
    inner: StdRng,
    seed: RngSeed,
}

impl Rng {
    pub fn new(seed: RngSeed) -> Self {
        Self {
            inner: StdRng::from_seed(seed),
            seed,
        }
    }

    /// Retrieves the `RngSeed` used by this `Rng`.
    pub fn seed(&self) -> &RngSeed {
        &self.seed
    }

    /// Samples a random `(delay, tick count)` pair.
    ///
    /// The delay is sampled from a normal distribution with mean `mean_ms` and
    /// standard deviation `std_ms`. These two paramters are in milliseconds. The sampled
    /// delay milliseconds is then clamped to `(min_ms, max_ms)` range, divided by `tick_ms` and
    /// rounded to get the tick count.
    pub fn random_delay_tick_count(
        &mut self,
        mean_ms: f32,
        std_ms: f32,
        tick_ms: f32,
        min_ms: f32,
        max_ms: f32,
    ) -> (f32, u32) {
        debug_assert!(std_ms > 0.0 && tick_ms > 0.0);

        let normal = Normal::new(mean_ms, std_ms).unwrap();
        let ms = normal.sample(&mut self.inner).max(min_ms).min(max_ms);
        let tick_count = (ms / tick_ms).round() as u32;
        (ms, tick_count)
    }

    /// Generates a pair of mean and standard deviation from the provided parameters using
    /// Ornstein-Uhlenbeck process.
    ///
    /// Delta time is 1.
    pub fn random_mean_std_pair(
        &mut self,
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

        let next_mean_normal_sample = normal.sample(&mut self.inner);
        let next_mean = current_mean
            + reversion_rate * (base_mean - current_mean)
            + volatility * next_mean_normal_sample;

        let next_std_normal_sample = normal.sample(&mut self.inner);
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
        let mut rng = Rng::new(SEED);
        let (_, count) =
            rng.random_delay_tick_count(83.99979, 28.149803, 1000.0 / 30.0, 80.0, 120.0);
        assert_eq!(count, 2);
    }

    #[test]
    fn random_mu_std_pair_seeded() {
        let mut rng = Rng::new(SEED);
        let (mean, std) = rng.random_mean_std_pair(85.0, 85.0, 30.0, 30.0, 0.05, 0.1);

        assert!(mean - 84.88451 < 0.01);
        assert!(std - 29.786358 < 0.01);
    }
}
