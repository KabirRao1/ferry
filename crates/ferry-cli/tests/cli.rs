//! `ferry-cli run` end to end (F16-AC7, F16-AC8): the real binary against
//! a workspace folder or Postman files and a local server.
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

/// A Postman collection that sends `{{apiKey}}` in a header to `{{base}}`
/// and tests for a 200, and an environment that sets `base` and has a
/// disabled `apiKey`, so the collection value is used until `--env-var`.
fn postman_files(f: &Folder, url: &str) {
    f.put(
        "keys.postman_collection.json",
        r#"{
  "info": { "name": "Keys", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
  "variable": [ { "key": "apiKey", "value": "from-collection" } ],
  "item": [ {
    "name": "Me",
    "event": [ { "listen": "test", "script": { "exec": [
      "pm.test('authorized', () => pm.response.to.have.status(200));"
    ] } } ],
    "request": { "method": "GET", "header": [ { "key": "X-Api-Key", "value": "{{apiKey}}" } ], "url": "{{base}}/me" }
  } ]
}"#,
    );
    f.put(
        "dev.postman_environment.json",
        &format!(
            r#"{{ "name": "Dev", "_postman_variable_scope": "environment",
  "values": [ {{ "key": "base", "value": "{url}", "enabled": true }},
    {{ "key": "apiKey", "value": "disabled", "enabled": false }} ] }}"#
        ),
    );
}

async fn key_server(key: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(header("x-api-key", key))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(10)
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac8_a_postman_collection_file_runs_with_an_environment_file() {
    let server = key_server("from-collection").await;
    let f = Folder::new();
    postman_files(&f, &server.uri());
    let (code, out, err) = cli(
        &f.0,
        &[
            "run",
            "keys.postman_collection.json",
            "-e",
            "dev.postman_environment.json",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.starts_with("✓ GET Me 200 · "), "{out}");
    // Nothing is written next to the files.
    assert_eq!(f.read("collections/Keys/collection.toml"), None);
    assert_eq!(f.read(".gitignore"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac8_env_vars_override_the_environment_and_collection() {
    let server = key_server("s3cret").await;
    let f = Folder::new();
    postman_files(&f, &server.uri());
    let file_args = [
        "run",
        "keys.postman_collection.json",
        "-e",
        "dev.postman_environment.json",
    ];
    // The collection's placeholder key is refused...
    let (code, out, _) = cli(&f.0, &file_args);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("✗ GET Me 401"), "{out}");
    // ...and the key from --env-var is sent; the last of a name wins.
    let mut args = file_args.to_vec();
    args.extend(["--env-var", "apiKey=wrong", "--env-var", "apiKey=s3cret"]);
    let (code, out, err) = cli(&f.0, &args);
    assert_eq!(code, 0, "{out}{err}");
    // Without an environment, --env-var forms one.
    let (code, out, err) = cli(
        &f.0,
        &[
            "run",
            "keys.postman_collection.json",
            "--env-var",
            &format!("base={}", server.uri()),
            "--env-var",
            "apiKey=s3cret",
        ],
    );
    assert_eq!(code, 0, "{out}{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn f16_ac8_env_vars_work_with_a_workspace() {
    let server = server().await;
    let f = workspace(&server.uri());
    // `who` comes from --env-var instead of the environment file.
    Mock::given(method("GET"))
        .and(path("/users"))
        .and(query_param("who", "env"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(1)
        .mount(&server)
        .await;
    let (code, out, err) = cli(
        &f.0,
        &["run", "api", "-e", "staging", "--env-var", "who=cli"],
    );
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(f.read("environments/staging.local.toml"), None);
}

#[test]
fn f16_ac8_bad_files_and_env_vars_exit_2() {
    let f = Folder::new();
    postman_files(&f, "http://127.0.0.1:9");
    f.put("not-postman.json", "[1, 2]");
    for (args, message) in [
        (vec!["run", "missing.json"], "Could not read missing.json"),
        (
            vec!["run", "not-postman.json"],
            "not-postman.json: Ferry cannot import this",
        ),
        (
            vec!["run", "dev.postman_environment.json"],
            "dev.postman_environment.json: This is not a Postman collection",
        ),
        (
            vec![
                "run",
                "keys.postman_collection.json",
                "-e",
                "keys.postman_collection.json",
            ],
            "keys.postman_collection.json: This is not a Postman environment",
        ),
        (
            vec!["run", "keys.postman_collection.json", "-e", "nope.json"],
            "Could not read nope.json",
        ),
        (
            vec!["run", "keys.postman_collection.json", "--env-var", "apiKey"],
            "--env-var: use name=value, not \"apiKey\"",
        ),
        (
            vec!["run", "keys.postman_collection.json", "--env-var", "=x"],
            "--env-var: use name=value, not \"=x\"",
        ),
        (
            vec!["run", "keys.postman_collection.json", "--env-var"],
            "--env-var needs a value",
        ),
    ] {
        let (code, _, err) = cli(&f.0, &args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(err.contains(message), "{args:?}: {err}");
    }
}
