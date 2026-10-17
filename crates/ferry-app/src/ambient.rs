//! The clock, randomness and time zone the app reads, injected so every
//! flow is deterministic in tests (F08-AC7, F10-AC3).

use std::fmt;
use std::time::SystemTime;

pub use ferry_core::variables::Ambient as Clock;

// CONTRACT
/// Everything `update()` may ask the outside world.
pub trait Ambient: Clock + Send + fmt::Debug {
    /// Seconds east of UTC, for local dates (F10-AC3).
    fn utc_offset(&self) -> i32;
}

// CONTRACT
/// The real clock, OS randomness and the local time zone.
#[derive(Default)]
pub struct SystemAmbient {
    rng: fastrand::Rng,
}

impl fmt::Debug for SystemAmbient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SystemAmbient")
    }
}

impl Clock for SystemAmbient {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn random(&mut self) -> u64 {
        self.rng.u64(..)
    }
}

impl Ambient for SystemAmbient {
    fn utc_offset(&self) -> i32 {
        chrono::Local::now().offset().local_minus_utc()
    }
}

// CONTRACT
/// A fixed clock and predictable randomness, for tests and the budget probe.
#[derive(Debug, Clone)]
pub struct FixedAmbient {
    pub now: SystemTime,
    pub offset: i32,
    next: u64,
}

impl FixedAmbient {
    #[must_use]
    pub fn new(now: SystemTime, offset: i32) -> Self {
        Self {
            now,
            offset,
            next: 0,
        }
    }
}

impl Clock for FixedAmbient {
    fn now(&self) -> SystemTime {
        self.now
    }

    fn random(&mut self) -> u64 {
        // SplitMix64: distinct, well-spread values in a fixed sequence.
        self.next = self.next.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.next;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Ambient for FixedAmbient {
    fn utc_offset(&self) -> i32 {
        self.offset
    }
}
