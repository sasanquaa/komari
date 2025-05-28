use rand::{SeedableRng, rngs::StdRng};
use rand_distr::{Distribution, Normal};

use crate::array::Array;

const SPEED_OF_REVERSION: f32 = 0.2;
const VOLATILITY: f32 = 0.15;

pub type RngSeed = [u8; 32];

#[derive(Debug)]
pub struct Rng {
    inner: StdRng,
}

impl Rng {
    pub fn new(seed: RngSeed) -> Self {
        Self {
            inner: StdRng::from_seed(seed),
        }
    }

    /// Samples a random tick count from `mean_ms`, `std_ms` and `tick_ms`.
    pub fn random_tick_count(&mut self, mean_ms: f32, std_ms: f32, tick_ms: f32) -> u32 {
        debug_assert!(std_ms > 0.0 && tick_ms > 0.0);

        let normal = Normal::new(mean_ms, std_ms).unwrap();
        let ms = normal.sample(&mut self.inner);
        (ms / tick_ms) as u32
    }

    /// Generates `N` pairs of mean and standard deviation from `base_mean`, `base_std` and
    /// `delta_time`.
    pub fn random_mu_std_pairs<const N: usize>(
        &mut self,
        base_mean: f32,
        base_std: f32,
        delta_time: f32,
    ) -> Array<(f32, f32), N> {
        debug_assert!(N > 1 && delta_time > 0.0 && base_std > 0.0);

        let normal = Normal::new(0.0, 1.0).unwrap();
        let speed_mul_delta_time = SPEED_OF_REVERSION * delta_time;
        let volatility_mul_delta_time_sqrt = VOLATILITY * f32::sqrt(delta_time);
        let mut array = Array::new();
        array.push((base_mean, base_std));

        for i in 1..N {
            let (prev_mean, prev_std) = array[i - 1];

            let next_mean_normal_sample = normal.sample(&mut self.inner);
            let next_mean = prev_mean
                + speed_mul_delta_time * (base_mean - prev_mean)
                + volatility_mul_delta_time_sqrt * next_mean_normal_sample;

            let next_std_normal_sample = normal.sample(&mut self.inner);
            let next_std = (prev_std
                + speed_mul_delta_time * (base_std - prev_std)
                + volatility_mul_delta_time_sqrt * next_std_normal_sample)
                .abs();

            array.push((next_mean, next_std));
        }
        array
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
        let count = rng.random_tick_count(83.99979, 28.149803, 1000.0 / 30.0);
        assert_eq!(count, 1);
    }

    #[test]
    fn random_mu_std_pairs_seeded() {
        let mut rng = Rng::new(SEED);
        let pairs = rng.random_mu_std_pairs::<3>(85.0, 30.0, 1000.0 / 30.0);
        println!("{pairs:?}");

        assert!(pairs[0].0 - 85.0 < 0.01);
        assert!(pairs[0].1 - 30.0 < 0.01);

        assert!(pairs[1].0 - 83.99979 < 0.01);
        assert!(pairs[1].1 - 28.149803 < 0.01);

        assert!(pairs[2].0 - 91.042534 < 0.01);
        assert!(pairs[2].1 - 41.771946 < 0.01);
    }
}
