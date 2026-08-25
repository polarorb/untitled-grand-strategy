//! Deterministic randomness for the simulation.
//!
//! Hand-rolled PCG32 (no external dependency, no platform variance). All
//! gameplay randomness must come from the [`SimRng`] resource or from a
//! stream forked off it — never from `rand::thread_rng`, hashing, or time.
//!
//! Forked streams keep systems order-independent: a combat roll in Korea
//! must not perturb an election roll in Italy just because tick scheduling
//! reordered them. Fork one stream per subsystem (or per entity id) using a
//! stable label.

use bevy_ecs::prelude::Resource;
use serde::{Deserialize, Serialize};

#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct SimRng {
    state: u64,
    inc: u64,
}

impl SimRng {
    pub fn seeded(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Derive an independent stream for a subsystem. `label` must be a
    /// stable compile-time constant (e.g. `b"combat"`), never data that can
    /// vary between runs.
    pub fn fork(&mut self, label: &[u8]) -> Self {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
        for &b in label {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Self::seeded(self.next_u64() ^ h)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn next_u64(&mut self) -> u64 {
        ((self.next_u32() as u64) << 32) | self.next_u32() as u64
    }

    /// Uniform in `[0, n)` via Lemire's method (unbiased).
    pub fn below(&mut self, n: u32) -> u32 {
        debug_assert!(n > 0);
        let mut x = self.next_u32();
        let mut m = (x as u64).wrapping_mul(n as u64);
        let mut l = m as u32;
        if l < n {
            let t = n.wrapping_neg() % n;
            while l < t {
                x = self.next_u32();
                m = (x as u64).wrapping_mul(n as u64);
                l = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// True with probability `percent`/100.
    pub fn percent(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_is_in_range() {
        let mut rng = SimRng::seeded(1);
        for _ in 0..10_000 {
            assert!(rng.below(7) < 7);
        }
    }

    #[test]
    fn forked_streams_are_independent_and_deterministic() {
        let mut a = SimRng::seeded(9);
        let mut b = SimRng::seeded(9);
        let mut fa = a.fork(b"combat");
        let mut fb = b.fork(b"combat");
        for _ in 0..100 {
            assert_eq!(fa.next_u64(), fb.next_u64());
        }
        let mut other = SimRng::seeded(9).fork(b"elections");
        assert_ne!(
            other.next_u64(),
            SimRng::seeded(9).fork(b"combat").next_u64()
        );
    }
}
