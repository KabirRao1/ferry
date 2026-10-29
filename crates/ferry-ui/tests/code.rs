//! The Code panel (F19-AC1) and pasted cURL errors (F11-AC1) in the UI.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_app::{App, Client, Config, Edit, Message as AppMessage};
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

fn with_code() -> App {
    let mut app = App::default();
    app.update(AppMessage::Edit(Edit::Url(
        "https://api.test/users?limit=5".into(),
    )));
    app.update(AppMessage::ToggleCode);
    app
}

#[test]
fn f19_ac1_the_code_button_toggles_the_panel() {
    let shell = shell_for(App::default());
    assert!(!has(&shell, "Copy"));
    let got = clicked(&shell, "</>");
    assert!(
        matches!(got.as_slice(), [AppMessage::ToggleCode]),
        "{got:?}"
    );
    let shell = shell_for(with_code());
    assert!(has(&shell, "Code"));
    assert!(has(
        &shell,
        "curl 'https://api.test/users?limit=5' \\\n  --location"
    ));
    let got = clicked(&shell, "Copy");
    assert!(matches!(got.as_slice(), [AppMessage::CopyCode]), "{got:?}");
}

#[test]
fn f19_ac1_the_panel_shows_build_errors() {
    let mut app = with_code();
    app.update(AppMessage::Edit(Edit::Url("not a url".into())));
    let shell = shell_for(app);
    assert!(clicked(&shell, "Copy").is_empty(), "nothing to copy");
}

#[test]
fn f11_ac1_curl_errors_show_under_the_url() {
    let mut app = App::default();
    app.update(AppMessage::Edit(Edit::Url("curl 'https://x".into())));
    assert!(has(
        &shell_for(app),
        "Could not read the cURL command: a quote is not closed"
    ));
}

#[cfg(windows)]
#[test]
fn f19_ac1_snapshot_code_panel() {
    let mut app = with_code();
    app.update(AppMessage::CodeLanguage(
        ferry_app::Language::PythonRequests,
    ));
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    let snap = ui.snapshot(&ferry_ui::theme::theme()).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/code_panel").unwrap(),
        "code_panel snapshot changed"
    );
}
