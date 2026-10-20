//! The Collection Runner's loop (F16-AC2..AC6, ADR-0047): which request runs
//! next, with which variables and data row, and what each one did. It hands
//! out [`ScriptedSend`]s and records their results, so the app and
//! `ferry-cli` drive the same loop.

use std::time::{Duration, SystemTime};

use ferry_core::collection::{self, Item, Node};
use ferry_core::datafile::Row;
use ferry_core::draft::RequestDraft;
use ferry_core::time;
use ferry_core::variables::Variable;
use ferry_core::workspace::Workspace;
use ferry_script::{Info, Script, TestResult, TestStatus, Variables};
use serde::Serialize;

use crate::app::Finished;
use crate::scripting::{ScriptReport, ScriptedSend, Targets, chain};

// CONTRACT
pub const MAX_ITERATIONS: usize = 1000;
// CONTRACT
pub const MAX_DELAY_MS: u64 = 60_000;
// CONTRACT
/// Requests one iteration may run before it is stopped (F16-AC5).
pub const MAX_STEPS: usize = 10_000;
// CONTRACT
pub const ITERATIONS_ERROR: &str = "Enter a number from 1 to 1000";
// CONTRACT
pub const DELAY_ERROR: &str = "Enter a number from 0 to 60000";
// CONTRACT
pub const STOPPED: &str = "Run stopped";

// CONTRACT
/// `Iterations` as typed (F16-AC1).
///
/// # Errors
/// [`ITERATIONS_ERROR`] outside 1 to 1000.
pub fn parse_iterations(text: &str) -> Result<usize, &'static str> {
    text.trim()
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=MAX_ITERATIONS).contains(n))
        .ok_or(ITERATIONS_ERROR)
}

// CONTRACT
/// `Delay` in milliseconds as typed (F16-AC1).
///
/// # Errors
/// [`DELAY_ERROR`] outside 0 to 60000.
pub fn parse_delay(text: &str) -> Result<Duration, &'static str> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= MAX_DELAY_MS)
        .map(Duration::from_millis)
        .ok_or(DELAY_ERROR)
}

// CONTRACT
/// A request the runner can run: its saved draft, inherited auth filled
/// in, and its scripts chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub path: String,
    pub name: String,
    pub draft: RequestDraft,
    pub pre: Vec<Script>,
    pub post: Vec<Script>,
}

// CONTRACT
/// The readable requests inside the collection or folder at `path`, in
/// sidebar order, folders depth first (F16-AC1).
#[must_use]
pub fn requests(workspace: &Workspace, path: &str) -> Vec<RunRequest> {
    fn walk(workspace: &Workspace, items: &[Item], out: &mut Vec<RunRequest>) {
        for item in items {
            match &item.node {
                Node::Request(file) => {
                    let mut draft = file.request.clone();
                    if draft.auth.kind == ferry_core::auth::AuthKind::Inherit {
                        draft.auth = collection::inherited_auth(&workspace.collections, &item.path)
                            .map(|(auth, _)| auth)
                            .unwrap_or_default();
                    }
                    let (pre, post) = chain(workspace, Some(&item.path), &file.request.scripts);
                    out.push(RunRequest {
                        path: item.path.clone(),
                        name: file.name.clone(),
                        draft,
                        pre,
                        post,
                    });
                }
                Node::Collection(_) | Node::Folder(_) => walk(workspace, &item.items, out),
                Node::Broken { .. } => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Some(item) = collection::find(&workspace.collections, path) {
        walk(workspace, &item.items, &mut out);
    }
    out
}

// CONTRACT
/// How to run (F16-AC1, F16-AC3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunConfig {
    pub iterations: usize,
    pub delay: Duration,
    /// One row per iteration; replaces `iterations` when present.
    pub data: Vec<Row>,
    /// `Stop on first failure`.
    pub bail: bool,
}

// CONTRACT
/// What one request in the run did (F16-AC4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// From 0.
    pub iteration: usize,
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub time: Duration,
    /// A failed send or a failed script.
    pub error: Option<String>,
    pub tests: Vec<TestResult>,
}

impl RunResult {
    fn count(&self, status: TestStatus) -> usize {
        self.tests.iter().filter(|t| t.status == status).count()
    }

    // CONTRACT
    #[must_use]
    pub fn passed(&self) -> usize {
        self.count(TestStatus::Passed)
    }

    // CONTRACT
    #[must_use]
    pub fn failed_tests(&self) -> usize {
        self.count(TestStatus::Failed)
    }

    // CONTRACT
    /// A failed send, a script error or a failed test (F16-AC2).
    #[must_use]
    pub fn failed(&self) -> bool {
        self.error.is_some() || self.failed_tests() > 0
    }
}

// CONTRACT
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Running,
    Finished,
    Stopped,
}

// CONTRACT
/// Totals for the summary line (F16-AC4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub iterations: usize,
    pub requests: usize,
    pub failed_requests: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration: Duration,
}

// CONTRACT
/// A run in progress or done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    /// The collection's or folder's name.
    pub name: String,
    pub environment: Option<String>,
    requests: Vec<RunRequest>,
    config: RunConfig,
    variables: Variables,
    targets: Targets,
    iteration: usize,
    /// The request to run next, if any in this iteration.
    next: Option<usize>,
    /// The request in flight.
    current: Option<usize>,
    steps: usize,
    pub results: Vec<RunResult>,
    /// `No request named …`, `Stopped after …`, `Run stopped`.
    pub notes: Vec<String>,
    pub status: RunStatus,
    pub started: SystemTime,
    pub ended: Option<SystemTime>,
}

impl Runner {
    // CONTRACT
    /// A run of `requests` with `variables` as they are now; `targets`
    /// says where changed ones are saved.
    #[must_use]
    pub fn new(
        name: String,
        requests: Vec<RunRequest>,
        config: RunConfig,
        variables: Variables,
        targets: Targets,
        started: SystemTime,
    ) -> Self {
        let environment = variables.environment_name.clone();
        let empty = requests.is_empty();
        Self {
            name,
            environment,
            requests,
            config,
            variables,
            targets,
            iteration: 0,
            next: (!empty).then_some(0),
            current: None,
            steps: 0,
            results: Vec::new(),
            notes: Vec::new(),
            status: if empty {
                RunStatus::Finished
            } else {
                RunStatus::Running
            },
            started,
            ended: empty.then_some(started),
        }
    }

    // CONTRACT
    /// The number of iterations: the data rows, or the configured number.
    #[must_use]
    pub fn iterations(&self) -> usize {
        if self.config.data.is_empty() {
            self.config.iterations.max(1)
        } else {
            self.config.data.len()
        }
    }

    // CONTRACT
    /// The wait before the next send: none before the first (F16-AC2).
    #[must_use]
    pub fn delay(&self) -> Duration {
        if self.results.is_empty() {
            Duration::ZERO
        } else {
            self.config.delay
        }
    }

    // CONTRACT
    /// The next send, or `None` when the run is over.
    pub fn next_send(&mut self, now: SystemTime, seed: u64) -> Option<ScriptedSend> {
        if self.status != RunStatus::Running {
            return None;
        }
        let index = self.next?;
        self.current = Some(index);
        let request = &self.requests[index];
        let mut variables = self.variables.clone();
        variables.data = self
            .config
            .data
            .get(self.iteration)
            .map(|row| row.iter().map(|(k, v)| Variable::new(k, v)).collect())
            .unwrap_or_default();
        Some(ScriptedSend {
            draft: request.draft.clone(),
            pre: request.pre.clone(),
            post: request.post.clone(),
            variables,
            info: Info {
                request_name: request.name.clone(),
                iteration: self.iteration,
                iteration_count: self.iterations(),
            },
            targets: self.targets.clone(),
            now,
            seed,
        })
    }

    // CONTRACT
    /// Record how the send from [`Runner::next_send`] ended and choose the
    /// next request (F16-AC2, F16-AC5).
    pub fn record(&mut self, finished: &Finished, report: &ScriptReport, now: SystemTime) {
        let Some(index) = self.current.take() else {
            return;
        };
        let request = &self.requests[index];
        let (method, url) = match &report.sent {
            Some(sent) => (sent.method.clone(), sent.url.clone()),
            None => (request.draft.method.to_string(), request.draft.url.clone()),
        };
        let (status, time, send_error) = match finished {
            Finished::Received(r) => (Some(r.response.status), r.response.elapsed, None),
            Finished::Failed(error) => (None, Duration::ZERO, Some(error.clone())),
            Finished::Cancelled => {
                self.stop(now);
                return;
            }
        };
        let result = RunResult {
            iteration: self.iteration,
            name: request.name.clone(),
            method,
            url,
            status,
            time,
            error: send_error.or_else(|| report.error.clone()),
            tests: report.tests.clone(),
        };
        let failed = result.failed();
        self.results.push(result);
        let mut variables = report.variables.clone();
        variables.changed = ferry_script::Changed::default();
        variables.data = Vec::new();
        self.variables = variables;
        if failed && self.config.bail {
            self.end(RunStatus::Finished, now);
            return;
        }
        self.steps += 1;
        self.next = match &report.next_request {
            None => Some(index + 1).filter(|i| *i < self.requests.len()),
            Some(None) => None,
            Some(Some(name)) => {
                let found = self.requests.iter().position(|r| r.name == *name);
                if found.is_none() {
                    self.notes
                        .push(format!("No request named {name} in this run"));
                }
                found
            }
        };
        if self.next.is_some() && self.steps >= MAX_STEPS {
            self.notes.push(format!(
                "Stopped after {MAX_STEPS} requests in one iteration"
            ));
            self.next = None;
        }
        if self.next.is_none() {
            self.iteration += 1;
            self.steps = 0;
            if self.iteration < self.iterations() {
                self.next = Some(0);
            } else {
                self.end(RunStatus::Finished, now);
            }
        }
    }

    // CONTRACT
    /// End the run with [`STOPPED`] (F16-AC4).
    pub fn stop(&mut self, now: SystemTime) {
        if self.status == RunStatus::Running {
            self.notes.push(STOPPED.to_owned());
            self.end(RunStatus::Stopped, now);
        }
    }

    fn end(&mut self, status: RunStatus, now: SystemTime) {
        self.status = status;
        self.next = None;
        self.current = None;
        self.ended = Some(now);
    }

    // CONTRACT
    /// Totals so far (F16-AC4). `now` is used while the run is going.
    #[must_use]
    pub fn summary(&self, now: SystemTime) -> Summary {
        let end = self.ended.unwrap_or(now);
        let iterations = match self.status {
            RunStatus::Running => self.iteration,
            _ => self
                .results
                .last()
                .map_or(0, |r| r.iteration + 1)
                .min(self.iterations()),
        };
        Summary {
            iterations,
            requests: self.results.len(),
            failed_requests: self.results.iter().filter(|r| r.failed()).count(),
            passed: self.results.iter().map(RunResult::passed).sum(),
            failed: self.results.iter().map(RunResult::failed_tests).sum(),
            skipped: self
                .results
                .iter()
                .map(|r| r.count(TestStatus::Skipped))
                .sum(),
            duration: end.duration_since(self.started).unwrap_or_default(),
        }
    }

    // CONTRACT
    /// The JSON report (F16-AC6).
    #[must_use]
    pub fn report(&self, now: SystemTime) -> String {
        #[derive(Serialize)]
        struct Totals {
            requests: usize,
            failed_requests: usize,
            tests: usize,
            passed: usize,
            failed: usize,
            skipped: usize,
        }
        #[derive(Serialize)]
        struct Test<'a> {
            name: &'a str,
            status: &'static str,
            error: Option<&'a str>,
        }
        #[derive(Serialize)]
        struct Result<'a> {
            iteration: usize,
            name: &'a str,
            method: &'a str,
            url: &'a str,
            status: Option<u16>,
            time_ms: u128,
            error: Option<&'a str>,
            tests: Vec<Test<'a>>,
        }
        #[derive(Serialize)]
        struct Report<'a> {
            collection: &'a str,
            environment: Option<&'a str>,
            started: String,
            duration_ms: u128,
            iterations: usize,
            totals: Totals,
            results: Vec<Result<'a>>,
        }
        let s = self.summary(now);
        let report = Report {
            collection: &self.name,
            environment: self.environment.as_deref(),
            started: time::iso8601_utc(self.started),
            duration_ms: s.duration.as_millis(),
            iterations: s.iterations,
            totals: Totals {
                requests: s.requests,
                failed_requests: s.failed_requests,
                tests: s.passed + s.failed + s.skipped,
                passed: s.passed,
                failed: s.failed,
                skipped: s.skipped,
            },
            results: self
                .results
                .iter()
                .map(|r| Result {
                    iteration: r.iteration,
                    name: &r.name,
                    method: &r.method,
                    url: &r.url,
                    status: r.status,
                    time_ms: r.time.as_millis(),
                    error: r.error.as_deref(),
                    tests: r
                        .tests
                        .iter()
                        .map(|t| Test {
                            name: &t.name,
                            status: match t.status {
                                TestStatus::Passed => "passed",
                                TestStatus::Failed => "failed",
                                TestStatus::Skipped => "skipped",
                            },
                            error: t.error.as_deref(),
                        })
                        .collect(),
                })
                .collect(),
        };
        serde_json::to_string_pretty(&report).unwrap_or_default()
    }
}
