//! F01-AC4: URL/params sync never panics, and URL -> table -> URL is stable
//! after one normalization.
#![no_main]

use ferry_core::params::{rows_from_url, url_with_rows};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|url: &str| {
    let rows = rows_from_url(url, &[]);
    let once = url_with_rows(url, &rows);
    let rows_again = rows_from_url(&once, &rows);
    let twice = url_with_rows(&once, &rows_again);
    assert_eq!(once, twice);
});
