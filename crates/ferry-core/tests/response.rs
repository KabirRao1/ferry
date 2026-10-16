//! F06-AC1..AC3 formatting, F05-AC3/AC6 and F01-AC5 error messages.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::time::Duration;

use bytes::Bytes;
use ferry_core::error::SendError;
use ferry_core::format;
use ferry_core::response::Response;

fn response(status: u16, reason: &str) -> Response {
    Response {
        status,
        reason: reason.into(),
        headers: vec![],
        body: Bytes::new(),
        elapsed: Duration::ZERO,
    }
}

#[test]
fn f06_ac1_status_line() {
    assert_eq!(response(200, "OK").status_line(), "200 OK");
    assert_eq!(response(404, "Not Found").status_line(), "404 Not Found");
    assert_eq!(response(299, "").status_line(), "299");
}

#[test]
fn f06_ac3_sizes_in_binary_units() {
    let cases = [
        (0, "0 B"),
        (1, "1 B"),
        (1023, "1023 B"),
        (1024, "1.0 KB"),
        (1536, "1.5 KB"),
        (1024 * 1024 - 1, "1024.0 KB"),
        (1024 * 1024, "1.0 MB"),
        (50 * 1024 * 1024, "50.0 MB"),
        (3 * 1024 * 1024 * 1024 / 2, "1.5 GB"),
        (u64::MAX, "17179869184.0 GB"),
    ];
    for (bytes, want) in cases {
        assert_eq!(format::size(bytes), want, "{bytes}");
    }
}

#[test]
fn f06_ac2_durations() {
    let cases = [
        (Duration::ZERO, "0 ms"),
        (Duration::from_micros(400), "0 ms"),
        (Duration::from_micros(85_600), "86 ms"),
        (Duration::from_millis(999), "999 ms"),
        (Duration::from_millis(1000), "1.00 s"),
        (Duration::from_millis(1254), "1.25 s"),
        (Duration::from_secs(61), "61.00 s"),
    ];
    for (d, want) in cases {
        assert_eq!(format::duration(d), want, "{d:?}");
    }
}

#[test]
fn f05_ac3_seconds_without_trailing_zeros() {
    assert_eq!(format::seconds(Duration::from_secs(30)), "30");
    assert_eq!(format::seconds(Duration::from_millis(200)), "0.2");
    assert_eq!(format::seconds(Duration::from_millis(1500)), "1.5");
    assert_eq!(format::seconds(Duration::from_millis(1250)), "1.25");
    assert_eq!(format::seconds(Duration::ZERO), "0");
}

#[test]
fn f05_ac6_error_messages() {
    let errors = [
        SendError::InvalidUrl,
        SendError::Dns {
            host: "api.example.test".into(),
        },
        SendError::ConnectionRefused {
            host: "127.0.0.1".into(),
            port: 8080,
        },
        SendError::Tls {
            detail: "received corrupt message".into(),
        },
        SendError::Timeout {
            after: Duration::from_secs(30),
        },
        SendError::Timeout {
            after: Duration::from_millis(200),
        },
        SendError::Other {
            detail: "connection reset".into(),
        },
    ];
    let text: Vec<String> = errors.iter().map(ToString::to_string).collect();
    insta::assert_snapshot!(text.join("\n"));
}
