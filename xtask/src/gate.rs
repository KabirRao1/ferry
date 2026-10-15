//! The merge gate (kickoff section 6). Every step must pass.

use std::process::Command;
use std::time::Instant;

use crate::util::{self, Result};
use crate::{arch, budgets, coverage};

/// Crates whose changed code is mutation-tested on every merge.
const MUTATED: &[&str] = &["ferry-core", "ferry-app", "ferry-store"];

pub fn run(ci: bool, skip_mutants: bool) -> Result {
    type Step<'a> = (&'a str, Box<dyn Fn() -> Result + 'a>);
    let steps: Vec<Step> = vec![
        (
            "fmt",
            Box::new(|| util::run("fmt", util::cargo().args(["fmt", "--all", "--check"]))),
        ),
        (
            "clippy",
            Box::new(|| {
                util::run(
                    "clippy",
                    util::cargo().args([
                        "clippy",
                        "--workspace",
                        "--all-targets",
                        "--all-features",
                        "--",
                        "-D",
                        "warnings",
                    ]),
                )
            }),
        ),
        (
            "nextest",
            Box::new(|| {
                util::run(
                    "nextest",
                    util::cargo().args(["nextest", "run", "--workspace"]),
                )
            }),
        ),
        (
            "doctests",
            Box::new(|| {
                util::run(
                    "doctests",
                    util::cargo().args(["test", "--doc", "--workspace"]),
                )
            }),
        ),
        ("coverage", Box::new(coverage::run)),
        (
            "deny",
            Box::new(|| {
                util::run(
                    "deny",
                    util::cargo().args(["deny", "--all-features", "check"]),
                )
            }),
        ),
        ("budgets", Box::new(move || budgets::run(ci, false))),
        ("arch", Box::new(arch::run)),
        (
            "mutants",
            Box::new(move || {
                if skip_mutants {
                    println!("mutants: skipped by --skip-mutants");
                    Ok(())
                } else {
                    mutants(false, None)
                }
            }),
        ),
    ];
    let mut summary = Vec::new();
    let mut failed = false;
    for (name, step) in &steps {
        println!("\n==> gate: {name}");
        let t = Instant::now();
        let result = step();
        let secs = t.elapsed().as_secs_f64();
        match &result {
            Ok(()) => summary.push(format!("  ok    {name:<10} {secs:6.1}s")),
            Err(e) => {
                failed = true;
                summary.push(format!("  FAIL  {name:<10} {secs:6.1}s  {e}"));
            }
        }
    }
    println!("\ngate summary:\n{}", summary.join("\n"));
    if failed {
        Err("gate failed".into())
    } else {
        Ok(())
    }
}

/// Mutation-test `ferry-core`, `ferry-app` and `ferry-store`: the whole crates
/// with `all`, otherwise only code changed relative to `base` (default:
/// `origin/main` if it exists, else `HEAD`). Untracked files are not in the diff.
pub fn mutants(all: bool, base: Option<&str>) -> Result {
    let mut cmd = util::cargo();
    cmd.args([
        "mutants",
        "--test-tool",
        "nextest",
        "--test-workspace",
        "true",
    ]);
    for krate in MUTATED {
        cmd.args(["--package", krate]);
    }
    if !all {
        let base = match base {
            Some(b) => b.to_owned(),
            None if git_ok(&["rev-parse", "--verify", "--quiet", "origin/main"]) => {
                "origin/main".into()
            }
            None => "HEAD".into(),
        };
        let diff = util::output(
            "git diff",
            Command::new("git").current_dir(util::root()).args([
                "diff",
                "--no-ext-diff",
                &base,
                "--",
            ]),
        )?;
        if diff.trim().is_empty() {
            println!("mutants: no changes against {base}; nothing to test");
            return Ok(());
        }
        let path = util::root().join("target").join("mutants.diff");
        std::fs::create_dir_all(util::root().join("target")).map_err(|e| e.to_string())?;
        std::fs::write(&path, diff).map_err(|e| format!("{}: {e}", path.display()))?;
        cmd.arg("--in-diff").arg(&path);
    }
    // cargo-mutants exits 3 when any mutant timed out, even if others were missed,
    // so the exit code alone is not trustworthy: read `missed.txt` (FND-005).
    let status = cmd
        .status()
        .map_err(|e| format!("cargo mutants: could not start: {e}"))?;
    let missed_path = util::root().join("mutants.out").join("missed.txt");
    let missed = std::fs::read_to_string(&missed_path).unwrap_or_default();
    verdict(status.code(), &missed)
}

/// Gate verdict from cargo-mutants' exit code and the contents of `missed.txt`.
/// A timeout means the mutation made tests hang, so it counts as caught (ADR-0010).
fn verdict(code: Option<i32>, missed: &str) -> Result {
    let missed: Vec<&str> = missed.lines().filter(|l| !l.trim().is_empty()).collect();
    if !missed.is_empty() {
        return Err(format!(
            "cargo mutants: {} missed mutant(s):
  {}",
            missed.len(),
            missed.join(
                "
  "
            )
        ));
    }
    match code {
        Some(0) => Ok(()),
        Some(3) => {
            println!(
                "mutants: some mutants timed out (counted as caught); see mutants.out/timeout.txt"
            );
            Ok(())
        }
        Some(2) => Err("cargo mutants: missed mutants; see mutants.out/missed.txt".into()),
        _ => Err(format!("cargo mutants: failed (exit code {code:?})")),
    }
}

fn git_ok(args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(util::root())
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(test)]
mod tests {
    use super::verdict;

    #[test]
    fn f00_ac5_missed_mutants_fail_even_when_exit_code_reports_timeouts() {
        let missed = "crates/ferry-core/src/text.rs:77:20: replace > with >= in LineIndex::line
";
        assert!(verdict(Some(3), missed).is_err());
        assert!(verdict(Some(2), missed).is_err());
    }

    #[test]
    fn f00_ac5_timeouts_only_pass() {
        assert!(verdict(Some(3), "").is_ok());
        assert!(
            verdict(
                Some(0),
                "
"
            )
            .is_ok()
        );
    }

    #[test]
    fn f00_ac5_other_failures_fail() {
        assert!(verdict(Some(1), "").is_err());
        assert!(verdict(None, "").is_err());
        assert!(verdict(Some(2), "").is_err());
    }
}
