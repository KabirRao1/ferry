//! The Collection Runner's loop (F16-AC1..AC6) with results fed in.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_app::runner::{
    DELAY_ERROR, ITERATIONS_ERROR, MAX_STEPS, RunConfig, RunStatus, Runner, STOPPED, parse_delay,
    parse_iterations, requests,
};
use ferry_app::{
    AuthKind, Bytes, Finished, Header, MemFs, Response, ScriptReport, ScriptVariables, Sent,
    TestResult, TestStatus, Timing, Variable, Workspace, WorkspaceStore, received,
};

const T0: u64 = 1_792_264_040;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(T0 + secs)
}

/// API (bearer, pre-request script) / [A, users/ (B, C), D]
fn workspace() -> Workspace {
    let fs = MemFs::new();
    let store = WorkspaceStore::new(fs.clone(), &PathBuf::from("/ws"));
    store.init().unwrap();
    let put = |rel: &str, text: &str| fs.put(PathBuf::from("/ws").join(rel), text);
    put(
        "collections/api/collection.toml",
        "name = \"API\"\n[auth]\nkind = \"bearer\"\ntoken = \"t\"\n[scripts]\npre_request = \"c();\"\n",
    );
    put(
        "collections/api/_order.toml",
        "order = [\"a.toml\", \"users\", \"d.toml\"]\n",
    );
    put(
        "collections/api/a.toml",
        "name = \"A\"\nurl = \"https://a.test\"\n[auth]\nkind = \"inherit\"\n",
    );
    put(
        "collections/api/d.toml",
        "name = \"D\"\nurl = \"https://d.test\"\n[scripts]\npost_response = \"d();\"\n",
    );
    put(
        "collections/api/users/folder.toml",
        "name = \"Users\"\n[scripts]\npost_response = \"f();\"\n",
    );
    put(
        "collections/api/users/b.toml",
        "name = \"B\"\nurl = \"https://b.test\"\n",
    );
    put(
        "collections/api/users/c.toml",
        "name = \"C\"\nurl = \"https://c.test\"\n",
    );
    put("collections/api/users/broken.toml", "name = ");
    store.load()
}

fn ok(status: u16) -> Finished {
    Finished::Received(Box::new(
        received(Response {
            status,
            reason: "OK".into(),
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            body: Bytes::from_static(b"x"),
            elapsed: Duration::from_millis(30),
            timing: Timing::default(),
            url: "https://x.test".into(),
            sent: Vec::new(),
            set_cookies: Vec::new(),
        })
        .unwrap(),
    ))
}

fn passed(name: &str) -> TestResult {
    TestResult {
        name: name.into(),
        status: TestStatus::Passed,
        error: None,
    }
}

fn failed(name: &str) -> TestResult {
    TestResult {
        name: name.into(),
        status: TestStatus::Failed,
        error: Some("expected 1 to equal 2".into()),
    }
}

fn runner(config: RunConfig) -> Runner {
    let ws = workspace();
    Runner::new(
        "API".into(),
        requests(&ws, "collections/api"),
        config,
        ScriptVariables {
            environment_name: Some("Dev".into()),
            ..ScriptVariables::default()
        },
        ferry_app::ScriptTargets::default(),
        at(0),
    )
}

fn config(iterations: usize) -> RunConfig {
    RunConfig {
        iterations,
        ..RunConfig::default()
    }
}

/// Run to the end, answering every send with `answer`; the names sent.
fn drive(
    r: &mut Runner,
    mut answer: impl FnMut(&str, usize) -> (Finished, ScriptReport),
) -> Vec<String> {
    let mut names = Vec::new();
    let mut clock = 0;
    while let Some(send) = r.next_send(at(clock), 7) {
        names.push(send.info.request_name.clone());
        let (finished, report) = answer(&send.info.request_name, send.info.iteration);
        clock += 1;
        r.record(&finished, &report, at(clock));
    }
    names
}

#[test]
fn f16_ac1_requests_in_sidebar_order_with_auth_and_scripts() {
    let ws = workspace();
    let list = requests(&ws, "collections/api");
    let names: Vec<_> = list.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C", "D"], "broken files are skipped");
    assert_eq!(list[0].draft.auth.kind, AuthKind::Bearer, "inherited");
    assert_eq!(list[0].pre.len(), 1);
    assert_eq!(list[1].post.len(), 1, "the folder's script");
    assert_eq!(list[3].post.len(), 1);
    let folder = requests(&ws, "collections/api/users");
    assert_eq!(folder.len(), 2);
    assert!(requests(&ws, "collections/none").is_empty());
}

#[test]
fn f16_ac1_numbers_are_checked() {
    assert_eq!(parse_iterations(" 3 "), Ok(3));
    assert_eq!(parse_iterations("1000"), Ok(1000));
    for bad in ["0", "1001", "", "x", "-1", "2.5"] {
        assert_eq!(parse_iterations(bad), Err(ITERATIONS_ERROR), "{bad}");
    }
    assert_eq!(parse_delay("0"), Ok(Duration::ZERO));
    assert_eq!(parse_delay("60000"), Ok(Duration::from_secs(60)));
    for bad in ["60001", "", "-5", "1s"] {
        assert_eq!(parse_delay(bad), Err(DELAY_ERROR), "{bad}");
    }
}

#[test]
fn f16_ac2_requests_run_in_order_for_each_iteration() {
    let mut r = runner(RunConfig {
        iterations: 2,
        delay: Duration::from_millis(250),
        ..RunConfig::default()
    });
    assert_eq!(r.delay(), Duration::ZERO, "no wait before the first");
    let mut iterations = Vec::new();
    let names = drive(&mut r, |_, iteration| {
        iterations.push(iteration);
        (ok(200), ScriptReport::default())
    });
    assert_eq!(names, ["A", "B", "C", "D", "A", "B", "C", "D"]);
    assert_eq!(iterations, [0, 0, 0, 0, 1, 1, 1, 1]);
    assert_eq!(r.delay(), Duration::from_millis(250));
    assert_eq!(r.status, RunStatus::Finished);
    assert!(r.next_send(at(99), 0).is_none());
    let s = r.summary(at(99));
    assert_eq!((s.iterations, s.requests, s.failed_requests), (2, 8, 0));
    assert_eq!(s.duration, Duration::from_secs(8));
}

#[test]
fn f16_ac2_variables_carry_from_request_to_request() {
    let mut r = runner(config(1));
    let mut seen = Vec::new();
    let mut clock = 0;
    while let Some(send) = r.next_send(at(clock), 0) {
        seen.push(send.variables.local.clone());
        let mut report = ScriptReport {
            variables: send.variables.clone(),
            ..ScriptReport::default()
        };
        report
            .variables
            .local
            .push(Variable::new(&send.info.request_name, "1"));
        report.variables.changed.environment = true;
        clock += 1;
        r.record(&ok(200), &report, at(clock));
    }
    assert_eq!(seen[0], []);
    assert_eq!(seen[3].len(), 3, "A, B and C set values before D");
}

#[test]
fn f16_ac2_failures_count_and_can_stop_the_run() {
    let answer = |name: &str, _| match name {
        "B" => (
            Finished::Failed("Connection refused by b.test".into()),
            ScriptReport::default(),
        ),
        "C" => (
            ok(200),
            ScriptReport {
                tests: vec![passed("p"), failed("f")],
                ..ScriptReport::default()
            },
        ),
        _ => (ok(200), ScriptReport::default()),
    };
    let mut r = runner(config(1));
    drive(&mut r, answer);
    let s = r.summary(at(99));
    assert_eq!(
        (s.requests, s.failed_requests, s.passed, s.failed),
        (4, 2, 1, 1)
    );
    assert_eq!(
        r.results[1].error.as_deref(),
        Some("Connection refused by b.test")
    );
    assert_eq!(r.results[1].status, None);
    let mut r = runner(RunConfig {
        bail: true,
        ..config(3)
    });
    let names = drive(&mut r, answer);
    assert_eq!(names, ["A", "B"], "stops at the first failure");
    assert_eq!(r.status, RunStatus::Finished);
    // A post-response script error is a failure too.
    let mut r = runner(RunConfig {
        bail: true,
        ..config(1)
    });
    let names = drive(&mut r, |_, _| {
        (
            ok(200),
            ScriptReport {
                error: Some("Post-response script failed in the request: x".into()),
                ..ScriptReport::default()
            },
        )
    });
    assert_eq!(names, ["A"]);
}

#[test]
fn f16_ac3_data_rows_set_the_iterations() {
    let mut r = runner(RunConfig {
        iterations: 9,
        data: vec![
            vec![("user".into(), "ada".into())],
            vec![("user".into(), "bob".into())],
        ],
        ..RunConfig::default()
    });
    assert_eq!(r.iterations(), 2);
    let mut users = Vec::new();
    let mut clock = 0;
    while let Some(send) = r.next_send(at(clock), 0) {
        users.push(send.variables.data[0].value.clone());
        assert_eq!(send.info.iteration_count, 2);
        clock += 1;
        // A data row is not carried into the next iteration.
        r.record(
            &ok(200),
            &ScriptReport {
                variables: send.variables,
                ..ScriptReport::default()
            },
            at(clock),
        );
    }
    assert_eq!(
        users,
        ["ada", "ada", "ada", "ada", "bob", "bob", "bob", "bob"]
    );
}

#[test]
fn f16_ac4_stopping_and_cancelled_sends() {
    let mut r = runner(config(1));
    let _ = r.next_send(at(0), 0);
    r.record(&ok(200), &ScriptReport::default(), at(1));
    let _ = r.next_send(at(1), 0);
    r.record(&Finished::Cancelled, &ScriptReport::default(), at(2));
    assert_eq!(r.status, RunStatus::Stopped);
    assert_eq!(r.notes, [STOPPED]);
    assert_eq!(r.results.len(), 1);
    assert!(r.next_send(at(3), 0).is_none());
    r.stop(at(4));
    assert_eq!(r.notes, [STOPPED], "stopping twice notes it once");
    assert_eq!(r.summary(at(9)).duration, Duration::from_secs(2));
    // A run with nothing in it is over at once.
    let ws = workspace();
    let empty = Runner::new(
        "Empty".into(),
        requests(&ws, "collections/none"),
        config(1),
        ScriptVariables::default(),
        ferry_app::ScriptTargets::default(),
        at(0),
    );
    assert_eq!(empty.status, RunStatus::Finished);
}

#[test]
fn f16_ac5_set_next_request_jumps_and_ends() {
    let mut r = runner(config(1));
    let names = drive(&mut r, |name, _| {
        let next = match name {
            "A" => Some(Some("D".to_owned())),
            "D" => Some(Some("C".to_owned())),
            "C" => Some(None),
            _ => None,
        };
        (
            ok(200),
            ScriptReport {
                next_request: next,
                ..ScriptReport::default()
            },
        )
    });
    assert_eq!(names, ["A", "D", "C"]);
    let mut r = runner(config(2));
    let names = drive(&mut r, |name, _| {
        let next = (name == "B").then(|| Some("Nope".to_owned()));
        (
            ok(200),
            ScriptReport {
                next_request: next,
                ..ScriptReport::default()
            },
        )
    });
    assert_eq!(names, ["A", "B", "A", "B"]);
    assert_eq!(
        r.notes,
        [
            "No request named Nope in this run",
            "No request named Nope in this run"
        ]
    );
}

#[test]
fn f16_ac5_endless_loops_stop_each_iteration() {
    let mut r = runner(config(1));
    let names = drive(&mut r, |_, _| {
        (
            ok(200),
            ScriptReport {
                next_request: Some(Some("A".to_owned())),
                ..ScriptReport::default()
            },
        )
    });
    assert_eq!(names.len(), MAX_STEPS);
    assert_eq!(r.notes, ["Stopped after 10000 requests in one iteration"]);
    assert_eq!(r.status, RunStatus::Finished);
}

#[test]
fn f16_ac6_the_report() {
    let mut r = runner(config(1));
    drive(&mut r, |name, _| match name {
        "A" => (
            ok(201),
            ScriptReport {
                tests: vec![passed("created"), failed("id")],
                sent: Some(Sent {
                    method: "POST".into(),
                    url: "https://a.test/".into(),
                    headers: Vec::new(),
                    body: String::new(),
                }),
                ..ScriptReport::default()
            },
        ),
        "B" => (Finished::Failed("boom".into()), ScriptReport::default()),
        _ => (ok(200), ScriptReport::default()),
    });
    let json: serde_json::Value = serde_json::from_str(&r.report(at(99))).unwrap();
    assert_eq!(json["collection"], "API");
    assert_eq!(json["environment"], "Dev");
    assert_eq!(json["started"], "2026-10-17T19:07:20.000Z");
    assert_eq!(json["duration_ms"], 4000);
    assert_eq!(json["iterations"], 1);
    assert_eq!(
        json["totals"],
        serde_json::json!({"requests": 4, "failed_requests": 2, "tests": 2, "passed": 1, "failed": 1, "skipped": 0})
    );
    let first = &json["results"][0];
    assert_eq!(first["method"], "POST");
    assert_eq!(first["url"], "https://a.test/");
    assert_eq!(first["status"], 201);
    assert_eq!(first["time_ms"], 30);
    assert_eq!(
        first["tests"][1],
        serde_json::json!({"name": "id", "status": "failed", "error": "expected 1 to equal 2"})
    );
    let second = &json["results"][1];
    assert_eq!(second["method"], "GET");
    assert_eq!(second["url"], "https://b.test");
    assert_eq!(second["status"], serde_json::Value::Null);
    assert_eq!(second["error"], "boom");
}
