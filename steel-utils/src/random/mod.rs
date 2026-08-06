//! This module contains utilities for random number generation.
use std::sync::atomic::{AtomicU64, Ordering};

use enum_dispatch::enum_dispatch;

use crate::random::{
    legacy_random::{LegacyRandom, LegacyRandomSplitter},
    name_hash::NameHash,
    xoroshiro::{Xoroshiro, XoroshiroSplitter},
};

/// This module contains the gaussian random number generator.
pub mod gaussian;
/// This module contains the legacy random number generator implementation.
pub mod legacy_random;
/// Precomputed name hashes for positional random seeding.
pub mod name_hash;
/// This module contains vanilla's feature-decoration `WorldgenRandom` wrapper.
pub mod worldgen_random;
/// This module contains the xoroshiro random number generator.
pub mod xoroshiro;

/// A trait for random number generators.
#[enum_dispatch]
#[expect(missing_docs, reason = "method names are self-explanatory")]
pub trait Random {
    #[must_use]
    fn fork(&mut self) -> Self;

    fn next_i32(&mut self) -> i32;

    fn next_i32_bounded(&mut self, bound: i32) -> i32;

    fn next_i32_between(&mut self, min: i32, max: i32) -> i32 {
        self.next_i32_bounded(max - min + 1) + min
    }

    fn next_i32_between_exclusive(&mut self, min: i32, max: i32) -> i32 {
        min + self.next_i32_bounded(max - min)
    }

    fn next_i64(&mut self) -> i64;

    fn next_f32(&mut self) -> f32;

    fn next_f64(&mut self) -> f64;

    fn next_bool(&mut self) -> bool;

    fn next_gaussian(&mut self) -> f64;

    fn triangle(&mut self, min: f64, max: f64) -> f64 {
        min + max * (self.next_f64() - self.next_f64())
    }

    fn triangle_f32(&mut self, min: f32, max: f32) -> f32 {
        min + max * (self.next_f32() - self.next_f32())
    }

    fn next_positional(&mut self) -> RandomSplitter;

    fn consume_count(&mut self, count: i32) {
        for _ in 0..count {
            self.next_i32();
        }
    }
}

/// A trait for positional random number generators.
#[enum_dispatch]
#[expect(missing_docs, reason = "method names are self-explanatory")]
pub trait PositionalRandom {
    fn at(&self, x: i32, y: i32, z: i32) -> RandomSource;

    fn with_hash_of(&self, hash: &NameHash) -> RandomSource;

    fn with_seed(&self, seed: u64) -> RandomSource;
}

/// A source of random numbers.
#[enum_dispatch(Random)]
pub enum RandomSource {
    /// A xoroshiro random number generator.
    Xoroshiro(Xoroshiro),
    /// A legacy Minecraft random number generator.
    Legacy(LegacyRandom),
}

impl RandomSource {
    /// Vanilla `RandomSource.createThreadSafe()`: a legacy LCG seeded with a
    /// fresh unique seed, used for live gameplay randomness such as a level's
    /// `random` field.
    #[must_use]
    pub fn create_thread_safe() -> Self {
        Self::Legacy(LegacyRandom::from_seed(generate_unique_seed() as u64))
    }
}

/// Vanilla `RandomSupport.generateUniqueSeed()`: mixes an atomic uniquifier
/// with the current time to seed independent runtime random sources.
#[must_use]
pub fn generate_unique_seed() -> i64 {
    static SEED_UNIQUIFIER: AtomicU64 = AtomicU64::new(8_682_522_807_148_012);
    const UNIQUIFIER_MULTIPLIER: u64 = 1_181_783_497_276_652_981;

    let uniquified = SEED_UNIQUIFIER.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |seed| {
        Some(seed.wrapping_mul(UNIQUIFIER_MULTIPLIER))
    });
    // `fetch_update` returns the previous value; apply the same multiply to get
    // the stored value, matching Java's `updateAndGet`.
    let uniquified = uniquified.map_or(
        SEED_UNIQUIFIER.load(Ordering::Relaxed),
        |previous| previous.wrapping_mul(UNIQUIFIER_MULTIPLIER),
    );

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as i64);
    (uniquified as i64) ^ nanos
}

/// A random number generator that can be split.
#[derive(Clone)]
#[enum_dispatch(PositionalRandom)]
pub enum RandomSplitter {
    /// A xoroshiro random number generator.
    Xoroshiro(XoroshiroSplitter),
    /// A legacy Minecraft random number generator splitter.
    Legacy(LegacyRandomSplitter),
}

/// Gets a seed from a position.
#[must_use]
pub fn get_seed(x: i32, y: i32, z: i32) -> i64 {
    let l = i64::from(x.wrapping_mul(3_129_871))
        ^ (i64::from(z).wrapping_mul(116_129_781_i64))
        ^ i64::from(y);
    let l = l
        .wrapping_mul(l)
        .wrapping_mul(42_317_861_i64)
        .wrapping_add(l.wrapping_mul(11));
    l >> 16
}
