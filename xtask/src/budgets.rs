//! F00-AC3: performance budgets as a gate (non-negotiable 6).
//!
//! Measures release builds of `ferry` and the `empty_window` baseline:
//! cold start (spawn to first frame), idle RSS after settling, binary size,
//! and real-window frame time while scrolling a 50 MB body.
//! In CI, timing budgets are advisory (shared runners, often no GPU; ADR-0005).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::util::{self, Result};

const MARKER: &str = "FERRY_PROBE interactive";
/// Tabs open while measuring idle RSS.
const PROBE_TABS: usize = 10;
const RUNS: usize = 5;
const SETTLE: Duration = Duration::from_millis(1500);
const START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    pub calibration: Calibration,
    /// Per-OS measurements (`std::env::consts::OS`): sizes and timings differ by platform.
    pub baseline: BTreeMap<String, Baseline>,
    pub budgets: Budgets,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Calibration {
    pub date: String,
    pub machine: String,
    pub method: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Baseline {
    pub empty_window_rss_mb: f64,
    pub empty_window_cold_start_ms: f64,
    pub binary_size_bytes: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Budgets {
    pub idle_rss_ratio_max: f64,
    pub cold_start_ms_max: f64,
    pub scroll_frame_p95_ms_max: f64,
    pub binary_size_regression_max: f64,
}

#[derive(Debug, Default)]
struct Measured {
    ferry_rss_mb: f64,
    ferry_cold_start_ms: f64,
    empty_rss_mb: f64,
    empty_cold_start_ms: f64,
    binary_size_bytes: u64,
    scroll_frame_p95_ms: f64,
}

pub fn run(ci: bool, record: bool) -> Result {
    let path = util::root().join("budgets.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut file: File = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;

    // `ferry` on its own, as the release workflow builds it: with the
    // examples, dev-dependencies (`iced_test`) would add features, such as
    // an embedded font, that the shipped binary lacks (FND-018).
    util::run(
        "build release",
        util::cargo().args(["build", "--release", "-p", "ferry-ui", "--bin", "ferry"]),
    )?;
    let release = util::root().join("target").join("release");
    let suffix = std::env::consts::EXE_SUFFIX;
    let ferry = release.join(format!("ferry{suffix}"));
    // Measured before the examples build, which never relinks `ferry`.
    let binary_size_bytes = std::fs::metadata(&ferry)
        .map_err(|e| format!("{}: {e}", ferry.display()))?
        .len();
    util::run(
        "build release examples",
        util::cargo().args([
            "build",
            "--release",
            "-p",
            "ferry-ui",
            "--example",
            "empty_window",
            "--example",
            "scroll_probe",
        ]),
    )?;
    let empty = release
        .join("examples")
        .join(format!("empty_window{suffix}"));
    let scroll = release
        .join("examples")
        .join(format!("scroll_probe{suffix}"));

    let mut m = Measured {
        binary_size_bytes,
        ..Measured::default()
    };
    (m.empty_cold_start_ms, m.empty_rss_mb) = probe_startup(&empty)?;
    (m.ferry_cold_start_ms, m.ferry_rss_mb) = probe_startup(&ferry)?;
    m.scroll_frame_p95_ms = probe_scroll(&scroll)?;

    if record {
        file.baseline.insert(
            std::env::consts::OS.to_owned(),
            Baseline {
                empty_window_rss_mb: round2(m.empty_rss_mb),
                empty_window_cold_start_ms: round2(m.empty_cold_start_ms),
                binary_size_bytes: m.binary_size_bytes,
            },
        );
        file.calibration.date = today();
        let out = toml::to_string_pretty(&file).map_err(|e| e.to_string())?;
        std::fs::write(&path, out).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("budgets: recorded baseline in {}", path.display());
    }

    report(&file, &m, ci, std::env::consts::OS)
}

struct Row {
    name: &'static str,
    actual: String,
    limit: String,
    ok: bool,
    enforced_in_ci: bool,
}

fn report(file: &File, m: &Measured, ci: bool, os: &str) -> Result {
    let b = &file.budgets;
    let rss_ratio = m.ferry_rss_mb / m.empty_rss_mb;
    let size_row = match file.baseline.get(os) {
        Some(base) => {
            #[allow(clippy::cast_precision_loss)]
            let growth = m.binary_size_bytes as f64 / base.binary_size_bytes as f64 - 1.0;
            Row {
                name: "release binary size",
                actual: format!(
                    "{:.2} MB ({:+.1}%)",
                    mb(m.binary_size_bytes),
                    growth * 100.0
                ),
                limit: format!(
                    "<= +{:.0}% of {:.2} MB",
                    b.binary_size_regression_max * 100.0,
                    mb(base.binary_size_bytes)
                ),
                ok: size_limit_ok(
                    m.binary_size_bytes,
                    base.binary_size_bytes,
                    b.binary_size_regression_max,
                ),
                enforced_in_ci: true,
            }
        }
        None => {
            println!(
                "budgets: no {os} baseline in budgets.toml; run `cargo xtask budgets --record` on {os}"
            );
            Row {
                name: "release binary size",
                actual: format!("{:.2} MB", mb(m.binary_size_bytes)),
                limit: format!("no {os} baseline"),
                ok: true,
                enforced_in_ci: true,
            }
        }
    };
    let rows = [
        Row {
            name: "idle RSS, 10 tabs / empty",
            actual: format!(
                "{rss_ratio:.2}x ({:.1} MB / {:.1} MB)",
                m.ferry_rss_mb, m.empty_rss_mb
            ),
            limit: format!("<= {:.2}x", b.idle_rss_ratio_max),
            ok: rss_ratio <= b.idle_rss_ratio_max,
            enforced_in_ci: true,
        },
        Row {
            name: "cold start to first frame",
            actual: format!(
                "{:.0} ms (empty window {:.0} ms)",
                m.ferry_cold_start_ms, m.empty_cold_start_ms
            ),
            limit: format!("<= {:.0} ms", b.cold_start_ms_max),
            ok: m.ferry_cold_start_ms <= b.cold_start_ms_max,
            enforced_in_ci: false,
        },
        Row {
            name: "scroll 50 MB body, frame p95",
            actual: format!("{:.2} ms", m.scroll_frame_p95_ms),
            limit: format!("<= {:.1} ms", b.scroll_frame_p95_ms_max),
            ok: m.scroll_frame_p95_ms <= b.scroll_frame_p95_ms_max,
            enforced_in_ci: false,
        },
        size_row,
    ];
    let mut failed = 0;
    println!("\n{:<30} {:<38} {:<24} result", "budget", "actual", "limit");
    for row in rows {
        let verdict = match (row.ok, row.enforced_in_ci || !ci) {
            (true, _) => "ok",
            (false, true) => {
                failed += 1;
                "FAIL"
            }
            (false, false) => "over (advisory in CI)",
        };
        println!(
            "{:<30} {:<38} {:<24} {verdict}",
            row.name, row.actual, row.limit
        );
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(format!("{failed} budget(s) exceeded"))
    }
}

/// Whether `size` is within `max_growth` (a fraction) of `baseline`.
/// Compares against the scaled limit so exactly +10% is not a float-rounding failure.
#[allow(clippy::cast_precision_loss)]
fn size_limit_ok(size: u64, baseline: u64, max_growth: f64) -> bool {
    size as f64 <= baseline as f64 * (1.0 + max_growth)
}

/// Median cold start (ms) and median idle RSS (MB) over [`RUNS`] launches.
fn probe_startup(exe: &Path) -> Result<(f64, f64)> {
    let mut starts = Vec::new();
    let mut rss = Vec::new();
    for _ in 0..RUNS {
        let (start, mem) = launch_once(exe)?;
        starts.push(start);
        rss.push(mem);
    }
    Ok((median(&mut starts), median(&mut rss)))
}

fn launch_once(exe: &Path) -> Result<(f64, f64)> {
    let t0 = Instant::now();
    let mut child = Command::new(exe)
        .env("FERRY_PROBE", "1")
        // The kickoff budget: idle RSS with 10 tabs open (the empty window ignores it).
        .env("FERRY_PROBE_TABS", PROBE_TABS.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", exe.display()))?;
    // Keep stderr so a failed launch reports why (for example, missing display libraries).
    let stderr = child.stderr.take().map(|mut err| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = std::io::Read::read_to_string(&mut err, &mut text);
            text
        })
    });
    let result = wait_for_marker(&mut child, t0).and_then(|cold| {
        // Measurement settle time, not test synchronization.
        std::thread::sleep(SETTLE);
        Ok((cold, rss_mb(child.id())?))
    });
    let _ = child.kill();
    let _ = child.wait();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    result.map_err(|e| format!("{}: {e}{}", exe.display(), stderr_tail(&stderr)))
}

/// The last lines of a child's stderr, formatted for an error message.
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return String::new();
    }
    let tail = &lines[lines.len().saturating_sub(15)..];
    format!(
        "
stderr (last {} lines):
  {}",
        tail.len(),
        tail.join(
            "
  "
        )
    )
}

fn wait_for_marker(child: &mut Child, t0: Instant) -> Result<f64> {
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            match line {
                Ok(l) if l.trim() == MARKER => {
                    let _ = tx.send(t0.elapsed());
                    return;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
    let elapsed = rx
        .recv_timeout(START_TIMEOUT)
        .map_err(|_| format!("no `{MARKER}` line within {START_TIMEOUT:?}"))?;
    Ok(elapsed.as_secs_f64() * 1e3)
}

fn rss_mb(pid: u32) -> Result<f64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let process = sys
        .process(pid)
        .ok_or("process exited before memory was sampled")?;
    #[allow(clippy::cast_precision_loss)]
    Ok(process.memory() as f64 / (1024.0 * 1024.0))
}

fn probe_scroll(exe: &Path) -> Result<f64> {
    let out = util::output(
        "scroll probe",
        Command::new(exe).env("ICED_PRESENT_MODE", "immediate"),
    )?;
    let line = out
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .ok_or("scroll probe printed no JSON")?;
    let v: serde_json::Value =
        serde_json::from_str(line).map_err(|e| format!("scroll probe: {e}"))?;
    v["frame_p95_ms"]
        .as_f64()
        .ok_or_else(|| "scroll probe: missing frame_p95_ms".into())
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

#[allow(clippy::cast_precision_loss)]
fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Today's UTC date as `YYYY-MM-DD`.
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f00_ac3_civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_740), (2026, 10, 14));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn f00_ac3_median_of_odd_and_empty() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert!(median(&mut []).is_nan());
    }

    fn file(baseline_size: u64) -> File {
        File {
            calibration: Calibration {
                date: String::new(),
                machine: String::new(),
                method: String::new(),
            },
            baseline: BTreeMap::from([(
                "testos".to_owned(),
                Baseline {
                    empty_window_rss_mb: 50.0,
                    empty_window_cold_start_ms: 100.0,
                    binary_size_bytes: baseline_size,
                },
            )]),
            budgets: Budgets {
                idle_rss_ratio_max: 2.0,
                cold_start_ms_max: 400.0,
                scroll_frame_p95_ms_max: 16.0,
                binary_size_regression_max: 0.10,
            },
        }
    }

    fn measured() -> Measured {
        Measured {
            ferry_rss_mb: 60.0,
            ferry_cold_start_ms: 200.0,
            empty_rss_mb: 50.0,
            empty_cold_start_ms: 150.0,
            binary_size_bytes: 1000,
            scroll_frame_p95_ms: 2.0,
        }
    }

    #[test]
    fn f00_ac3_within_budgets_passes() {
        assert!(report(&file(1000), &measured(), false, "testos").is_ok());
    }

    #[test]
    fn f00_ac3_rss_regression_fails_everywhere() {
        let m = Measured {
            ferry_rss_mb: 101.0,
            ..measured()
        };
        assert!(report(&file(1000), &m, false, "testos").is_err());
        assert!(report(&file(1000), &m, true, "testos").is_err());
    }

    #[test]
    fn f00_ac3_binary_growth_over_10_percent_fails() {
        let m = Measured {
            binary_size_bytes: 1101,
            ..measured()
        };
        assert!(report(&file(1000), &m, true, "testos").is_err());
        let m = Measured {
            binary_size_bytes: 1100,
            ..measured()
        };
        assert!(report(&file(1000), &m, true, "testos").is_ok());
    }

    #[test]
    fn f00_ac3_timing_budgets_fail_locally_but_are_advisory_in_ci() {
        let m = Measured {
            ferry_cold_start_ms: 401.0,
            scroll_frame_p95_ms: 17.0,
            ..measured()
        };
        assert!(report(&file(1000), &m, false, "testos").is_err());
        assert!(report(&file(1000), &m, true, "testos").is_ok());
    }

    #[test]
    fn f00_ac3_missing_os_baseline_does_not_fail_size() {
        let m = Measured {
            binary_size_bytes: 10_000,
            ..measured()
        };
        assert!(report(&file(1000), &m, true, "otheros").is_ok());
        assert!(report(&file(1000), &m, true, "testos").is_err());
    }

    #[test]
    fn f00_ac3_stderr_tail_keeps_last_lines() {
        assert_eq!(stderr_tail(""), "");
        assert_eq!(
            stderr_tail(
                "
 
"
            ),
            ""
        );
        let many: String = (0..20)
            .map(|i| {
                format!(
                    "line {i}
"
                )
            })
            .collect();
        let tail = stderr_tail(&many);
        assert!(tail.starts_with(
            "
stderr (last 15 lines):
  line 5
"
        ));
        assert!(tail.ends_with("line 19"));
    }
}
