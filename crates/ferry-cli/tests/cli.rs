//! `ferry-cli run` end to end (F16-AC7): the real binary against a
//! workspace folder and a local server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh workspace folder in the temporary directory.
struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ferry-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn put(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.0.join(rel)).ok()
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_ferry-cli"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// collections/api: Login (sets token), Users (uses it, tests the status).
fn workspace(url: &str) -> Folder {
    let f = Folder::new();
    f.put(
        "collections/api/collection.toml",
        &format!("name = \"API\"\n[[variables]]\nkey = \"base\"\nvalue = \"{url}\"\n"),
    );
    f.put(
        "collections/api/_order.toml",
        "order = [\"login.toml\", \"users.toml\"]\n",
    );
    f.put(
        "collections/api/login.toml",
        "name = \"Login\"\nmethod = \"POST\"\nurl = \"{{base}}/login\"\n[scripts]\npost_response = \"pm.environment.set('token', pm.response.json().token);\"\n",
    );
    f.put(
        "collections/api/users.toml",
        "name = \"Users\"\nurl = \"{{base}}/users?who={{who}}\"\n[[headers]]\nkey = \"Authorization\"\nvalue = \"Bearer {{token}}\"\n[scripts]\npost_response = \"pm.test('status is 200', () => pm.response.to.have.status(200));\"\n",
    );
    f.put(
        "environments/staging.toml",
        "name = \"Staging\"\n[[variables]]\nkey = \"token\"\nvalue = \"\"\n[[variables]]\nkey = \"who\"\nvalue = \"env\"\n",
    );
    f
}

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"token": "t-9"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users"))
        .and(header("authorization", "Bearer t-9"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac7_a_passing_run_exits_0() {
    let server = server().await;
    let f = workspace(&server.uri());
    let (code, out, err) = cli(&f.0, &["run", "api", "--environment", "staging"]);
    assert_eq!(code, 0, "{out}{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines[0].starts_with("✓ POST Login 200 · "), "{out}");
    assert!(lines[1].starts_with("✓ GET Users 200 · "), "{out}");
    assert!(
        out.contains("2 requests in 1 iteration: 0 failed. Tests: 1 passed, 0 failed, 0 skipped."),
        "{out}"
    );
    // Nothing is saved: no local file appears.
    assert_eq!(f.read("environments/staging.local.toml"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac7_failures_exit_1_and_are_listed() {
    let server = server().await;
    let f = workspace(&server.uri());
    // Without an environment, Login's script cannot store the token, and
    // Users cannot be sent with {{token}} unresolved.
    let (code, out, _) = cli(
        &f.0,
        &[
            "run",
            "collections/api",
            "--workspace",
            f.0.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("✗ POST Login 200"), "{out}");
    assert!(
        out.contains(
            "    Post-response script failed in the request: Error: No environment is selected"
        ),
        "{out}"
    );
    assert!(out.contains("✗ GET Users error"), "{out}");
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac7_iterations_data_bail_and_report() {
    let server = server().await;
    Mock::given(method("GET"))
        .and(path("/users"))
        .and(query_param("who", "bad"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&server)
        .await;
    let f = workspace(&server.uri());
    f.put("data.csv", "who\nada\nbad\nbob\n");
    let report = f.0.join("out/report.json");
    std::fs::create_dir_all(report.parent().unwrap()).unwrap();
    let (code, out, _) = cli(
        &f.0,
        &[
            "run",
            "api",
            "-e",
            "Staging",
            "--data",
            "data.csv",
            "--bail",
            "--report",
            report.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("✗ [2] GET Users 500"), "{out}");
    assert!(
        out.contains("    ✗ status is 200: expected response to have status code 200 but got 500"),
        "{out}"
    );
    assert!(!out.contains("[3]"), "--bail stops the run: {out}");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(json["collection"], "API");
    assert_eq!(json["environment"], "Staging");
    assert_eq!(json["totals"]["requests"], 4);
    assert_eq!(json["results"][3]["status"], 500);
    assert!(out.contains("Report written to"), "{out}");
}

#[test]
fn f16_ac7_bad_arguments_exit_2() {
    let f = Folder::new();
    f.put("collections/api/collection.toml", "name = \"API\"\n");
    for (args, message) in [
        (vec!["run"], "Name a collection or folder to run"),
        (vec!["fly"], "Unknown command \"fly\""),
        (vec!["run", "api", "--wat"], "Unknown option \"--wat\""),
        (vec!["run", "api", "extra"], "Unexpected argument \"extra\""),
        (
            vec!["run", "api", "--iterations"],
            "--iterations needs a value",
        ),
        (
            vec!["run", "api", "--iterations", "0"],
            "--iterations: Enter a number from 1 to 1000",
        ),
        (
            vec!["run", "api", "--delay", "x"],
            "--delay: Enter a number from 0 to 60000",
        ),
        (
            vec!["run", "nope"],
            "No collection or folder \"nope\" in the workspace",
        ),
        (
            vec!["run", "api", "-e", "prod"],
            "No environment named \"prod\"",
        ),
        (
            vec!["run", "api", "--data", "missing.csv"],
            "Could not read missing.csv",
        ),
        (
            vec!["run", "api", "--workspace", "no/such/dir"],
            "No workspace folder at no/such/dir",
        ),
    ] {
        let (code, _, err) = cli(&f.0, &args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains(message), "{args:?}: {err}");
    }
    f.put("bad.csv", "[1]");
    let (code, _, err) = cli(&f.0, &["run", "api", "--data", "bad.csv"]);
    assert_eq!(code, 2);
    assert!(
        err.contains("bad.csv: Data file must be CSV with a header row or a JSON array of objects"),
        "{err}"
    );
    let (code, out, _) = cli(&f.0, &["--help"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("Usage: ferry-cli run"), "{out}");
}

#[test]
fn f16_ac7_an_empty_collection_passes() {
    let f = Folder::new();
    f.put("collections/empty/collection.toml", "name = \"Empty\"\n");
    let (code, out, _) = cli(&f.0, &["run", "empty"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0 requests in 0 iterations"), "{out}");
}
