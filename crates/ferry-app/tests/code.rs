//! Pasting cURL (F11-AC1) and the Code panel (F19-AC1) in the app.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_app::{
    App, AuthKind, BodyMode, Edit, Effect, Language, Message, Method, NOT_A_REQUEST, ScriptPhase,
    Session,
};

fn edit(app: &mut App, e: Edit) {
    app.update(Message::Edit(e));
}

#[test]
fn f11_ac1_pasting_curl_replaces_the_request() {
    let mut app = App::default();
    edit(
        &mut app,
        Edit::Script(ScriptPhase::PreRequest, "keep();".into()),
    );
    edit(
        &mut app,
        Edit::Url(
            "curl -X PUT 'https://api.test/items?id=7' -H 'X-A: 1' -u ada:pw --json '{\"n\":1}'"
                .into(),
        ),
    );
    let tab = app.tab();
    assert_eq!(tab.draft.method, Method::Put);
    assert_eq!(tab.method_text, "PUT");
    assert_eq!(tab.draft.url, "https://api.test/items?id=7");
    assert_eq!(tab.draft.params[0].row.key, "id");
    assert_eq!(tab.draft.auth.kind, AuthKind::Basic);
    assert_eq!(tab.draft.body.mode, BodyMode::Raw);
    assert_eq!(tab.draft.scripts.pre_request, "keep();", "scripts stay");
    assert_eq!(tab.raw_revision, 1, "the body editor reloads");
    assert_eq!(tab.curl_error, None);
    assert!(tab.is_dirty());
}

#[test]
fn f11_ac1_unreadable_curl_leaves_the_request_alone() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://keep.test".into()));
    edit(&mut app, Edit::Url("curl 'https://broken".into()));
    assert_eq!(app.tab().draft.url, "https://keep.test");
    assert_eq!(
        app.tab().curl_error.as_deref(),
        Some("Could not read the cURL command: a quote is not closed")
    );
    // The next ordinary edit clears the message.
    edit(&mut app, Edit::Url("https://keep.test/2".into()));
    assert_eq!(app.tab().curl_error, None);
}

#[test]
fn f19_ac1_code_panel_language_and_copy() {
    let mut app = App::default();
    assert!(!app.code_open());
    app.update(Message::ToggleCode);
    assert!(app.code_open());
    edit(
        &mut app,
        Edit::Url("https://api.test/u?q={{missing}}".into()),
    );
    let code = app.code().unwrap();
    assert!(
        code.starts_with("curl 'https://api.test/u?q={{missing}}'"),
        "{code}"
    );
    app.update(Message::CodeLanguage(Language::PythonRequests));
    assert!(app.code().unwrap().starts_with("import requests"));
    match app.update(Message::Autosave) {
        Some(Effect::SaveSession(session)) => {
            assert_eq!(session.code_language, Language::PythonRequests);
        }
        other => panic!("the language is saved with the session: {other:?}"),
    }
    match app.update(Message::CopyCode) {
        Some(Effect::CopyText(text)) => assert_eq!(text, app.code().unwrap()),
        other => panic!("{other:?}"),
    }
    // Restored on the next start.
    let restored = App::restore(app.session());
    assert_eq!(restored.code_language(), Language::PythonRequests);
    assert_eq!(
        App::restore(Session::default()).code_language(),
        Language::Curl
    );
    app.update(Message::ToggleCode);
    assert!(!app.code_open());
}

#[test]
fn f19_ac1_code_errors_and_non_requests() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("not a url".into()));
    assert!(app.code().is_err());
    assert!(app.update(Message::CopyCode).is_none());
    // A request with variables resolves them as for sending.
    let mut app = App::default();
    edit(
        &mut app,
        Edit::Url("https://api.test/{{$randomInt}}".into()),
    );
    let first = app.code().unwrap();
    assert_eq!(first, app.code().unwrap(), "the same on every redraw");
    assert!(!first.contains("{{"), "{first}");
    assert_eq!(NOT_A_REQUEST, "Code is available for requests.");
}
