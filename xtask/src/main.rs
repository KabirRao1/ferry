//! `cargo xtask <command>`: Ferry's gate, budgets and CI helpers.

mod arch;
mod budgets;
mod coverage;
mod gate;
mod util;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  gate [--ci] [--skip-mutants]   run the full merge gate (section 6 of docs/kickoff.md)
  arch                           check crate dependency direction
  budgets [--ci] [--record]      measure performance budgets against budgets.toml
  coverage                       run coverage and check per-crate floors
  mutants [--all] [--base REF]   mutation-test changed code in core, app, store
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let ci = flag("--ci") || std::env::var_os("CI").is_some();
    let result = match args.first().map(String::as_str) {
        Some("gate") => gate::run(ci, flag("--skip-mutants")),
        Some("arch") => arch::run(),
        Some("budgets") => budgets::run(ci, flag("--record")),
        Some("coverage") => coverage::run(),
        Some("mutants") => {
            let base = args
                .windows(2)
                .find(|w| w[0] == "--base")
                .map(|w| w[1].clone());
            gate::mutants(flag("--all"), base.as_deref())
        }
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
