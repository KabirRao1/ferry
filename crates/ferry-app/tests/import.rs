//! The Import dialog (F11-AC2..AC5) and `Export…` (F11-AC6), with the
//! files written to an in-memory workspace.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;

use ferry_app::{
    App, AuthKind, Dialog, Doc, Effect, MemFs, Message, Method, NOT_IMPORTABLE, Target,
    WorkspaceStore, collection,
};

use ferry_core::import::postman::export_collection;
use ferry_core::import::{Import, from_tree, import};

const PAYMENTS: &str =
    include_str!("../../ferry-core/tests/fixtures/postman/payments.postman_collection.json");

fn store() -> (MemFs, WorkspaceStore<MemFs>) {
    let fs = MemFs::new();
    let store = WorkspaceStore::new(fs.clone(), &PathBuf::from("/ws"));
    store.init().unwrap();
    (fs, store)
}

/// Apply a workspace effect and report it, as the shell does.
fn apply(app: &mut App, store: &WorkspaceStore<MemFs>, effect: Option<Effect>) {
    let Some(Effect::Workspace { action, ops }) = effect else {
        panic!("not a workspace effect: {effect:?}")
    };
    store.apply(&ops).unwrap();
    app.update(Message::WorkspaceApplied {
        action,
        error: None,
        workspace: Arc::new(store.load()),
    });
}

fn summary(app: &App) -> Vec<String> {
    match app.dialog() {
        Some(Dialog::Import { summary, .. }) => summary.clone(),
        other => panic!("no Import dialog: {other:?}"),
    }
}

fn error(app: &App) -> Option<String> {
    match app.dialog() {
        Some(Dialog::Import { error, .. }) => error.clone(),
        other => panic!("no Import dialog: {other:?}"),
    }
}

fn paste(app: &mut App, text: &str) -> Option<Effect> {
    app.update(Message::OpenImport);
    app.update(Message::ImportText(text.into()));
    app.update(Message::DialogConfirm)
}

#[test]
fn f11_ac2_the_dialog_reads_pasted_text_and_files() {
    let mut app = App::default();
    assert!(app.update(Message::ImportChooseFile).is_none(), "no dialog");
    assert!(paste(&mut app, "hello").is_none());
    assert_eq!(error(&app).as_deref(), Some(NOT_IMPORTABLE));
    // Typing clears the error.
    app.update(Message::ImportText("{".into()));
    assert_eq!(error(&app), None);
    assert!(matches!(
        app.update(Message::ImportChooseFile),
        Some(Effect::PickImportFile)
    ));
    app.update(Message::ImportFileLoaded(Err(
        "Could not read a.json: denied".into(),
    )));
    assert_eq!(
        error(&app).as_deref(),
        Some("Could not read a.json: denied")
    );
    // A file's text imports as soon as it is read.
    assert!(matches!(
        app.update(Message::ImportFileLoaded(Ok(PAYMENTS.into()))),
        Some(Effect::Workspace { .. })
    ));
    app.update(Message::DialogCancel);
    assert!(app.dialog().is_none());
    assert!(
        app.update(Message::ImportFileLoaded(Ok(PAYMENTS.into())))
            .is_none(),
        "a file read after the dialog closed is ignored"
    );
}

#[test]
fn f11_ac2_curl_opens_in_a_new_tab() {
    let mut app = App::default();
    let tabs = app.tabs().len();
    assert!(paste(&mut app, "curl -X PUT https://a.test/x -H 'A: b'").is_none());
    assert!(app.dialog().is_none());
    assert_eq!(app.tabs().len(), tabs + 1);
    assert_eq!(app.tab().doc, Doc::Request);
    assert_eq!(app.tab().draft.method, Method::Put);
    assert_eq!(app.tab().method_text, "PUT");
    assert_eq!(app.tab().draft.url, "https://a.test/x");
}

#[test]
fn f11_ac3_ac4_a_postman_collection_is_written_to_the_workspace() {
    let (fs, store) = store();
    fs.put(
        "/ws/collections/Payments API/collection.toml",
        "name = \"Payments API\"\n",
    );
    let mut app = App::default().with_workspace(store.load());
    let effect = paste(&mut app, PAYMENTS);
    assert_eq!(
        summary(&app),
        [
            "Imported Payments API (2): 7 requests in 2 folders",
            "Folder Auth: folder variables are not supported",
            "POST Create charge: auth type \"ntlm\" is not supported, set to No Auth",
            "PUT Upload receipt: path variables are not supported (:id)",
            "PUT Upload receipt: form field \"receipt\" has 2 files, only the first is kept",
            "PUT Upload receipt: form field \"empty\" names no file, imported as text",
        ]
    );
    apply(&mut app, &store, effect);
    assert!(app.status().is_none(), "{:?}", app.status());

    // Read back from the files: the same collection, in the same order,
    // under the unique name, with the secret value in the local file.
    let dir = "collections/Payments API (2)";
    let item = collection::find(&app.workspace().collections, dir).expect("created");
    let Import::Collection(mut expected) = import(PAYMENTS).unwrap().import else {
        panic!()
    };
    expected.file.name = "Payments API (2)".into();
    let mut got = from_tree(item).unwrap();
    for v in &mut got.file.variables {
        v.origin = ferry_app::Origin::Shared;
    }
    assert_eq!(got, expected);
    let shared = fs.text(format!("/ws/{dir}/collection.toml")).unwrap();
    assert!(!shared.contains("sk_live"), "{shared}");
    let local = fs.text(format!("/ws/{dir}/collection.local.toml")).unwrap();
    assert!(local.contains("sk_live"), "{local}");
    assert!(app.is_expanded(dir), "the new collection is open");

    // Done closes the summary.
    app.update(Message::DialogConfirm);
    assert!(app.dialog().is_none());
}

#[test]
fn f11_ac4_nothing_is_written_when_the_file_cannot_be_read() {
    let mut app = App::default();
    let broken = &PAYMENTS[..PAYMENTS.len() / 2];
    assert!(paste(&mut app, broken).is_none());
    assert!(
        error(&app)
            .unwrap()
            .starts_with("Ferry cannot import this: it is not valid JSON (line "),
        "{:?}",
        error(&app)
    );
}

#[test]
fn f11_ac5_a_postman_environment_becomes_an_environment() {
    let (fs, store) = store();
    fs.put("/ws/environments/Staging.toml", "name = \"Staging\"\n");
    let mut app = App::default().with_workspace(store.load());
    let text = r#"{ "name": "Staging", "_postman_variable_scope": "environment", "values": [
        { "key": "base", "value": "https://staging", "enabled": true },
        { "key": "token", "value": "abc", "type": "secret", "enabled": true } ] }"#;
    let effect = paste(&mut app, text);
    assert_eq!(
        summary(&app),
        ["Imported environment Staging (2): 2 variables"]
    );
    apply(&mut app, &store, effect);
    let entry = app
        .workspace()
        .environment("environments/Staging (2).toml")
        .expect("created");
    assert_eq!(entry.name(), "Staging (2)");
    let vars = &entry.file.as_ref().unwrap().variables;
    assert_eq!(vars[1].value, "abc");
    assert!(vars[1].secret);
    let shared = fs.text("/ws/environments/Staging (2).toml").unwrap();
    assert!(!shared.contains("abc"), "{shared}");
    assert!(
        fs.text("/ws/environments/Staging (2).local.toml")
            .unwrap()
            .contains("abc")
    );
}

#[test]
fn f11_ac6_export_writes_postman_files() {
    let (_, store) = store();
    let mut app = App::default().with_workspace(store.load());
    let effect = paste(&mut app, PAYMENTS);
    apply(&mut app, &store, effect);
    app.update(Message::Menu(Some("collections/Payments API".into())));
    let Some(Effect::SaveExport { name, contents }) = app.update(Message::Export(Target::Item(
        "collections/Payments API".into(),
    ))) else {
        panic!("no export")
    };
    assert_eq!(app.menu(), None, "the menu closes");
    assert_eq!(name, "Payments API.postman_collection.json");
    let item = collection::find(&app.workspace().collections, "collections/Payments API").unwrap();
    assert_eq!(contents, export_collection(&from_tree(item).unwrap()));
    assert!(!contents.contains("sk_live"), "secret values stay home");
    let Import::Collection(back) = import(&contents).unwrap().import else {
        panic!()
    };
    assert_eq!(back.counts(), (7, 2));

    // Only collections export; requests and examples do not.
    for target in [
        Target::Item("collections/Payments API/Auth".into()),
        Target::Item("collections/missing".into()),
        Target::Example("collections/Payments API/Auth/Log in.toml".into(), 0),
    ] {
        assert!(
            app.update(Message::Export(target.clone())).is_none(),
            "{target:?}"
        );
    }

    app.update(Message::Exported(Ok(PathBuf::from("/out/p.json"))));
    assert_eq!(
        app.notice(),
        Some(format!("Exported to {}", PathBuf::from("/out/p.json").display()).as_str())
    );
    app.update(Message::Exported(Err(
        "Could not save /out/p.json: full".into()
    )));
    assert_eq!(app.status(), Some("Could not save /out/p.json: full"));
    app.update(Message::DismissStatus);
    assert_eq!((app.status(), app.notice()), (None, None));
}

#[test]
fn f11_ac6_environments_export_without_secrets() {
    let (fs, store) = store();
    fs.put(
        "/ws/environments/dev.toml",
        "name = \"Dev\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://dev\"\n[[variables]]\nkey = \"token\"\nsecret = true\n",
    );
    fs.put(
        "/ws/environments/dev.local.toml",
        "[[variables]]\nkey = \"token\"\nvalue = \"abc\"\n[[variables]]\nkey = \"scripted\"\nvalue = \"s\"\n",
    );
    let mut app = App::default().with_workspace(store.load());
    let Some(Effect::SaveExport { name, contents }) = app.update(Message::Export(
        Target::Environment("environments/dev.toml".into()),
    )) else {
        panic!("no export")
    };
    assert_eq!(name, "Dev.postman_environment.json");
    assert!(contents.contains("https://dev"), "{contents}");
    assert!(!contents.contains("abc"), "secret value: {contents}");
    assert!(
        !contents.contains("scripted"),
        "local-only value: {contents}"
    );
    let Import::Environment(back) = import(&contents).unwrap().import else {
        panic!()
    };
    assert_eq!(back.variables.len(), 2);
    assert!(back.variables[1].secret);
}

#[test]
fn f11_ac3_saving_an_imported_request_keeps_its_description() {
    let (fs, store) = store();
    let mut app = App::default().with_workspace(store.load());
    let effect = paste(&mut app, PAYMENTS);
    apply(&mut app, &store, effect);
    app.update(Message::DialogCancel);
    let path = "collections/Payments API/Auth/Log in.toml";
    app.update(Message::OpenItem(path.into()));
    app.update(Message::Edit(ferry_app::Edit::Url(
        "{{baseUrl}}/login2".into(),
    )));
    let effect = app.update(Message::Save);
    apply(&mut app, &store, effect);
    let text = fs.text(format!("/ws/{path}")).unwrap();
    assert!(text.contains("login2"), "{text}");
    assert!(text.contains("Returns a token."), "{text}");
    assert_eq!(
        app.workspace().request(path).unwrap().request.auth.kind,
        AuthKind::None
    );
}
