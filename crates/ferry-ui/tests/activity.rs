//! Headless UI for what sends leave behind and for reading responses:
//! history (F10-AC3..AC5), cookies (F17-AC3..AC5), the console
//! (F18-AC2, AC3), search (F06-AC8), the `JSONPath` filter (F22-AC4) and
//! Save as Example (F32-AC1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use ferry_app::PREVIEW_UNAVAILABLE;
use ferry_app::{
    App, Bytes, Client, Config, Edit, Finished, FixedAmbient, Header, Jar, Message as AppMessage,
    RequestTab, Response, ResponseTab, SetCookie, received, run_find,
};
use ferry_ui::console::{EMPTY, ERRORS_ONLY};
use ferry_ui::dialogs::{ADD_COOKIE, CLEAR_HISTORY, MANAGE_COOKIES};
use ferry_ui::response::{NO_COOKIES, OPEN_IN_BROWSER, SAVE_AS_EXAMPLE};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{Message, Shell};
use ferry_ui::sidebar::{NO_HISTORY, NO_HISTORY_MATCH};
use iced::keyboard::Modifiers;
use iced::{Settings, Size};
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 800.0);
/// 2026-10-17T19:07:20Z.
const NOW_MS: u64 = 1_792_264_040_000;
const URL: &str = "https://api.example.test/users";
/// `ferry_core::json::MAX_FILTER_BODY`.
const MAX_FILTER_BODY: usize = 10 * 1024 * 1024;

fn settings() -> Settings {
    Settings {
        default_text_size: TEXT.into(),
        ..Settings::default()
    }
}

fn shell_for(app: App) -> Shell {
    Shell::new(app, Client::new(Config::default()).unwrap(), None)
}

fn sim(shell: &Shell) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), WINDOW, shell.view())
}

fn has(shell: &Shell, text: &str) -> bool {
    sim(shell).find(text).is_ok()
}

fn app_messages(shell: &Shell, interact: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<String> {
    let mut ui = sim(shell);
    interact(&mut ui);
    ui.into_messages()
        .filter_map(|m| match m {
            Message::App(m) => Some(format!("{m:?}")),
            _ => None,
        })
        .collect()
}

fn fresh() -> App {
    App::default().with_ambient(Box::new(FixedAmbient::new(
        UNIX_EPOCH + Duration::from_millis(NOW_MS),
        0,
    )))
}

fn response(content_type: &str, body: impl Into<Bytes>) -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: content_type.into(),
        }],
        body: body.into(),
        elapsed: Duration::from_millis(42),
        url: URL.into(),
        ..Response::default()
    }
}

/// Send from the active tab of `app` and receive `response`.
fn receive(app: &mut App, response: Response) {
    app.update(AppMessage::Edit(Edit::Url(URL.into())));
    let Some(ferry_app::Effect::Send { tab, id, .. }) = app.update(AppMessage::SendPressed) else {
        panic!("no send");
    };
    app.update(AppMessage::Finished {
        tab,
        id,
        result: Arc::new(Finished::Received(Box::new(received(response).unwrap()))),
    });
}

/// Apply `message` and run the searches and filters it starts.
fn drive(app: &mut App, message: AppMessage) {
    let mut next = app.update(message);
    while let Some(effect) = next {
        next = app.update(run_find(effect).unwrap());
    }
}

// --- F10 history -----------------------------------------------------------------------

#[test]
fn f10_ac3_the_history_sidebar_lists_sends_by_day() {
    let mut app = fresh();
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::History,
    ));
    assert!(has(&shell_for(app), NO_HISTORY));

    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::History,
    ));
    let shell = shell_for(app);
    assert!(has(&shell, "Today"));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(URL);
    });
    assert!(got.contains(&"OpenHistory(0)".to_owned()), "{got:?}");
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("Clear");
    });
    assert_eq!(got, vec!["ClearHistory".to_owned()]);
}

#[test]
fn f10_ac3_a_search_with_no_matches_says_so() {
    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::History,
    ));
    app.update(AppMessage::HistorySearch("nothing like it".into()));
    assert!(has(&shell_for(app), NO_HISTORY_MATCH));
}

#[test]
fn f10_ac5_clearing_asks_first() {
    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    app.update(AppMessage::ClearHistory);
    let shell = shell_for(app);
    assert!(has(&shell, CLEAR_HISTORY));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("Clear");
    });
    assert_eq!(got, vec!["DialogConfirm".to_owned()]);
}

// --- F17 cookies -----------------------------------------------------------------------

fn with_cookie() -> Response {
    let mut r = response("text/plain", "x");
    r.headers.push(Header {
        name: "set-cookie".into(),
        value: "sid=abc123; Path=/; HttpOnly".into(),
    });
    r.set_cookies = vec![SetCookie {
        url: URL.into(),
        header: "sid=abc123; Path=/; HttpOnly".into(),
    }];
    r
}

#[test]
fn f17_ac4_the_cookies_tab_lists_what_the_response_set() {
    let mut app = fresh();
    receive(&mut app, with_cookie());
    let shell = shell_for(app);
    assert!(has(&shell, "Cookies (1)"));
    let mut app = fresh();
    receive(&mut app, with_cookie());
    app.update(AppMessage::ResponseTabSelected(ResponseTab::Cookies));
    let shell = shell_for(app);
    for text in [
        "sid",
        "abc123",
        "api.example.test",
        "Session",
        "true",
        "false",
    ] {
        assert!(has(&shell, text), "{text}");
    }

    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    app.update(AppMessage::ResponseTabSelected(ResponseTab::Cookies));
    let shell = shell_for(app);
    assert!(has(&shell, "Cookies (0)"));
    assert!(has(&shell, NO_COOKIES));
}

#[test]
fn f17_ac3_the_setting_is_offered() {
    let mut app = fresh();
    app.update(AppMessage::RequestTabSelected(RequestTab::Settings));
    assert!(has(&shell_for(app), "Send and store cookies"));
}

fn jar() -> Jar {
    let mut jar = Jar::default();
    let now = i64::try_from(NOW_MS).unwrap();
    jar.store("sid=1; Path=/", "api.test", "/", now).unwrap();
    jar.store("theme=dark; Path=/; Secure", "api.test", "/", now)
        .unwrap();
    jar.store("lang=en", "docs.test", "/", now).unwrap();
    jar
}

#[test]
fn f17_ac5_the_cookie_manager_opens_from_the_footer_and_under_send() {
    let shell = shell_for(fresh());
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("Cookies");
    });
    assert_eq!(got, vec!["OpenCookies".to_owned()]);
}

#[test]
fn f17_ac5_the_manager_lists_domains_and_edits_cookies() {
    let mut app = fresh().with_cookies(jar());
    app.update(AppMessage::OpenCookies);
    let shell = shell_for(app);
    for text in [
        MANAGE_COOKIES,
        "api.test",
        "docs.test",
        "sid=1",
        "theme=dark",
    ] {
        assert!(has(&shell, text), "{text}");
    }
    assert!(
        !has(&shell, "lang=en"),
        "only the selected domain's cookies"
    );
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(ADD_COOKIE);
    });
    assert_eq!(got, vec!["CookieEdit(None)".to_owned()]);
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("docs.test");
    });
    assert_eq!(got, vec!["CookieDomain(\"docs.test\")".to_owned()]);

    let mut app = fresh().with_cookies(jar());
    app.update(AppMessage::OpenCookies);
    app.update(AppMessage::CookieEdit(None));
    app.update(AppMessage::CookieText("no equals sign".into()));
    app.update(AppMessage::CookieSave);
    let shell = shell_for(app);
    assert!(has(&shell, "New cookie"));
    assert!(has(&shell, "Enter a cookie as name=value; attributes"));
}

// --- F18 console -----------------------------------------------------------------------

#[test]
fn f18_ac2_the_footer_toggles_the_console() {
    let shell = shell_for(fresh());
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("Console");
    });
    assert_eq!(got, vec!["ToggleConsole".to_owned()]);
    let mut app = fresh();
    app.update(AppMessage::ToggleConsole);
    assert!(has(&shell_for(app), EMPTY));
}

#[test]
fn f18_ac2_entries_show_one_line_and_expand() {
    let mut app = fresh();
    app.update(AppMessage::ToggleConsole);
    receive(&mut app, response("text/plain", "hello body"));
    let summary = format!("GET {URL} 200 · 42 ms");
    let shell = shell_for(app);
    assert!(has(&shell, &summary));
    assert!(!has(&shell, "Response Headers"));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(summary.as_str());
    });
    assert_eq!(got, vec!["ConsoleExpand(0)".to_owned()]);
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(ERRORS_ONLY);
    });
    assert_eq!(got, vec!["ConsoleErrorsOnly(true)".to_owned()]);

    let mut app = fresh();
    app.update(AppMessage::ToggleConsole);
    receive(&mut app, response("text/plain", "hello body"));
    app.update(AppMessage::ConsoleExpand(0));
    let shell = shell_for(app);
    assert!(has(&shell, "Response Headers"));
    assert!(has(&shell, "hello body"));
}

// --- F06-AC8 search ----------------------------------------------------------------------

fn searching(query: &str) -> App {
    let mut app = fresh();
    receive(
        &mut app,
        response("text/plain", "error one\nall fine\nerror two"),
    );
    drive(&mut app, AppMessage::FindOpen);
    drive(&mut app, AppMessage::FindQuery(query.into()));
    app
}

#[test]
fn f06_ac8_the_find_bar_counts_and_steps() {
    let shell = shell_for(searching("error"));
    assert!(has(&shell, "1 of 2"));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("↓");
    });
    assert_eq!(got, vec!["FindNext".to_owned()]);
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("↑");
    });
    assert_eq!(got, vec!["FindPrevious".to_owned()]);
    assert!(has(&shell_for(searching("nope")), "No results"));
}

#[test]
fn f06_ac8_the_search_button_opens_the_bar() {
    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    let got = app_messages(&shell_for(app), |ui| {
        let _ = ui.click("Search");
    });
    assert_eq!(got, vec!["FindOpen".to_owned()]);
}

#[test]
fn f06_ac8_shift_enter_goes_back_and_escape_closes() {
    let mut shell = shell_for(searching("error"));
    let _ = shell.update(Message::FindSubmit);
    assert!(has(&shell, "2 of 2"), "Enter: next");
    let _ = shell.update(Message::Modifiers(Modifiers::SHIFT));
    let _ = shell.update(Message::FindSubmit);
    assert!(has(&shell, "1 of 2"), "Shift+Enter: previous");
    let _ = shell.update(Message::App(AppMessage::DialogCancel));
    assert!(!has(&shell, "1 of 2"), "Escape closes the bar");
}

#[test]
fn f06_ac8_escape_closes_a_dialog_before_the_find_bar() {
    let mut app = searching("error");
    app.update(AppMessage::OpenCookies);
    let mut shell = shell_for(app);
    let _ = shell.update(Message::App(AppMessage::DialogCancel));
    assert!(!has(&shell, MANAGE_COOKIES));
    assert!(has(&shell, "1 of 2"));
}

// --- F22-AC4 filter --------------------------------------------------------------------

#[test]
fn f22_ac4_filter_results_and_errors_are_shown() {
    let body = r#"{"items":[{"name":"alpha"},{"name":"beta"}]}"#;
    let mut app = fresh();
    receive(&mut app, response("application/json", body));
    drive(
        &mut app,
        AppMessage::FilterChanged("$.items[*].name".into()),
    );
    let shell = shell_for(app);
    assert!(has(&shell, "  \"alpha\","));
    assert!(!has(&shell, "  \"items\": ["));

    let mut app = fresh();
    receive(&mut app, response("application/json", body));
    drive(&mut app, AppMessage::FilterChanged("$[".into()));
    let error = filter_error(&app);
    assert!(error.starts_with("Invalid JSONPath: "), "{error}");
    assert!(has(&shell_for(app), &error));
}

fn filter_error(app: &App) -> String {
    match &app.tab().phase {
        ferry_app::Phase::Received(r) => match &r.filter.state {
            ferry_app::FilterState::Invalid(e) => e.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn f22_ac4_large_bodies_explain_there_is_no_filter() {
    let mut big = b"[".to_vec();
    big.resize(MAX_FILTER_BODY + 1, b' ');
    big.push(b']');
    let mut app = fresh();
    receive(&mut app, response("application/json", big));
    assert!(has(&shell_for(app), ferry_app::FILTER_TOO_LARGE));
}

// --- F22 Preview -----------------------------------------------------------------------

fn previewing(app: App) -> App {
    let mut app = app;
    receive(&mut app, response("text/html", "<h1>Hi</h1>"));
    app.update(AppMessage::BodyFormatSelected(
        ferry_app::BodyFormat::Preview,
    ));
    app
}

#[test]
fn f22_ac1_preview_is_offered_with_open_in_browser() {
    let mut app = fresh();
    receive(&mut app, response("text/html", "<h1>Hi</h1>"));
    let shell = shell_for(app);
    assert!(has(&shell, "Preview"));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(OPEN_IN_BROWSER);
    });
    assert_eq!(got, vec!["OpenInBrowser".to_owned()]);
    let got = app_messages(&shell, |ui| {
        let _ = ui.click("Preview");
    });
    assert_eq!(got, vec!["BodyFormatSelected(Preview)".to_owned()]);
}

#[test]
fn f22_ac1_the_web_view_area_is_left_empty() {
    let shell = shell_for(previewing(fresh()));
    assert!(
        !has(&shell, "<h1>Hi</h1>"),
        "the body text is not drawn under the web view"
    );
    assert!(!has(&shell, PREVIEW_UNAVAILABLE));
    assert!(!has(&shell, "Search"), "no find bar for a preview");
}

#[test]
fn f22_ac3_without_a_web_view_open_in_browser_is_offered() {
    let shell = shell_for(previewing(fresh().without_preview()));
    assert!(has(&shell, PREVIEW_UNAVAILABLE));
    let mut app = previewing(fresh());
    app.update(AppMessage::PreviewFailed("WebView2 is missing".into()));
    let shell = shell_for(app);
    assert!(has(&shell, PREVIEW_UNAVAILABLE));
    assert!(has(&shell, "WebView2 is missing"));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(OPEN_IN_BROWSER);
    });
    assert_eq!(got, vec!["OpenInBrowser".to_owned()]);
}

#[test]
fn f22_ac3_open_in_browser_writes_a_file_with_the_extension() {
    // Only the file: launching the system's application is not tested.
    let first = ferry_ui::preview::write_temp(b"%PDF-1.4", "pdf").unwrap();
    let second = ferry_ui::preview::write_temp(b"<p>", "html").unwrap();
    assert_ne!(first, second, "each call gets its own file");
    let name = first.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("ferry-preview-") && name.ends_with(".pdf"),
        "{name}"
    );
    assert_eq!(first.parent(), Some(std::env::temp_dir().as_path()));
    assert_eq!(std::fs::read(&first).unwrap(), b"%PDF-1.4");
    assert_eq!(std::fs::read(&second).unwrap(), b"<p>");
    let _ = std::fs::remove_file(first);
    let _ = std::fs::remove_file(second);
}

// --- F32-AC1 Save as Example -----------------------------------------------------------

#[test]
fn f32_ac1_save_as_example_needs_a_saved_request() {
    let mut app = fresh();
    receive(&mut app, response("text/plain", "x"));
    let shell = shell_for(app);
    assert!(has(&shell, SAVE_AS_EXAMPLE));
    let got = app_messages(&shell, |ui| {
        let _ = ui.click(SAVE_AS_EXAMPLE);
    });
    assert!(got.is_empty(), "disabled in an unlinked tab: {got:?}");
}

// --- snapshots ---------------------------------------------------------------------------
// Windows only; see ADR-0006.

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
fn snapshot_history_and_console() {
    let mut app = fresh();
    receive(&mut app, with_cookie());
    let mut failed = response("text/plain", "nope");
    failed.status = 404;
    failed.reason = "Not Found".into();
    receive(&mut app, failed);
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::History,
    ));
    app.update(AppMessage::ToggleConsole);
    app.update(AppMessage::ConsoleExpand(1));
    snapshot(app, "activity_history_console");
}

#[cfg(windows)]
#[test]
fn snapshot_cookie_manager() {
    let mut app = fresh().with_cookies(jar());
    app.update(AppMessage::OpenCookies);
    app.update(AppMessage::CookieEdit(Some(("sid".into(), "/".into()))));
    snapshot(app, "activity_cookie_manager");
}

#[cfg(windows)]
#[test]
fn snapshot_cookies_tab() {
    let mut app = fresh();
    receive(&mut app, with_cookie());
    app.update(AppMessage::ResponseTabSelected(ResponseTab::Cookies));
    snapshot(app, "activity_cookies_tab");
}

#[cfg(windows)]
#[test]
fn snapshot_find_and_filter() {
    let body =
        r#"{"items":[{"name":"alpha","tags":["a","b"]},{"name":"beta","tags":[]}],"total":2}"#;
    let mut app = fresh();
    receive(&mut app, response("application/json", body));
    drive(&mut app, AppMessage::FilterChanged("$.items[*]".into()));
    drive(&mut app, AppMessage::FindOpen);
    drive(&mut app, AppMessage::FindQuery("name".into()));
    drive(&mut app, AppMessage::FindNext);
    snapshot(app, "activity_find_filter");
}
