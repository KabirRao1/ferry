//! Headless UI for importing (F11-AC2, AC4) and exporting (F11-AC6).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;

use ferry_app::{
    App, Client, Config, Dialog, MemFs, Message as AppMessage, Target, WorkspaceStore,
};
use ferry_ui::dialogs::{CHOOSE_FILE, IMPORT_HINT};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{Message, Shell};
use ferry_ui::sidebar::{EXPORT, IMPORT};
use iced::widget::text_editor::{Action, Edit};
use iced::{Point, Settings, Size};
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 800.0);
const PAYMENTS: &str =
    include_str!("../../ferry-core/tests/fixtures/postman/payments.postman_collection.json");

fn workspace_app() -> App {
    let fs = MemFs::new();
    fs.put("/ws/collections/api/collection.toml", "name = \"API\"\n");
    fs.put("/ws/environments/dev.toml", "name = \"Development\"\n");
    App::default().with_workspace(WorkspaceStore::new(fs, &PathBuf::from("/ws")).load())
}

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
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on screen"));
    ui.into_messages()
        .filter_map(|m| match m {
            Message::App(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn dialog_text(shell: &Shell) -> String {
    match shell.app().dialog() {
        Some(Dialog::Import { text, .. }) => text.clone(),
        other => panic!("no Import dialog: {other:?}"),
    }
}

#[test]
fn f11_ac2_import_button_opens_the_dialog() {
    // In the header, so it is there with the sidebar hidden too.
    let mut app = workspace_app();
    app.update(AppMessage::ToggleSidebar);
    let got = clicked(&shell_for(app), IMPORT);
    assert!(
        matches!(got.as_slice(), [AppMessage::OpenImport]),
        "{got:?}"
    );
}

#[test]
fn f11_ac2_the_dialog_takes_a_file_or_pasted_text() {
    let mut app = workspace_app();
    app.update(AppMessage::OpenImport);
    let mut shell = shell_for(app);
    assert!(has(&shell, IMPORT_HINT));
    let got = clicked(&shell, CHOOSE_FILE);
    assert!(
        matches!(got.as_slice(), [AppMessage::ImportChooseFile]),
        "{got:?}"
    );
    let got = clicked(&shell, "Cancel");
    assert!(
        matches!(got.as_slice(), [AppMessage::DialogCancel]),
        "{got:?}"
    );
    // Pasting keeps line breaks, which YAML and multi-line cURL need.
    let text = "curl https://a.test \\\n  -H 'A: b'";
    let _ = shell.update(Message::ImportAction(Action::Edit(Edit::Paste(Arc::new(
        text.into(),
    )))));
    assert_eq!(dialog_text(&shell), text);
    // The title also reads Import: click the button right of Cancel.
    let mut ui = sim(&shell);
    let cancel = ui.find("Cancel").unwrap().visible_bounds().unwrap();
    ui.point_at(Point::new(
        cancel.x + cancel.width + 30.0,
        cancel.center_y(),
    ));
    let _ = ui.simulate(iced_test::simulator::click());
    let got: Vec<AppMessage> = ui
        .into_messages()
        .filter_map(|m| match m {
            Message::App(m) => Some(m),
            _ => None,
        })
        .collect();
    assert!(
        got.iter().any(|m| matches!(m, AppMessage::DialogConfirm)),
        "{got:?}"
    );
}

#[test]
fn f11_ac2_errors_show_in_the_dialog() {
    let mut app = workspace_app();
    app.update(AppMessage::OpenImport);
    app.update(AppMessage::ImportText("hello".into()));
    app.update(AppMessage::DialogConfirm);
    assert!(has(&shell_for(app), ferry_app::NOT_IMPORTABLE));
}

fn imported() -> App {
    let mut app = workspace_app();
    app.update(AppMessage::OpenImport);
    app.update(AppMessage::ImportText(PAYMENTS.into()));
    app.update(AppMessage::DialogConfirm);
    app
}

#[test]
fn f11_ac4_the_summary_lists_what_was_not_carried_over() {
    let shell = shell_for(imported());
    for line in [
        "Imported Payments API: 7 requests in 2 folders",
        "Not carried over",
        "•  POST Create charge: auth type \"ntlm\" is not supported, set to No Auth",
    ] {
        assert!(has(&shell, line), "missing {line}");
    }
    let got = clicked(&shell, "Done");
    assert!(
        matches!(got.as_slice(), [AppMessage::DialogConfirm]),
        "{got:?}"
    );
}

#[test]
fn f11_ac6_export_is_in_the_menus() {
    let mut app = workspace_app();
    app.update(AppMessage::Menu(Some("collections/api".into())));
    let got = clicked(&shell_for(app), EXPORT);
    assert!(
        matches!(got.as_slice(), [AppMessage::Export(Target::Item(p))] if p == "collections/api"),
        "{got:?}"
    );
    let mut app = workspace_app();
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::Environments,
    ));
    app.update(AppMessage::Menu(Some("environments/dev.toml".into())));
    let got = clicked(&shell_for(app), EXPORT);
    assert!(
        matches!(got.as_slice(), [AppMessage::Export(Target::Environment(p))] if p == "environments/dev.toml"),
        "{got:?}"
    );
}

#[test]
fn f11_ac6_the_footer_says_where_the_export_went() {
    let mut app = workspace_app();
    app.update(AppMessage::Exported(Ok(PathBuf::from("out.json"))));
    assert!(has(&shell_for(app), "Exported to out.json"));
}

#[cfg(windows)]
#[test]
fn f11_ac2_snapshot_import_dialogs() {
    let mut app = workspace_app();
    app.update(AppMessage::OpenImport);
    app.update(AppMessage::ImportText(
        "curl https://api.test/users \\\n  -H 'Accept: application/json'".into(),
    ));
    let shell = shell_for(app);
    let snap = sim(&shell).snapshot(&ferry_ui::theme::theme()).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/import_dialog").unwrap(),
        "import_dialog snapshot changed"
    );
    let shell = shell_for(imported());
    let snap = sim(&shell).snapshot(&ferry_ui::theme::theme()).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/import_summary")
            .unwrap(),
        "import_summary snapshot changed"
    );
}
