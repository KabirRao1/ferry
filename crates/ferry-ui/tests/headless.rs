//! Headless UI tests with `iced_test` (F00-AC1).
//! Screen: F01-AC1/AC5, F05-AC1/AC2/AC7, F06-AC1..AC5. Body view: F06-AC9/AC10.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use ferry_app::body_view::{self as body, BodyView};
use ferry_app::{
    App, Bytes, Finished, Header, Message, Received, Response, ResponseTab, WRAP_BYTES,
};
use ferry_ui::body_view::{self, LINE_HEIGHT};
use ferry_ui::screen::{self, CANCELLED, EMPTY_BODY, IDLE_HINT, SENDING, URL_INPUT};
use ferry_ui::shell;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Settings, Size, mouse};
use iced_test::Simulator;

fn body_state(text: &str) -> BodyView {
    BodyView::new(Bytes::copy_from_slice(text.as_bytes()), WRAP_BYTES).unwrap()
}

fn numbered(n: usize) -> String {
    (0..n).map(|i| format!("line-{i}\n")).collect()
}

fn sim(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(
        Settings::default(),
        Size::new(900.0, 500.0),
        screen::view(app),
    )
}

/// An app that has sent to `url` and received `response`.
fn app_with(response: Response) -> App {
    let mut app = App::default();
    app.update(Message::UrlChanged("https://api.example.test/users".into()));
    app.update(Message::SendPressed);
    let body = BodyView::new(response.body.clone(), WRAP_BYTES).unwrap();
    app.update(Message::Finished {
        id: 1,
        result: Arc::new(Finished::Received(Received { response, body })),
    });
    app
}

fn ok_response(body: &'static str) -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![
            Header {
                name: "content-type".into(),
                value: "application/json".into(),
            },
            Header {
                name: "x-request-id".into(),
                value: "abc123".into(),
            },
        ],
        body: Bytes::from_static(body.as_bytes()),
        elapsed: Duration::from_millis(42),
    }
}

// --- screen ---------------------------------------------------------------

#[test]
fn f00_ac1_idle_screen_shows_hint_method_and_send() {
    let app = App::default();
    let mut ui = sim(&app);
    assert!(ui.find(IDLE_HINT).is_ok());
    // The method picker's label is not a text widget, so `find` cannot see it;
    // the idle snapshot covers it.
    assert!(ui.find("Send").is_ok());
}

#[test]
fn f05_ac1_typing_a_url_and_pressing_enter_sends() {
    let app = App::default();
    let mut ui = sim(&app);
    ui.click(iced::widget::Id::new(URL_INPUT)).unwrap();
    let _ = ui.typewrite("http://x.test");
    let _ = ui.tap_key(Key::Named(Named::Enter));
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::UrlChanged(u) if u == "h")),
        "{messages:?}"
    );
    assert!(
        matches!(messages.last(), Some(Message::SendPressed)),
        "{messages:?}"
    );
}

#[test]
fn f05_ac1_send_button_sends() {
    let app = App::default();
    let mut ui = sim(&app);
    ui.click("Send").unwrap();
    assert!(matches!(
        ui.into_messages().collect::<Vec<_>>().as_slice(),
        [Message::SendPressed]
    ));
}

#[test]
fn f05_ac1_ctrl_or_cmd_enter_shortcut_sends() {
    let press = |modifiers| {
        let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key,
            modified_key,
            physical_key,
            location,
            text,
            repeat,
            ..
        }) = iced_test::simulator::press_key(Key::Named(Named::Enter), None)
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
    };
    assert!(matches!(
        shell::shortcut(press(Modifiers::COMMAND)),
        Some(shell::Message::App(Message::SendPressed))
    ));
    assert!(shell::shortcut(press(Modifiers::default())).is_none());
    assert!(shell::shortcut(press(Modifiers::SHIFT)).is_none());
}

#[test]
fn f05_ac7_sending_shows_progress_and_cancel() {
    let mut app = App::default();
    app.update(Message::UrlChanged("http://x.test".into()));
    app.update(Message::SendPressed);
    let mut ui = sim(&app);
    assert!(ui.find(SENDING).is_ok());
    assert!(ui.find("Send").is_err(), "Send must become Cancel");
    ui.click("Cancel").unwrap();
    assert!(matches!(
        ui.into_messages().collect::<Vec<_>>().as_slice(),
        [Message::CancelPressed]
    ));
}

#[test]
fn f05_ac2_cancelled_request_says_so() {
    let mut app = App::default();
    app.update(Message::UrlChanged("http://x.test".into()));
    app.update(Message::SendPressed);
    app.update(Message::CancelPressed);
    assert!(sim(&app).find(CANCELLED).is_ok());
}

#[test]
fn f01_ac5_invalid_url_message_is_shown() {
    let mut app = App::default();
    app.update(Message::UrlChanged("example.com".into()));
    app.update(Message::SendPressed);
    assert!(
        sim(&app)
            .find("Enter a URL starting with http:// or https://")
            .is_ok()
    );
}

#[test]
fn f06_ac1_to_ac5_response_summary_and_body() {
    let app = app_with(ok_response("{\"id\": 1}"));
    let mut ui = sim(&app);
    assert!(ui.find("200 OK").is_ok(), "status");
    assert!(ui.find("42 ms").is_ok(), "time");
    assert!(ui.find("9 B").is_ok(), "size");
    assert!(ui.find("Headers (2)").is_ok(), "header count");
    assert!(ui.find("{\"id\": 1}").is_ok(), "raw body");
}

#[test]
fn f06_ac4_headers_tab_lists_headers() {
    let mut app = app_with(ok_response("x"));
    let mut ui = sim(&app);
    ui.click("Headers (2)").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        messages.as_slice(),
        [Message::TabSelected(ResponseTab::Headers)]
    ));
    app.update(Message::TabSelected(ResponseTab::Headers));
    let mut ui = sim(&app);
    assert!(ui.find("content-type").is_ok());
    assert!(ui.find("application/json").is_ok());
    assert!(ui.find("abc123").is_ok());
}

#[test]
fn f06_ac5_empty_body_is_labelled() {
    let app = app_with(ok_response(""));
    assert!(sim(&app).find(EMPTY_BODY).is_ok());
}

// --- body view ------------------------------------------------------------

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

// --- snapshots --------------------------------------------------------------
// Pixel snapshots are platform-specific (font rasterization differs), so they
// are recorded and enforced on Windows only. See docs/DECISIONS.md ADR-0006.

#[cfg(windows)]
fn snapshot(app: &App, name: &str) {
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(900.0, 420.0),
        screen::view(app),
    );
    let snap = ui.snapshot(&iced::Theme::Dark).unwrap();
    assert!(
        snap.matches_image(format!("tests/snapshots/{name}"))
            .unwrap(),
        "{name} snapshot changed"
    );
}

#[cfg(windows)]
#[test]
fn f00_ac1_snapshot_idle() {
    snapshot(&App::default(), "screen_idle");
}

#[cfg(windows)]
#[test]
fn f01_ac1_snapshot_selected_method() {
    let mut app = App::default();
    app.update(Message::MethodSelected(ferry_app::Method::Patch));
    app.update(Message::UrlChanged(
        "https://api.example.test/items/7".into(),
    ));
    snapshot(&app, "screen_method_patch");
}

#[cfg(windows)]
#[test]
fn f06_ac1_snapshot_response_body() {
    let body = "{\n  \"id\": 1,\n  \"name\": \"ferry\",\n  \"tags\": [\"rust\", \"http\"]\n}";
    snapshot(&app_with(ok_response(body)), "screen_response_body");
}

#[cfg(windows)]
#[test]
fn f06_ac4_snapshot_response_headers() {
    let mut app = app_with(ok_response("x"));
    app.update(Message::TabSelected(ResponseTab::Headers));
    snapshot(&app, "screen_response_headers");
}

#[cfg(windows)]
#[test]
fn f05_ac6_snapshot_error() {
    let mut app = App::default();
    app.update(Message::UrlChanged("http://127.0.0.1:9/".into()));
    app.update(Message::SendPressed);
    app.update(Message::Finished {
        id: 1,
        result: Arc::new(Finished::Failed("Connection refused by 127.0.0.1:9".into())),
    });
    snapshot(&app, "screen_error");
}

#[cfg(windows)]
#[test]
fn f06_ac10_body_view_snapshot_matches() {
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
        "body view snapshot changed"
    );
}

#[test]
fn f06_ac1_status_colors_by_class() {
    use screen::status_color;
    let classes = [200, 301, 404, 503];
    let colors: Vec<_> = classes.iter().map(|&s| status_color(s)).collect();
    for (i, a) in colors.iter().enumerate() {
        for b in &colors[i + 1..] {
            assert_ne!(a, b, "each status class has its own color");
        }
    }
    assert_eq!(status_color(200), status_color(299));
    assert_eq!(status_color(500), status_color(599));
    assert_eq!(status_color(100), iced::Theme::Dark.palette().text);
}
