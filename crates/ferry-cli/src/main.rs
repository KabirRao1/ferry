//! `ferry-cli run`: run a collection or folder headless, for CI (F16-AC7,
//! ADR-0047). Script variable changes are never saved.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime};

use ferry_app::runner::{self, RunConfig, RunResult, RunStatus, Runner};
use ferry_app::{
    CancellationToken, Client, Config, Cookies, Finished, Jar, RealFs, TestStatus, Workspace,
    WorkspaceStore, datafile, execute_scripted, format, run_variables,
};

const USAGE: &str = "Usage: ferry-cli run <collection or folder> [options]

Runs the requests of a collection or folder in a Ferry workspace, with
their scripts and tests, and exits 0 when everything passed, 1 when a
request or test failed, and 2 for invalid arguments.

<collection or folder>   a path in the workspace, such as collections/api or api/users

Options:
  --workspace <dir>      the workspace folder (default: the current folder)
  --environment <name>   the environment to use, by name or file name
  --iterations <n>       how many times to run (1 to 1000, default 1)
  --data <file>          a CSV or JSON data file; one iteration per row
  --delay <ms>           wait between requests (0 to 60000, default 0)
  --bail                 stop at the first failure
  --report <file>        write a JSON report
";

/// Parsed command-line arguments.
#[derive(Debug, Default)]
struct Args {
    target: String,
    workspace: Option<PathBuf>,
    environment: Option<String>,
    iterations: Option<String>,
    data: Option<PathBuf>,
    delay: Option<String>,
    bail: bool,
    report: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("run") => {}
        Some("--help" | "-h" | "help") | None => return Err(String::new()),
        Some(other) => return Err(format!("Unknown command \"{other}\"")),
    }
    let mut out = Args::default();
    let mut target = None;
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--workspace" => out.workspace = Some(value("--workspace")?.into()),
            "--environment" | "-e" => out.environment = Some(value("--environment")?),
            "--iterations" | "-n" => out.iterations = Some(value("--iterations")?),
            "--data" | "-d" => out.data = Some(value("--data")?.into()),
            "--delay" => out.delay = Some(value("--delay")?),
            "--bail" => out.bail = true,
            "--report" => out.report = Some(value("--report")?.into()),
            "--help" | "-h" => return Err(String::new()),
            flag if flag.starts_with('-') => return Err(format!("Unknown option \"{flag}\"")),
            path if target.is_none() => target = Some(path.to_owned()),
            extra => return Err(format!("Unexpected argument \"{extra}\"")),
        }
    }
    out.target = target.ok_or("Name a collection or folder to run")?;
    Ok(out)
}

/// The workspace path of `target`: as given, or under `collections/`.
fn find_target(workspace: &Workspace, target: &str) -> Option<String> {
    let target = target.replace('\\', "/");
    let target = target.trim_matches('/');
    [target.to_owned(), format!("collections/{target}")]
        .into_iter()
        .find(|path| {
            ferry_app::collection::find(&workspace.collections, path).is_some_and(|item| {
                matches!(
                    item.node,
                    ferry_app::Node::Collection(_) | ferry_app::Node::Folder(_)
                )
            })
        })
}

/// The environment file for a name, a file name or a path.
fn find_environment(workspace: &Workspace, wanted: &str) -> Option<String> {
    let lower = wanted.to_lowercase();
    workspace
        .environments
        .iter()
        .find(|e| {
            let file = ferry_app::collection::file_name(&e.path).to_lowercase();
            e.name().to_lowercase() == lower
                || file == lower
                || file.strip_suffix(".toml") == Some(lower.as_str())
                || e.path.to_lowercase() == lower.replace('\\', "/")
        })
        .map(|e| e.path.clone())
}

/// The runner for `args`, or why it cannot run.
fn prepare(args: &Args, workspace: &Workspace) -> Result<Runner, String> {
    let path = find_target(workspace, &args.target).ok_or_else(|| {
        format!(
            "No collection or folder \"{}\" in the workspace",
            args.target
        )
    })?;
    let environment = match &args.environment {
        Some(name) => Some(
            find_environment(workspace, name)
                .ok_or_else(|| format!("No environment named \"{name}\""))?,
        ),
        None => None,
    };
    let iterations = match &args.iterations {
        Some(text) => runner::parse_iterations(text).map_err(|e| format!("--iterations: {e}"))?,
        None => 1,
    };
    let delay = match &args.delay {
        Some(text) => runner::parse_delay(text).map_err(|e| format!("--delay: {e}"))?,
        None => Duration::ZERO,
    };
    let data = match &args.data {
        Some(file) => {
            let text = std::fs::read_to_string(file)
                .map_err(|e| format!("Could not read {}: {e}", file.display()))?;
            datafile::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?
        }
        None => Vec::new(),
    };
    let requests = runner::requests(workspace, &path);
    let link = requests.first().map(|r| r.path.clone());
    let (variables, targets) = run_variables(workspace, environment.as_deref(), link.as_deref());
    let name = ferry_app::collection::find(&workspace.collections, &path)
        .map_or_else(|| path.clone(), |item| item.name().to_owned());
    Ok(Runner::new(
        name,
        requests,
        RunConfig {
            iterations,
            delay,
            data,
            bail: args.bail,
        },
        variables,
        targets,
        SystemTime::now(),
    ))
}

/// `✓ GET Name 200 · 85 ms` and the failed tests under it.
fn print_result(r: &RunResult, iterations: usize) {
    let mark = if r.failed() { "✗" } else { "✓" };
    let iteration = if iterations > 1 {
        format!("[{}] ", r.iteration + 1)
    } else {
        String::new()
    };
    let status = r
        .status
        .map_or_else(|| "error".to_owned(), |s| s.to_string());
    println!(
        "{mark} {iteration}{} {} {status} · {}",
        r.method,
        r.name,
        format::duration(r.time)
    );
    if let Some(error) = &r.error {
        println!("    {error}");
    }
    for test in r.tests.iter().filter(|t| t.status == TestStatus::Failed) {
        match &test.error {
            Some(error) => println!("    ✗ {}: {error}", test.name),
            None => println!("    ✗ {}", test.name),
        }
    }
}

/// Keep the cookies a response set, as the app does (F17-AC1).
fn store_cookies(jar: &mut Jar, finished: &Finished, now: i64) {
    let Finished::Received(received) = finished else {
        return;
    };
    for cookie in &received.response.set_cookies {
        if let Ok(url) = url::Url::parse(&cookie.url)
            && let Some(host) = url.host_str()
        {
            let _ = jar.store(&cookie.header, host, url.path(), now);
        }
    }
}

fn now_ms() -> i64 {
    ferry_app::time::unix_millis(SystemTime::now())
}

async fn run(mut runner: Runner, report: Option<&Path>) -> Result<bool, String> {
    let client = Client::new(Config::default()).map_err(|e| e.to_string())?;
    let mut jar = Jar::default();
    let iterations = runner.iterations();
    let mut seed = u64::from(std::process::id());
    loop {
        let delay = runner.delay();
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let Some(send) = runner.next_send(SystemTime::now(), seed) else {
            break;
        };
        tokio::time::sleep(delay).await;
        let cookies = Cookies {
            jar: jar.clone(),
            now: now_ms(),
        };
        let (finished, script_report) =
            execute_scripted(send, CancellationToken::new(), cookies, client.clone()).await;
        store_cookies(&mut jar, &finished, now_ms());
        runner.record(&finished, &script_report, SystemTime::now());
        if let Some(result) = runner.results.last() {
            print_result(result, iterations);
        }
    }
    let now = SystemTime::now();
    for note in &runner.notes {
        println!("{note}");
    }
    let s = runner.summary(now);
    println!(
        "\n{} {} in {} iteration{}: {} failed. Tests: {} passed, {} failed, {} skipped. {}",
        s.requests,
        if s.requests == 1 {
            "request"
        } else {
            "requests"
        },
        s.iterations,
        if s.iterations == 1 { "" } else { "s" },
        s.failed_requests,
        s.passed,
        s.failed,
        s.skipped,
        format::duration(s.duration)
    );
    if let Some(path) = report {
        std::fs::write(path, runner.report(now))
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        println!("Report written to {}", path.display());
    }
    Ok(s.failed_requests == 0 && runner.status != RunStatus::Stopped)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(args) => args,
        Err(message) => {
            if message.is_empty() {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let root = match &args.workspace {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().unwrap_or_default(),
    };
    if !root.is_dir() {
        eprintln!("No workspace folder at {}", root.display());
        return ExitCode::from(2);
    }
    let workspace = WorkspaceStore::new(RealFs, &root).load();
    let runner = match prepare(&args, &workspace) {
        Ok(runner) => runner,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    else {
        eprintln!("Could not start the async runtime");
        return ExitCode::from(2);
    };
    match runtime.block_on(run(runner, args.report.as_deref())) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
