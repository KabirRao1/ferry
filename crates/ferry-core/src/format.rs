//! Human-readable formatting of sizes and durations (F06-AC2, F06-AC3).

use std::time::Duration;

// CONTRACT
/// Byte count in binary units: `512 B`, `1.5 KB`, `50.0 MB`, `2.0 GB`.
#[must_use]
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // Display only: f64 precision loss above 2^53 bytes is irrelevant.
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

// CONTRACT
/// Elapsed time: whole milliseconds below one second (`85 ms`), otherwise
/// seconds with two decimals (`1.25 s`).
#[must_use]
pub fn duration(elapsed: Duration) -> String {
    if elapsed < Duration::from_secs(1) {
        format!("{} ms", (elapsed.as_secs_f64() * 1000.0).round())
    } else {
        format!("{:.2} s", elapsed.as_secs_f64())
    }
}

// CONTRACT
/// Seconds without trailing zeros: `30`, `0.2`, `1.5`.
#[must_use]
pub fn seconds(d: Duration) -> String {
    let text = format!("{:.3}", d.as_secs_f64());
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
