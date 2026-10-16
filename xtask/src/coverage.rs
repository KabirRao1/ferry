//! F00-AC4: per-crate line coverage floors (kickoff section 5).

use serde_json::Value;

use crate::util::{self, Result};

/// Crate and minimum line coverage (percent). `ferry-ui` and `xtask` are excluded.
pub const FLOORS: &[(&str, f64)] = &[
    ("ferry-core", 90.0),
    ("ferry-app", 90.0),
    ("ferry-store", 90.0),
    ("ferry-http", 80.0),
];

pub fn run() -> Result {
    // Needed only for coverage, so it is not in rust-toolchain.toml (keeps first builds lean).
    util::run(
        "rustup component add llvm-tools-preview",
        std::process::Command::new("rustup")
            .current_dir(util::root())
            .args(["component", "add", "llvm-tools-preview"]),
    )?;
    let out = util::root().join("target").join("llvm-cov-summary.json");
    util::run(
        "cargo llvm-cov",
        util::cargo()
            .args([
                "llvm-cov",
                "nextest",
                "--workspace",
                "--exclude",
                "ferry-ui",
                "--exclude",
                "xtask",
                "--json",
                "--summary-only",
                "--output-path",
            ])
            .arg(&out),
    )?;
    let text = std::fs::read_to_string(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| format!("llvm-cov json: {e}"))?;
    let failures = check(&json)?;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Print per-crate coverage and return floor violations.
pub fn check(json: &Value) -> Result<Vec<String>> {
    let files = json["data"][0]["files"]
        .as_array()
        .ok_or("llvm-cov json: missing data[0].files")?;
    let mut failures = Vec::new();
    for (krate, floor) in FLOORS {
        let needle = format!("crates/{krate}/src/");
        let (mut count, mut covered) = (0u64, 0u64);
        for f in files {
            let name = f["filename"].as_str().unwrap_or("").replace('\\', "/");
            if name.contains(&needle) {
                count += f["summary"]["lines"]["count"].as_u64().unwrap_or(0);
                covered += f["summary"]["lines"]["covered"].as_u64().unwrap_or(0);
            }
        }
        if count == 0 {
            println!("coverage: {krate:<12} no instrumented lines yet (floor {floor}%)");
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let pct = covered as f64 * 100.0 / count as f64;
        let verdict = if pct >= *floor { "ok" } else { "FAIL" };
        println!("coverage: {krate:<12} {pct:6.2}% of {count} lines (floor {floor}%) {verdict}");
        if pct < *floor {
            failures.push(format!("{krate} line coverage {pct:.2}% is below {floor}%"));
        }
    }
    Ok(failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(name: &str, count: u64, covered: u64) -> Value {
        json!({"filename": name, "summary": {"lines": {"count": count, "covered": covered}}})
    }

    #[test]
    fn f00_ac4_floors_are_checked_per_crate() {
        let j = json!({"data": [{"files": [
            file(r"C:\dev\ferry\crates\ferry-core\src\text.rs", 100, 95),
            file("/w/crates/ferry-app/src/lib.rs", 100, 89),
            file("/w/crates/ferry-http/src/lib.rs", 10, 8),
            file("/w/crates/ferry-core/tests/t.rs", 100, 0),
        ]}]});
        assert_eq!(
            check(&j).unwrap(),
            vec!["ferry-app line coverage 89.00% is below 90%"]
        );
    }

    #[test]
    fn f00_ac4_crates_without_lines_pass() {
        let j = json!({"data": [{"files": []}]});
        assert!(check(&j).unwrap().is_empty());
    }

    #[test]
    fn f00_ac4_malformed_json_is_an_error() {
        assert!(check(&json!({})).is_err());
    }
}
