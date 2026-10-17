//! Calendar arithmetic without a time-zone database: UTC timestamps and
//! civil dates at a given UTC offset (F08-AC7, F10-AC3).

use std::time::{SystemTime, UNIX_EPOCH};

const MS_PER_DAY: i64 = 86_400_000;

// CONTRACT
/// A calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i64,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

// CONTRACT
/// Milliseconds since the Unix epoch; negative before it.
#[must_use]
pub fn unix_millis(at: SystemTime) -> i64 {
    match at.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => {
            // Round toward negative infinity, so 1 ms before the epoch is -1.
            let d = before.duration();
            let ms = i64::try_from(d.as_millis()).unwrap_or(i64::MAX);
            if d.as_nanos() % 1_000_000 == 0 {
                -ms
            } else {
                -ms - 1
            }
        }
    }
}

// CONTRACT
/// ISO 8601 in UTC with milliseconds, for example `2026-10-17T19:07:20.123Z`.
#[must_use]
pub fn iso8601_utc(at: SystemTime) -> String {
    let ms = unix_millis(at);
    let date = date_at(ms, 0);
    let in_day = ms.rem_euclid(MS_PER_DAY);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        date.year,
        date.month,
        date.day,
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000
    )
}

// CONTRACT
/// The date at `unix_millis`, seen at `offset_seconds` east of UTC.
#[must_use]
pub fn date_at(unix_millis: i64, offset_seconds: i32) -> Date {
    let local = unix_millis.saturating_add(i64::from(offset_seconds) * 1000);
    civil_from_days(local.div_euclid(MS_PER_DAY))
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a
/// proleptic Gregorian date.
fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    // `month` is 1..=12 and `day` 1..=31 by construction.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Date {
        year,
        month: month as u8,
        day: day as u8,
    }
}

// CONTRACT
/// English month name, `January` to `December`; empty outside 1 to 12.
#[must_use]
pub fn month_name(month: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    month
        .checked_sub(1)
        .and_then(|i| NAMES.get(usize::from(i)))
        .copied()
        .unwrap_or("")
}
