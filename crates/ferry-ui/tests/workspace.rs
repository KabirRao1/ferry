//! Headless UI for the workspace: F04-AC6, F08-AC6, AC8, AC9, F09-AC3..AC7,
//! AC11, AC12, AC13, F14-AC6, F32-AC3.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use ferry_app::{
    App, AuthKind, Client, Config, ContainerSection, Edit, EnvTarget, FixedAmbient, MemFs,
    Message as AppMessage, RequestTab, Target, WorkspaceStore,
};
use ferry_ui::dialogs::delete_question;
use ferry_ui::request::MASKED;
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{Message, Shell};
use ferry_ui::sidebar::{GLOBALS, NO_RESULTS};
use iced::{Settings, Size};
use iced_test::Simulator;

const WINDOW: Size = Size::new(1280.0, 800.0);
const LIST: &str = "collections/api/users/list.toml";
const HEALTH: &str = "collections/api/health.toml";

fn root() -> PathBuf {
    PathBuf::from("/ws")
}

/// An app on a sample workspace held in memory.
fn sample_app() -> App {
    let fs = MemFs::new();
    let put = |rel: &str, text: &str| fs.put(root().join(rel), text);
    put(
        "collections/api/collection.toml",
        "name = \"Payments API\"\ndescription = \"Charges and refunds.\"\n[auth]\nkind = \"bearer\"\ntoken = \"{{token}}\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://api.test\"\n",
    );
    put("collections/api/users/folder.toml", "name = \"Users\"\n");
    put(
        "collections/api/users/list.toml",
        "name = \"List users\"\nurl = \"{{base}}/users?limit={{limit}}\"\n[auth]\nkind = \"inherit\"\n[[examples]]\nname = \"Two users\"\nstatus = 200\nbody = '[{\"id\": 1}, {\"id\": 2}]'\n[[examples.headers]]\nname = \"content-type\"\nvalue = \"application/json\"\n",
    );
    put(
        "collections/api/health.toml",
        "name = \"Health\"\nmethod = \"HEAD\"\nurl = \"{{base}}/health\"\n",
    );
    put("collections/api/broken.toml", "name = ");
    put("collections/zeta/collection.toml", "name = \"Zeta\"\n");
    put(
        "environments/dev.toml",
        "name = \"Development\"\n[[variables]]\nkey = \"token\"\nvalue = \"\"\nsecret = true\n[[variables]]\nkey = \"debug\"\nvalue = \"1\"\nenabled = false\n",
    );
    put(
        "environments/dev.local.toml",
        "[[variables]]\nkey = \"token\"\nvalue = \"s3cret\"\n",
    );
    put("environments/broken.toml", "variables = 1");
    let store = WorkspaceStore::new(fs, &root());
    App::default()
        .with_ambient(Box::new(FixedAmbient::new(
            UNIX_EPOCH + Duration::from_secs(1_792_264_040),
            0,
        )))
        .with_workspace(store.load())
}

fn shell_for(app: App) -> Shell {
    Shell::new(app, Client::new(Config::default()).unwrap(), None)
}

fn settings() -> Settings {
    Settings {
        default_text_size: TEXT.into(),
        ..Settings::default()
    }
}

fn sim(shell: &Shell) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), WINDOW, shell.view())
}

fn has(app: App, text: &str) -> bool {
    sim(&shell_for(app)).find(text).is_ok()
}

fn clicked(app: App, label: &str) -> Vec<AppMessage> {
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    ui.click(label)
        .unwrap_or_else(|_| panic!("{label:?} is not on screen"));
    ui.into_messages()
        .filter_map(|m| match m {
            Message::App(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn expanded(mut app: App) -> App {
    app.update(AppMessage::ToggleExpanded("collections/api".into()));
    app.update(AppMessage::ToggleExpanded("collections/api/users".into()));
    app
}

// --- F09 collections tree -----------------------------------------------------------

#[test]
fn f09_ac3_tree_lists_collections_and_expands() {
    let app = sample_app();
    assert!(has(sample_app(), "Payments API"));
    assert!(has(sample_app(), "Zeta"));
    assert!(!has(sample_app(), "List users"), "collapsed at first");
    let got = clicked(app, "▸");
    assert!(
        matches!(&got[..], [AppMessage::ToggleExpanded(p)] if p == "collections/api"),
        "{got:?}"
    );
    let app = expanded(sample_app());
    for label in ["Users", "List users", "Health", "broken", "HEAD"] {
        assert!(has(expanded(sample_app()), label), "missing {label}");
    }
    drop(app);
}

#[test]
fn f09_ac4_pressing_a_request_starts_a_click_or_drag() {
    let got = clicked(expanded(sample_app()), "Health");
    assert!(
        got.iter()
            .any(|m| matches!(m, AppMessage::ItemPressed(p) if p == HEALTH)),
        "{got:?}"
    );
}

#[test]
fn f09_ac6_new_collection_buttons() {
    let got = clicked(sample_app(), "New");
    assert!(matches!(&got[..], [AppMessage::NewCollection]), "{got:?}");
    let empty = App::default();
    let got = clicked(empty, "New Collection");
    assert!(matches!(&got[..], [AppMessage::NewCollection]), "{got:?}");
}

#[test]
fn f09_ac7_item_menus_offer_actions() {
    let mut app = expanded(sample_app());
    app.update(AppMessage::Menu(Some("collections/api/users".into())));
    for label in [
        "Add Request",
        "Add Folder",
        "Settings",
        "Rename",
        "Duplicate",
        "Delete",
    ] {
        assert!(
            has(
                {
                    let mut a = expanded(sample_app());
                    a.update(AppMessage::Menu(Some("collections/api/users".into())));
                    a
                },
                label
            ),
            "missing {label}"
        );
    }
    let got = clicked(app, "Add Request");
    assert!(
        matches!(&got[..], [AppMessage::NewRequest(p)] if p == "collections/api/users"),
        "{got:?}"
    );
    // A broken file can only be deleted.
    let mut app = expanded(sample_app());
    app.update(AppMessage::Menu(Some("collections/api/broken.toml".into())));
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    assert!(ui.find("Delete").is_ok());
    assert!(ui.find("Rename").is_err());
}

#[test]
fn f32_ac3_examples_are_listed_under_their_request() {
    let mut app = expanded(sample_app());
    app.update(AppMessage::ToggleExpanded(LIST.into()));
    let got = clicked(app, "Two users");
    assert!(
        matches!(&got[..], [AppMessage::OpenExample(p, 0)] if p == LIST),
        "{got:?}"
    );
}

#[test]
fn f09_ac13_search_results_replace_the_sections() {
    let mut app = sample_app();
    app.update(AppMessage::Search("health".into()));
    let got = clicked(app, "Health");
    assert!(
        matches!(&got[..], [AppMessage::OpenItem(p)] if p == HEALTH),
        "{got:?}"
    );
    let mut app = sample_app();
    app.update(AppMessage::Search("zzz".into()));
    assert!(has(app, NO_RESULTS));
}

// --- F08 environments ----------------------------------------------------------------

fn environments(mut app: App) -> App {
    app.update(AppMessage::SidebarSection(
        ferry_app::SidebarSection::Environments,
    ));
    app
}

#[test]
fn f08_ac8_environments_list_globals_and_files() {
    let got = clicked(environments(sample_app()), GLOBALS);
    assert!(
        matches!(&got[..], [AppMessage::OpenEnvironment(EnvTarget::Globals)]),
        "{got:?}"
    );
    let got = clicked(environments(sample_app()), "Development");
    assert!(
        matches!(&got[..], [AppMessage::OpenEnvironment(EnvTarget::Environment(p))] if p == "environments/dev.toml"),
        "{got:?}"
    );
    // A broken environment is listed but does nothing when clicked.
    let shell = shell_for(environments(sample_app()));
    let mut ui = sim(&shell);
    let _ = ui.click("broken");
    assert!(ui.into_messages().next().is_none());
}

#[test]
fn f08_ac8_environment_tab_masks_secrets() {
    let mut app = sample_app();
    app.update(AppMessage::OpenEnvironment(EnvTarget::Environment(
        "environments/dev.toml".into(),
    )));
    assert!(has_after(&app, "ENVIRONMENT"));
    assert!(has_after(&app, "Development"));
    assert!(
        has_after(
            &app,
            "Secret values are masked and saved only to dev.local.toml, which git ignores."
        ),
        "the note names the local file"
    );
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    assert!(ui.find("s3cret").is_err(), "secret values are masked");
}

fn has_after(app: &App, text: &str) -> bool {
    let shell = Shell::new(
        clone_app(app),
        Client::new(Config::default()).unwrap(),
        None,
    );
    sim(&shell).find(text).is_ok()
}

/// Rebuild the same app state through the session (tabs) and messages.
fn clone_app(app: &App) -> App {
    let mut copy = sample_app();
    for tab in app.tabs().iter().skip(1) {
        match &tab.doc {
            ferry_app::Doc::Environment(env) => {
                copy.update(AppMessage::OpenEnvironment(env.target.clone()));
            }
            ferry_app::Doc::Container(c) => {
                copy.update(AppMessage::OpenItem(c.path.clone()));
            }
            _ => {}
        }
    }
    copy
}

#[test]
fn f08_ac6_variable_strip_shows_values_and_missing_names() {
    let mut app = sample_app();
    app.update(AppMessage::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    app.update(AppMessage::OpenItem(LIST.into()));
    app.update(AppMessage::Edit(Edit::AuthKind(AuthKind::Bearer)));
    app.update(AppMessage::Edit(Edit::Token("{{token}}".into())));
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    assert!(ui.find("{{base}}").is_ok());
    assert!(ui.find("https://api.test").is_ok());
    assert!(ui.find("{{limit}}").is_ok());
    assert!(ui.find("not defined").is_ok());
    assert!(ui.find(MASKED).is_ok());
    assert!(ui.find("s3cret").is_err());
}

// --- F14-AC6 location and save ------------------------------------------------------------

#[test]
fn f14_ac6_location_and_save_button() {
    let mut app = sample_app();
    app.update(AppMessage::OpenItem(LIST.into()));
    let shell = shell_for(app);
    let mut ui = sim(&shell);
    for part in ["Payments API", "List users"] {
        assert!(ui.find(part).is_ok(), "{part}");
    }
    let mut app = sample_app();
    app.update(AppMessage::OpenItem(LIST.into()));
    let got = clicked(app, "Save");
    assert!(matches!(&got[..], [AppMessage::Save]), "{got:?}");
}

#[test]
fn f04_ac6_inherit_note_names_the_parent() {
    let mut app = sample_app();
    app.update(AppMessage::OpenItem(LIST.into()));
    app.update(AppMessage::RequestTabSelected(RequestTab::Authorization));
    assert!(has(
        app,
        "This request is using Bearer Token from collection \"Payments API\"."
    ));
}

// --- F09-AC11 settings tabs -----------------------------------------------------------

#[test]
fn f09_ac11_collection_tab_sections() {
    let mut app = sample_app();
    app.update(AppMessage::OpenItem("collections/api".into()));
    for label in [
        "COLLECTION",
        "Overview",
        "Authorization",
        "Variables (1)",
        "Description",
    ] {
        let mut a = sample_app();
        a.update(AppMessage::OpenItem("collections/api".into()));
        assert!(has(a, label), "missing {label}");
    }
    let got = clicked(app, "Variables (1)");
    assert!(
        matches!(
            &got[..],
            [AppMessage::Edit(Edit::ContainerSection(
                ContainerSection::Variables
            ))]
        ),
        "{got:?}"
    );
}

// --- dialogs ------------------------------------------------------------------------

#[test]
fn f09_ac5_save_dialog_lists_destinations() {
    let mut app = sample_app();
    // Without the sidebar, the dialog holds the only "Zeta".
    app.update(AppMessage::ToggleSidebar);
    app.update(AppMessage::Save);
    for label in ["Save Request", "Payments API", "Users", "Zeta", "Cancel"] {
        let mut a = sample_app();
        a.update(AppMessage::Save);
        assert!(has(a, label), "missing {label}");
    }
    let got = clicked(app, "Zeta");
    assert!(
        matches!(&got[..], [AppMessage::DialogPick(p)] if p == "collections/zeta"),
        "{got:?}"
    );
}

#[test]
fn f09_ac7_delete_questions() {
    assert_eq!(
        delete_question(&Target::Item("x".into()), "Health"),
        "Delete \"Health\"?"
    );
    assert_eq!(
        delete_question(&Target::Environment("x".into()), "Dev"),
        "Delete environment \"Dev\"?"
    );
    let mut app = sample_app();
    app.update(AppMessage::Delete(Target::Item(HEALTH.into())));
    assert!(has(app, "Delete \"Health\"?"));
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
fn f09_ac3_snapshot_collections_tree() {
    let mut app = expanded(sample_app());
    app.update(AppMessage::ToggleExpanded(LIST.into()));
    app.update(AppMessage::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    app.update(AppMessage::OpenItem(LIST.into()));
    snapshot(app, "workspace_tree");
}

#[cfg(windows)]
#[test]
fn f08_ac8_snapshot_environment_tab() {
    let mut app = environments(sample_app());
    app.update(AppMessage::OpenEnvironment(EnvTarget::Environment(
        "environments/dev.toml".into(),
    )));
    snapshot(app, "workspace_environment");
}

#[cfg(windows)]
#[test]
fn f09_ac11_snapshot_collection_settings() {
    let mut app = sample_app();
    app.update(AppMessage::OpenItem("collections/api".into()));
    snapshot(app, "workspace_collection");
}

#[cfg(windows)]
#[test]
fn f09_ac5_snapshot_save_dialog() {
    let mut app = expanded(sample_app());
    app.update(AppMessage::Edit(Edit::Url("https://api.test/new".into())));
    app.update(AppMessage::Save);
    snapshot(app, "workspace_save_dialog");
}

#[cfg(windows)]
#[test]
fn f32_ac3_snapshot_example_tab() {
    let mut app = sample_app();
    app.update(AppMessage::OpenExample(LIST.into(), 0));
    snapshot(app, "workspace_example");
}
