//! Headless UI tests with `iced_test` (F00-AC1).
//! Workbench: F01-AC5/AC7, F02-AC3/AC4, F03, F04, F05-AC1/AC2/AC4/AC5/AC7,
//! F06-AC1..AC6, F07-AC1/AC2, F14-AC1..AC5. Body view: F06-AC9/AC10.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use ferry_app::body_view::{self as body, BodyView};
use ferry_app::{
    App, AuthKind, BodyMode, Bytes, Client, Config, Edit, Finished, Header, Message as AppMessage,
    RawLanguage, RequestTab, Response, ResponseTab, RowEdit, Table, Timing, WRAP_BYTES, received,
};
use ferry_ui::body_view::{self, LINE_HEIGHT};
use ferry_ui::chrome::UNTITLED;
use ferry_ui::editors::Editors;
use ferry_ui::request::{NO_AUTH, NO_BODY, URL_INPUT};
use ferry_ui::response::{CANCELLED, EMPTY_BODY, FAILED_TITLE, IDLE_HINT, SENDING, timing_lines};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{self, Message, Shell};
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::text_editor;
use iced::{Settings, Size, mouse};
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 800.0);

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

/// Messages produced by `interact` on a fresh simulator of `shell`.
fn messages(shell: &Shell, interact: impl FnOnce(&mut Simulator<'_, Message>)) -> Vec<Message> {
    let mut ui = sim(shell);
    interact(&mut ui);
    ui.into_messages().collect()
}

fn app_messages(messages: Vec<Message>) -> Vec<AppMessage> {
    messages
        .into_iter()
        .filter_map(|m| match m {
            Message::App(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn has(shell: &Shell, text: &str) -> bool {
    sim(shell).find(text).is_ok()
}

fn edit(app: &mut App, e: Edit) {
    app.update(AppMessage::Edit(e));
}

fn ok_response(body: &'static str, content_type: &str) -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![
            Header {
                name: "content-type".into(),
                value: content_type.into(),
            },
            Header {
                name: "x-request-id".into(),
                value: "abc123".into(),
            },
        ],
        body: Bytes::from_static(body.as_bytes()),
        elapsed: Duration::from_millis(42),
        timing: Timing {
            dns: Some(Duration::from_millis(3)),
            connect: Some(Duration::from_millis(12)),
            waiting: Duration::from_millis(25),
            download: Duration::from_millis(2),
        },
    }
}

/// An app that has sent to a URL and received `response`.
fn app_with(response: Response) -> App {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://api.example.test/users".into()));
    app.update(AppMessage::SendPressed);
    let tab = app.tab().id;
    app.update(AppMessage::Finished {
        tab,
        id: 1,
        result: Arc::new(Finished::Received(Box::new(received(response).unwrap()))),
    });
    app
}

// --- F14 layout -------------------------------------------------------------------------

#[test]
fn f14_ac1_layout_regions_are_present() {
    let shell = shell_for(App::default());
    let mut ui = sim(&shell);
    for label in [
        "Ferry",
        "Collections",
        "Environments",
        "History",
        UNTITLED,
        "No environment",
        "Send",
        "Hide sidebar",
        "Console",
        "Cookies",
        "Runner",
        IDLE_HINT,
        "Response",
    ] {
        assert!(ui.find(label).is_ok(), "missing {label:?}");
    }
}

#[test]
fn f14_ac2_request_tabs_show_counts_and_dots() {
    let mut app = App::default();
    let shell = shell_for(App::default());
    for label in [
        "Params",
        "Authorization",
        "Headers (4)",
        "Body",
        "Scripts",
        "Settings",
    ] {
        assert!(has(&shell, label), "missing {label:?}");
    }
    edit(&mut app, Edit::Url("http://a.test/?q=1&r=2".into()));
    edit(&mut app, Edit::AuthKind(AuthKind::Bearer));
    edit(&mut app, Edit::Token("t".into()));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawText("x".into()));
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("X-A".into())),
    );
    let shell = shell_for(app);
    // 1 user header + Content-Type, Authorization, Content-Length, Host,
    // User-Agent, Accept, Accept-Encoding.
    for label in ["Params (2)", "Authorization ●", "Headers (8)", "Body ●"] {
        assert!(has(&shell, label), "missing {label:?}");
    }
}

#[test]
fn f14_ac2_clicking_a_request_tab_selects_it() {
    let shell = shell_for(App::default());
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Scripts").unwrap();
    }));
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::RequestTabSelected(RequestTab::Scripts)]
        ),
        "{got:?}"
    );
}

#[test]
fn f14_ac3_response_tabs_and_summary() {
    let shell = shell_for(app_with(ok_response("{\"id\": 1}", "text/plain")));
    for label in [
        "Body",
        "Cookies",
        "Headers (2)",
        "Test Results",
        "200 OK",
        "42 ms",
        "9 B",
    ] {
        assert!(has(&shell, label), "missing {label:?}");
    }
}

#[test]
fn f14_ac4_sidebar_toggle() {
    let mut shell = shell_for(App::default());
    let got = messages(&shell, |ui| {
        ui.click("Hide sidebar").unwrap();
    });
    for m in got {
        let _ = shell.update(m);
    }
    assert!(!shell.app().sidebar_open());
    assert!(has(&shell, "Show sidebar"));
    assert!(!has(&shell, "No collections yet"));
}

#[test]
fn f14_ac4_divider_drag_resizes_only_while_pressed() {
    let mut shell = shell_for(App::default());
    let _ = shell.update(Message::ResizeMoved(0.3));
    assert!(
        (shell.app().split() - 0.45).abs() < 1e-6,
        "not resizing yet"
    );
    let _ = shell.update(Message::ResizeStart);
    assert!(shell.resizing());
    let _ = shell.update(Message::ResizeMoved(0.3));
    assert!((shell.app().split() - 0.3).abs() < 1e-6);
    let _ = shell.update(Message::ResizeMoved(0.95));
    assert!((shell.app().split() - 0.8).abs() < 1e-6, "clamped");
    let _ = shell.update(Message::ResizeEnd);
    let _ = shell.update(Message::ResizeMoved(0.5));
    assert!((shell.app().split() - 0.8).abs() < 1e-6);
}

#[test]
fn f14_ac5_later_features_explain_themselves_and_do_nothing() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Scripts));
    let shell = shell_for(app);
    assert!(has(&shell, "Scripts arrive in Phase 4"));
    assert!(has(&shell, "No collections yet"));
    // Disabled controls emit nothing.
    for label in ["Console", "Runner", "No environment"] {
        let got = messages(&shell, |ui| {
            let _ = ui.click(label);
        });
        assert!(got.is_empty(), "{label} emitted {got:?}");
    }

    let mut app = app_with(ok_response("x", "text/plain"));
    app.update(AppMessage::ResponseTabSelected(ResponseTab::Cookies));
    assert!(has(&shell_for(app), "Cookies arrive in Phase 3"));
    let mut app = app_with(ok_response("x", "text/plain"));
    app.update(AppMessage::ResponseTabSelected(ResponseTab::TestResults));
    assert!(has(&shell_for(app), "Test results arrive in Phase 4"));
    let mut app = App::default();
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::History,
    ));
    assert!(has(&shell_for(app), "No history yet"));
}

// --- F05 sending ------------------------------------------------------------------------

#[test]
fn f05_ac1_typing_a_url_and_pressing_enter_sends() {
    let shell = shell_for(App::default());
    let got = app_messages(messages(&shell, |ui| {
        ui.click(iced::widget::Id::new(URL_INPUT)).unwrap();
        let _ = ui.typewrite("http://x.test");
        let _ = ui.tap_key(Key::Named(Named::Enter));
    }));
    assert!(
        got.iter()
            .any(|m| matches!(m, AppMessage::Edit(Edit::Url(u)) if u == "h")),
        "{got:?}"
    );
    assert!(
        matches!(got.last(), Some(AppMessage::SendPressed)),
        "{got:?}"
    );
}

#[test]
fn f05_ac1_send_button_sends() {
    let shell = shell_for(App::default());
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Send").unwrap();
    }));
    assert!(
        matches!(got.as_slice(), [AppMessage::SendPressed]),
        "{got:?}"
    );
}

fn key(k: Key, modifiers: Modifiers) -> iced::keyboard::Event {
    let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key,
        location,
        text,
        repeat,
        ..
    }) = iced_test::simulator::press_key(k, None)
    else {
        unreachable!()
    };
    iced::keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key,
        location,
        modifiers,
        text,
        repeat,
    }
}

#[test]
fn f05_ac1_f07_f14_keyboard_shortcuts() {
    let shortcut = |k, m| match shell::shortcut(key(k, m)) {
        Some(Message::App(m)) => Some(format!("{m:?}")),
        Some(other) => panic!("unexpected {other:?}"),
        None => None,
    };
    let ch = |c: &str| Key::Character(c.into());
    let cmd = Modifiers::COMMAND;
    assert_eq!(
        shortcut(Key::Named(Named::Enter), cmd).as_deref(),
        Some("SendPressed")
    );
    assert_eq!(shortcut(ch("t"), cmd).as_deref(), Some("NewTab"));
    assert_eq!(shortcut(ch("w"), cmd).as_deref(), Some("CloseActiveTab"));
    assert_eq!(shortcut(ch("b"), cmd).as_deref(), Some("ToggleSidebar"));
    assert_eq!(
        shortcut(Key::Named(Named::Tab), Modifiers::CTRL).as_deref(),
        Some("NextTab")
    );
    assert_eq!(
        shortcut(Key::Named(Named::Tab), Modifiers::CTRL | Modifiers::SHIFT).as_deref(),
        Some("PreviousTab")
    );
    assert_eq!(
        shortcut(Key::Named(Named::Enter), Modifiers::default()),
        None
    );
    assert_eq!(shortcut(Key::Named(Named::Enter), Modifiers::SHIFT), None);
    assert_eq!(shortcut(ch("t"), Modifiers::default()), None);
    assert_eq!(shortcut(Key::Named(Named::Tab), Modifiers::default()), None);
    let release = iced::keyboard::Event::ModifiersChanged(Modifiers::COMMAND);
    assert!(shell::shortcut(release).is_none());
}

#[test]
fn f05_ac7_sending_shows_progress_and_cancel() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://x.test".into()));
    app.update(AppMessage::SendPressed);
    let shell = shell_for(app);
    assert!(has(&shell, SENDING));
    assert!(!has(&shell, "Send"), "Send must become Cancel");
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Cancel").unwrap();
    }));
    assert!(
        matches!(got.as_slice(), [AppMessage::CancelPressed]),
        "{got:?}"
    );
}

#[test]
fn f05_ac2_cancelled_request_says_so() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://x.test".into()));
    app.update(AppMessage::SendPressed);
    app.update(AppMessage::CancelPressed);
    assert!(has(&shell_for(app), CANCELLED));
}

#[test]
fn f01_ac5_invalid_url_message_is_shown() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("example.com".into()));
    app.update(AppMessage::SendPressed);
    let shell = shell_for(app);
    assert!(has(&shell, FAILED_TITLE));
    assert!(has(&shell, "Enter a URL starting with http:// or https://"));
}

#[test]
fn f01_ac7_invalid_method_shows_reason() {
    let mut app = App::default();
    edit(&mut app, Edit::MethodText("GE T".into()));
    assert!(has(
        &shell_for(app),
        "Method contains invalid character ' '"
    ));
}

#[test]
fn f05_ac4_ac5_settings_tab() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Settings));
    let shell = shell_for(app);
    assert!(has(&shell, "Automatically follow redirects"));
    assert!(has(&shell, "Enable SSL certificate verification"));
}

// --- F06 response -------------------------------------------------------------------------

#[test]
fn f06_ac6_json_is_pretty_by_default_with_format_buttons() {
    let shell = shell_for(app_with(ok_response("{\"id\":1}", "application/json")));
    for label in ["Pretty", "Raw", "Hex", "{", "  \"id\": 1", "}"] {
        assert!(has(&shell, label), "missing {label:?}");
    }
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Raw").unwrap();
    }));
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::BodyFormatSelected(ferry_app::BodyFormat::Raw)]
        ),
        "{got:?}"
    );
}

#[test]
fn f06_ac2_timing_breakdown_lines() {
    let r = ok_response("", "text/plain");
    let lines: Vec<(&str, String)> = timing_lines(&r);
    assert_eq!(
        lines,
        [
            ("DNS Lookup", "3 ms".to_owned()),
            ("Connect (TCP + TLS)", "12 ms".to_owned()),
            ("Waiting (TTFB)", "25 ms".to_owned()),
            ("Download", "2 ms".to_owned()),
            ("Total", "42 ms".to_owned()),
        ]
    );
    let reused = Response {
        timing: Timing::default(),
        ..r
    };
    assert_eq!(timing_lines(&reused)[0].0, "Connection reused");
    assert_eq!(timing_lines(&reused).len(), 4);
}

#[test]
fn f06_ac4_headers_tab_lists_headers() {
    let mut app = app_with(ok_response("x", "application/json"));
    let shell = shell_for(app_with(ok_response("x", "application/json")));
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Headers (2)").unwrap();
    }));
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::ResponseTabSelected(ResponseTab::Headers)]
        ),
        "{got:?}"
    );
    app.update(AppMessage::ResponseTabSelected(ResponseTab::Headers));
    let shell = shell_for(app);
    for label in ["content-type", "application/json", "abc123"] {
        assert!(has(&shell, label), "missing {label:?}");
    }
}

#[test]
fn f06_ac5_empty_body_is_labelled() {
    assert!(has(
        &shell_for(app_with(ok_response("", "text/plain"))),
        EMPTY_BODY
    ));
}

// --- F02, F03, F04 request editing ----------------------------------------------------------

#[test]
fn f02_ac3_auto_headers_hidden_then_listed() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Headers));
    let shell = shell_for(app);
    assert!(has(&shell, "4 hidden"));
    assert!(!has(&shell, "User-Agent"));
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Headers));
    edit(&mut app, Edit::ToggleAutoHeaders);
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("Accept".into())),
    );
    let shell = shell_for(app);
    for label in ["User-Agent", "Accept-Encoding", "overridden"] {
        assert!(has(&shell, label), "missing {label:?}");
    }
}

#[test]
fn f02_ac4_bulk_edit_shows_text_and_edits_flow_back() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/?a=1".into()));
    let mut shell = shell_for(app);
    let got = messages(&shell, |ui| {
        ui.click("Bulk Edit").unwrap();
    });
    for m in got {
        let _ = shell.update(m);
    }
    assert_eq!(shell.app().tab().bulk_text(Table::Params), Some("a: 1\n"));
    assert!(has(&shell, "Key-Value Edit"));
    let _ = shell.update(Message::BulkAction(
        Table::Params,
        text_editor::Action::Move(text_editor::Motion::DocumentEnd),
    ));
    for c in "b: 2".chars() {
        let _ = shell.update(Message::BulkAction(
            Table::Params,
            text_editor::Action::Edit(text_editor::Edit::Insert(c)),
        ));
    }
    assert_eq!(shell.app().tab().draft.url, "http://a.test/?a=1&b=2");
}

#[test]
fn f01_ac8_trailing_blank_row_has_placeholders() {
    let shell = shell_for(App::default());
    for label in ["Key", "Value", "Description", "Bulk Edit", "Query Params"] {
        assert!(has(&shell, label), "missing {label:?}");
    }
}

#[test]
fn f03_body_modes_raw_json_and_errors() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    let shell = shell_for(app);
    // Radio labels are not text widgets; the body snapshot shows them.
    assert!(has(&shell, NO_BODY));
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Json));
    edit(&mut app, Edit::RawText("{\"a\":".into()));
    let shell = shell_for(app);
    assert!(has(&shell, "Beautify"));
    assert!(has(
        &shell,
        "Invalid JSON at line 1, column 5: EOF while parsing a value"
    ));
}

#[test]
fn f03_ac3_raw_editor_edits_reach_the_app_and_beautify_reloads_it() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Json));
    let mut shell = shell_for(app);
    for c in "{\"a\":1}".chars() {
        let _ = shell.update(Message::RawAction(text_editor::Action::Edit(
            text_editor::Edit::Insert(c),
        )));
    }
    // Cursor moves are not edits.
    let _ = shell.update(Message::RawAction(text_editor::Action::Move(
        text_editor::Motion::Home,
    )));
    assert_eq!(shell.app().tab().draft.body.raw, "{\"a\":1}");
    let _ = shell.update(Message::App(AppMessage::Edit(Edit::Beautify)));
    let pretty = "{\n  \"a\": 1\n}";
    assert_eq!(shell.app().tab().draft.body.raw, pretty);
    let editors = Editors::for_app(shell.app());
    assert_eq!(
        editors.get(shell.app().tab().id).unwrap().raw.text(),
        pretty
    );
}

#[test]
fn f03_ac5_ac6_file_buttons_request_a_dialog() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    edit(&mut app, Edit::BodyMode(BodyMode::Binary));
    let shell = shell_for(app);
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Select File").unwrap();
    }));
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::PickFile(ferry_app::FileTarget::Binary)]
        ),
        "{got:?}"
    );
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    edit(&mut app, Edit::BodyMode(BodyMode::FormData));
    let shell = shell_for(app);
    let got = app_messages(messages(&shell, |ui| {
        ui.click("File").unwrap();
    }));
    assert!(
        matches!(
            got.as_slice(),
            [AppMessage::PickFile(ferry_app::FileTarget::FormField(0))]
        ),
        "{got:?}"
    );
}

#[test]
fn f04_auth_fields_follow_the_type() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Authorization));
    assert!(has(&shell_for(app), NO_AUTH));
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Authorization));
    edit(&mut app, Edit::AuthKind(AuthKind::Basic));
    let shell = shell_for(app);
    assert!(has(&shell, "Username"));
    assert!(has(&shell, "Password"));
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Authorization));
    edit(&mut app, Edit::AuthKind(AuthKind::ApiKey));
    let shell = shell_for(app);
    assert!(has(&shell, "Add to"));
}

// --- F07 tabs ----------------------------------------------------------------------------

#[test]
fn f07_ac1_plus_opens_a_tab_and_titles_show() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://one.test/a".into()));
    app.update(AppMessage::NewTab);
    let shell = shell_for(app);
    assert!(has(&shell, "https://one.test/a"));
    assert!(has(&shell, UNTITLED));
    let got = app_messages(messages(&shell, |ui| {
        ui.click("+").unwrap();
    }));
    assert!(matches!(got.as_slice(), [AppMessage::NewTab]), "{got:?}");
}

#[test]
fn f07_ac2_confirm_dialog_for_dirty_tab() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://dirty.test".into()));
    app.update(AppMessage::CloseActiveTab);
    let shell = shell_for(app);
    assert!(has(&shell, "Discard unsaved changes?"));
    let got = app_messages(messages(&shell, |ui| {
        ui.click("Discard").unwrap();
    }));
    assert!(
        matches!(got.as_slice(), [AppMessage::ConfirmClose(true)]),
        "{got:?}"
    );
}

#[test]
fn f07_ac4_dirty_tab_shows_a_dot() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://dirty.test".into()));
    assert!(has(&shell_for(app), "●"));
    assert!(!has(&shell_for(App::default()), "●"));
}

// --- body view ------------------------------------------------------------------------------

fn body_state(text: &str) -> BodyView {
    BodyView::new(Bytes::copy_from_slice(text.as_bytes()), WRAP_BYTES).unwrap()
}

fn numbered(n: usize) -> String {
    (0..n).map(|i| format!("line-{i}\n")).collect()
}

#[test]
fn f06_ac10_only_visible_lines_become_widgets() {
    let state = body_state(&numbered(100_000));
    let height = 10.0 * LINE_HEIGHT;
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, height),
        body_view::view(&state),
    );
    assert!(ui.find("line-0").is_ok());
    assert!(ui.find("line-9").is_ok());
    assert!(
        ui.find("line-10").is_err(),
        "line past the viewport was built"
    );
}

#[test]
fn f06_ac10_scrolled_view_starts_at_top_line() {
    let mut state = body_state(&numbered(1000));
    state.update(body::Message::ScrolledTo(500));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, 5.0 * LINE_HEIGHT),
        body_view::view(&state),
    );
    assert!(ui.find("line-500").is_ok());
    assert!(ui.find("line-499").is_err());
}

#[test]
fn f06_ac10_mouse_wheel_scrolls_by_lines() {
    let state = body_state(&numbered(1000));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, 200.0),
        body_view::view(&state),
    );
    ui.point_at(iced::Point::new(100.0, 100.0));
    let _ = ui.simulate([iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    })]);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        vec![body::Message::ScrolledBy(3)]
    );
}

#[test]
fn f06_ac10_delta_conversion() {
    use body_view::lines_for_delta;
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
        -3
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Pixels { x: 0.0, y: -1.0 }),
        1
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Pixels { x: 0.0, y: 0.0 }),
        0
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Lines {
            x: 0.0,
            y: f32::NAN
        }),
        0
    );
    assert_eq!(body_view::rows_for_height(0.0), 0);
    assert_eq!(body_view::rows_for_height(LINE_HEIGHT * 2.5), 3);
    assert_eq!(body_view::rows_for_height(f32::INFINITY), 0);
}

#[test]
fn f06_ac7_hex_view_renders_offsets() {
    let state = BodyView::hex(Bytes::from_static(b"\x00\x01ABC"));
    let line = state.visible(1)[0].1.clone().into_owned();
    assert!(line.starts_with("00000000  00 01 41 42 43 "), "{line}");
    assert!(line.ends_with("|..ABC|"), "{line}");
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(800.0, 100.0),
        body_view::view(&state),
    );
    assert!(ui.find(line.as_str()).is_ok());
}

// --- snapshots --------------------------------------------------------------------------------
// Pixel snapshots are platform-specific (font rasterization differs), so they
// are recorded and enforced on Windows only. See docs/DECISIONS.md ADR-0006.

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
fn f14_ac1_snapshot_workbench_idle() {
    snapshot(App::default(), "workbench_idle");
}

#[cfg(windows)]
#[test]
fn f06_ac6_snapshot_json_response() {
    let mut app = app_with(ok_response(
        r#"{"id":7,"name":"Ferry","tags":["fast","native"],"active":true}"#,
        "application/json",
    ));
    edit(&mut app, Edit::MethodSelected(ferry_app::Method::Post));
    app.update(AppMessage::NewTab);
    edit(
        &mut app,
        Edit::Url("https://api.example.test/items?page=2".into()),
    );
    app.update(AppMessage::PreviousTab);
    snapshot(app, "workbench_json_response");
}

#[cfg(windows)]
#[test]
fn f02_ac3_snapshot_headers_with_auto_headers() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://api.example.test/users".into()));
    app.update(AppMessage::RequestTabSelected(RequestTab::Headers));
    edit(&mut app, Edit::ToggleAutoHeaders);
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("X-Trace".into())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Value("abc".into())),
    );
    snapshot(app, "workbench_headers");
}

#[cfg(windows)]
#[test]
fn f03_snapshot_raw_json_body() {
    let mut app = App::default();
    edit(&mut app, Edit::MethodSelected(ferry_app::Method::Post));
    edit(&mut app, Edit::Url("https://api.example.test/users".into()));
    app.update(AppMessage::RequestTabSelected(RequestTab::Body));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Json));
    edit(&mut app, Edit::RawText("{\n  \"name\": \"Ada\"\n}".into()));
    snapshot(app, "workbench_body_raw");
}

#[cfg(windows)]
#[test]
fn f04_snapshot_basic_auth() {
    let mut app = App::default();
    app.update(AppMessage::RequestTabSelected(RequestTab::Authorization));
    edit(&mut app, Edit::AuthKind(AuthKind::Basic));
    edit(&mut app, Edit::Username("ada".into()));
    edit(&mut app, Edit::Password("secret".into()));
    snapshot(app, "workbench_auth_basic");
}

#[cfg(windows)]
#[test]
fn f05_ac6_snapshot_error() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://api.example.test/".into()));
    app.update(AppMessage::SendPressed);
    let tab = app.tab().id;
    app.update(AppMessage::Finished {
        tab,
        id: 1,
        result: Arc::new(Finished::Failed(
            "DNS lookup failed for api.example.test".into(),
        )),
    });
    snapshot(app, "workbench_error");
}

#[cfg(windows)]
#[test]
fn f07_ac2_snapshot_confirm_close() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://api.example.test/".into()));
    app.update(AppMessage::CloseActiveTab);
    snapshot(app, "workbench_confirm_close");
}

#[cfg(windows)]
#[test]
fn f14_ac4_snapshot_sidebar_hidden_and_settings() {
    let mut app = App::default();
    app.update(AppMessage::ToggleSidebar);
    app.update(AppMessage::RequestTabSelected(RequestTab::Settings));
    snapshot(app, "workbench_settings_no_sidebar");
}

#[cfg(windows)]
#[test]
fn f06_ac10_snapshot_scrolled_body() {
    let mut state = body_state(&numbered(1000));
    state.update(body::Message::ScrolledTo(42));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(480.0, 180.0),
        body_view::view(&state),
    );
    let snap = ui.snapshot(&iced::Theme::Dark).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/body_view_scrolled")
            .unwrap(),
        "body_view_scrolled snapshot changed"
    );
}
