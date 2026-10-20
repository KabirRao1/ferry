//! The injected clock, randomness and time zone (F08-AC7, F10-AC3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_app::{Ambient, Clock, FixedAmbient, SystemAmbient};

#[test]
fn f08_ac7_the_system_ambient_reads_the_real_world() {
    let mut ambient = SystemAmbient::default();
    let before = SystemTime::now();
    let now = ambient.now();
    let after = SystemTime::now();
    assert!(before <= now && now <= after);
    // Time zones run from UTC-12 to UTC+14.
    let offset = ambient.utc_offset();
    assert!((-12 * 3600..=14 * 3600).contains(&offset), "{offset}");
    let values: HashSet<u64> = (0..64).map(|_| ambient.random()).collect();
    assert!(values.len() > 60, "random values repeat: {values:?}");
    assert_eq!(format!("{ambient:?}"), "SystemAmbient");
}

#[test]
fn f08_ac7_the_fixed_ambient_is_predictable() {
    let at = UNIX_EPOCH + Duration::from_secs(1_792_264_040);
    let mut a = FixedAmbient::new(at, -18_000);
    let mut b = a.clone();
    assert_eq!(a.now(), at);
    assert_eq!(a.utc_offset(), -18_000);
    // SplitMix64 from zero.
    assert_eq!(a.random(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(b.random(), 0xE220_A839_7B1D_CDAF);
    let values: HashSet<u64> = (0..1000).map(|_| a.random()).collect();
    assert_eq!(values.len(), 1000);
}
