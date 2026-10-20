//! Scripts in the UI: F15-AC1 editors, F15-AC10 Test Results, F15-AC11
//! console output, F15-AC3 post-response errors.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use ferry_app::{
    App, Bytes, Client, Config, Edit, Finished, Header, Message as AppMessage, RequestTab,
    Response, ResponseTab, ScriptLevel, ScriptLine, ScriptPhase, ScriptReport, TestFilter,
    TestResult, TestStatus, Timing, received,
};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{Message, Shell};
use ferry_ui::tests::NO_TESTS;
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

fn response() -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "application/json".into(),
        }],
        body: Bytes::from_static(br#"{"id": 7}"#),
        elapsed: Duration::from_millis(42),
        timing: Timing::default(),
        url: "https://api.test/users".into(),
        sent: Vec::new(),
        set_cookies: Vec::new(),
    }
}

fn test(name: &str, status: TestStatus, error: Option<&str>) -> TestResult {
    TestResult {
        name: name.into(),
        status,
        error: error.map(Into::into),
    }
}

/// An app whose scripted send ended with `report`.
fn scripted(report: ScriptReport) -> App {
    let mut app = App::default();
    app.update(AppMessage::Edit(Edit::Url("https://api.test/users".into())));
    app.update(AppMessage::Edit(Edit::Script(
        ScriptPhase::PostResponse,
        "pm.test('status is 200', () => pm.response.to.have.status(200));".into(),
    )));
    app.update(AppMessage::SendPressed);
    let tab = app.tab().id;
    app.update(AppMessage::ScriptedFinished {
        tab,
        id: 1,
        result: Arc::new(Finished::Received(Box::new(received(response()).unwrap()))),
        report: Arc::new(report),
    });
    app.update(AppMessage::ResponseTabSelected(ResponseTab::TestResults));
    app
}

fn report() -> ScriptReport {
    ScriptReport {
        tests: vec![
            test("status is 200", TestStatus::Passed, None),
            test(
                "has an id",
                TestStatus::Failed,
                Some("expected 7 to equal 8"),
            ),
            test("later", TestStatus::Skipped, None),
        ],
        console: vec![
            ScriptLine {
                level: ScriptLevel::Log,
                text: "token refreshed".into(),
            },
            ScriptLine {
                level: ScriptLevel::Warn,
                text: "slow response".into(),
            },
        ],
        ..ScriptReport::default()
    }
}

#[test]
fn f15_ac10_test_results_list_and_count() {
    let shell = shell_for(scripted(report()));
    assert!(has(&shell, "Test Results (1/3)"));
    for text in [
        "PASS",
        "status is 200",
        "FAIL",
        "has an id",
        "expected 7 to equal 8",
        "SKIP",
        "later",
    ] {
        assert!(has(&shell, text), "{text}");
    }
    let got = clicked(&shell, "Failed");
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::TestFilterSelected(TestFilter::Failed)]
        ),
        "{got:?}"
    );
}

#[test]
fn f15_ac10_filters_narrow_the_list() {
    let mut app = scripted(report());
    app.update(AppMessage::TestFilterSelected(TestFilter::Failed));
    let shell = shell_for(app);
    assert!(has(&shell, "has an id"));
    assert!(!has(&shell, "status is 200"));
    assert!(!has(&shell, "later"));
    let mut app = scripted(report());
    app.update(AppMessage::TestFilterSelected(TestFilter::Skipped));
    let shell = shell_for(app);
    assert!(has(&shell, "later"));
    assert!(!has(&shell, "has an id"));
}

#[test]
fn f15_ac10_without_tests() {
    let shell = shell_for(scripted(ScriptReport::default()));
    assert!(has(&shell, "Test Results"));
    assert!(has(&shell, NO_TESTS));
}

#[test]
fn f15_ac3_post_response_errors_show_above_the_tests() {
    let error = "Post-response script failed in the request: ReferenceError: x is not defined";
    let shell = shell_for(scripted(ScriptReport {
        error: Some(error.into()),
        ..report()
    }));
    assert!(has(&shell, error));
}

#[test]
fn f15_ac11_console_shows_script_output() {
    let mut app = scripted(report());
    app.update(AppMessage::ToggleConsole);
    let id = app.console().entries[0].id;
    app.update(AppMessage::ConsoleExpand(id));
    let shell = shell_for(app);
    for text in ["Script Output", "token refreshed", "slow response", "warn"] {
        assert!(has(&shell, text), "{text}");
    }
}

#[test]
fn f15_ac1_request_scripts_tab_shows_the_editor() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Scripts));
    app.update(AppMessage::Edit(Edit::Script(
        ScriptPhase::PreRequest,
        "pm.environment.set('ts', Date.now());".into(),
    )));
    let shell = shell_for(app);
    assert!(has(&shell, "Pre-request ●"));
    assert!(has(&shell, "Post-response"));
    assert!(has(&shell, "Scripts ●"));
}

// --- snapshots -----------------------------------------------------------------------

#[cfg(windows)]
fn snapshot(app: App, name: &str) {
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    let snap = ui.snapshot(&ferry_ui::theme::theme()).unwrap();
    assert!(
        snap.matches_image(format!("tests/snapshots/{name}"))
            .unwrap(),
        "{name} snapshot changed"
    );
}

#[cfg(windows)]
#[test]
fn f15_ac10_snapshot_test_results() {
    snapshot(scripted(report()), "scripts_test_results");
}

#[cfg(windows)]
#[test]
fn f15_ac1_snapshot_scripts_tab() {
    let mut app = App::default();
    app.update(AppMessage::Edit(Edit::Url("https://api.test/users".into())));
    app.update(AppMessage::RequestTabSelected(RequestTab::Scripts));
    app.update(AppMessage::Edit(Edit::Script(
        ScriptPhase::PreRequest,
        "const ts = Date.now();\npm.environment.set('ts', ts);\nconsole.log('stamped', ts);".into(),
    )));
    snapshot(app, "scripts_tab");
}
