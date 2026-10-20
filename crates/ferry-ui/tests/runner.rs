//! The Runner tab in the UI (F16-AC1, AC4, AC6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use ferry_app::{
    App, Bytes, Client, Config, Effect, Finished, FixedAmbient, Header, MemFs,
    Message as AppMessage, Response, RunnerEdit, ScriptReport, Sent, TestResult, TestStatus,
    Timing, WorkspaceStore, received,
};
use ferry_ui::runner::{NO_REQUESTS, NO_RUN};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{Message, Shell};
use iced::{Settings, Size};
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 800.0);

fn shell_for(app: App) -> Shell {
    Shell::new(app, Client::new(Config::default()).unwrap(), None)
}

fn sim(shell: &Shell) -> Simulator<'_, Message> {
    Simulator::with_size(
        Settings {
            default_text_size: TEXT.into(),
            ..Settings::default()
        },
        WINDOW,
        shell.view(),
    )
}

fn has(shell: &Shell, text: &str) -> bool {
    sim(shell).find(text).is_ok()
}

fn clicked(shell: &Shell, label: &str) -> Vec<AppMessage> {
    let mut ui = sim(shell);
    let _ = ui.click(label);
    ui.into_messages()
        .filter_map(|m| match m {
            Message::App(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn app() -> App {
    let fs = MemFs::new();
    let root = PathBuf::from("/ws");
    let put = |rel: &str, text: &str| fs.put(root.join(rel), text);
    put(
        "collections/api/collection.toml",
        "name = \"Payments API\"\n",
    );
    put(
        "collections/api/_order.toml",
        "order = [\"login.toml\", \"charge.toml\"]\n",
    );
    put(
        "collections/api/login.toml",
        "name = \"Log in\"\nmethod = \"POST\"\nurl = \"https://api.test/login\"\n",
    );
    put(
        "collections/api/charge.toml",
        "name = \"Create charge\"\nmethod = \"POST\"\nurl = \"https://api.test/charges\"\n",
    );
    put("collections/empty/collection.toml", "name = \"Empty\"\n");
    let store = WorkspaceStore::new(fs, &root);
    store.init().unwrap();
    App::default()
        .with_ambient(Box::new(FixedAmbient::new(
            UNIX_EPOCH + Duration::from_secs(1_792_264_040),
            0,
        )))
        .with_workspace(store.load())
}

fn response(status: u16) -> Finished {
    Finished::Received(Box::new(
        received(Response {
            status,
            reason: "OK".into(),
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            body: Bytes::from_static(b"ok"),
            elapsed: Duration::from_millis(120),
            timing: Timing::default(),
            url: "https://api.test".into(),
            sent: Vec::new(),
            set_cookies: Vec::new(),
        })
        .unwrap(),
    ))
}

/// A Runner tab whose run sent both requests twice.
fn finished_run() -> App {
    let mut app = app();
    app.update(AppMessage::OpenRunner("collections/api".into()));
    app.update(AppMessage::RunnerEdit(RunnerEdit::Iterations("2".into())));
    let tab = app.tab().id;
    let mut effect = app.update(AppMessage::RunStart);
    let mut step = 0;
    while let Some(Effect::RunStep {
        generation, send, ..
    }) = effect
    {
        let failing = send.info.request_name == "Create charge" && step == 3;
        let report = ScriptReport {
            tests: vec![TestResult {
                name: "status is 200".into(),
                status: if failing {
                    TestStatus::Failed
                } else {
                    TestStatus::Passed
                },
                error: failing
                    .then(|| "expected response to have status code 200 but got 402".into()),
            }],
            sent: Some(Sent {
                method: "POST".into(),
                url: send.draft.url.clone(),
                headers: Vec::new(),
                body: String::new(),
            }),
            ..ScriptReport::default()
        };
        let result = if failing {
            response(402)
        } else {
            response(200)
        };
        effect = app.update(AppMessage::RunStepFinished {
            tab,
            generation,
            result: Arc::new(result),
            report: Arc::new(report),
        });
        step += 1;
    }
    app
}

#[test]
fn f16_ac1_settings_and_requests() {
    let mut app = app();
    app.update(AppMessage::OpenRunner("collections/api".into()));
    let shell = shell_for(app);
    for text in [
        "RUNNER",
        "Payments API",
        "Log in",
        "Create charge",
        "Iterations",
        "Delay (ms)",
        "Select File",
        "Stop on first failure",
        NO_RUN,
    ] {
        assert!(has(&shell, text), "{text}");
    }
    let got = clicked(&shell, "Run");
    assert!(matches!(got.as_slice(), [AppMessage::RunStart]), "{got:?}");
    let got = clicked(&shell, "Select File");
    assert!(
        matches!(got.as_slice(), [AppMessage::RunnerPickData]),
        "{got:?}"
    );
}

#[test]
fn f16_ac1_invalid_numbers_show_why_and_disable_run() {
    let mut app = app();
    app.update(AppMessage::OpenRunner("collections/api".into()));
    app.update(AppMessage::RunnerEdit(RunnerEdit::Iterations("0".into())));
    app.update(AppMessage::RunnerEdit(RunnerEdit::Delay("-1".into())));
    let shell = shell_for(app);
    assert!(has(&shell, "Enter a number from 1 to 1000"));
    assert!(has(&shell, "Enter a number from 0 to 60000"));
    assert!(clicked(&shell, "Run").is_empty());
}

#[test]
fn f16_ac1_empty_collections_say_so() {
    let mut app = app();
    app.update(AppMessage::OpenRunner("collections/empty".into()));
    assert!(has(&shell_for(app), NO_REQUESTS));
}

#[test]
fn f16_ac4_results_summary_and_details() {
    let mut app = finished_run();
    let shell = shell_for(finished_run());
    assert!(has(&shell, "Finished"));
    assert!(has(
        &shell,
        "Iterations 2/2 · Requests 4 · Passed 3 · Failed 1 · 0 ms"
    ));
    assert!(has(&shell, "402"));
    assert!(has(&shell, "Export Results"));
    let got = clicked(&shell, "Export Results");
    assert!(matches!(got.as_slice(), [AppMessage::RunExport]), "{got:?}");
    app.update(AppMessage::RunSelect(3));
    let shell = shell_for(app);
    assert!(has(&shell, "POST https://api.test/charges"));
    assert!(has(
        &shell,
        "— expected response to have status code 200 but got 402"
    ));
}

#[test]
fn f16_ac1_footer_runner_opens_the_runner() {
    let shell = shell_for(app());
    let got = clicked(&shell, "Runner");
    assert!(
        matches!(got.as_slice(), [AppMessage::OpenRunnerHere]),
        "{got:?}"
    );
    let mut app = app();
    app.update(AppMessage::OpenRunnerHere);
    assert_eq!(app.tab().title(), "Empty", "the first collection by name");
}

#[cfg(windows)]
#[test]
fn f16_ac4_snapshot_runner() {
    let mut app = finished_run();
    app.update(AppMessage::RunSelect(3));
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    let snap = ui.snapshot(&ferry_ui::theme::theme()).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/runner_results")
            .unwrap(),
        "runner_results snapshot changed"
    );
}
