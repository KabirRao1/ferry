//! App flows as message sequences.
//! Phase 1: F01-AC1/AC5, F05-AC1/AC2/AC6/AC7, F06-AC1/AC4/AC10.
//! Phase 2: F01-AC3/AC7/AC8, F02-AC2/AC4, F03-AC3/AC5/AC6, F04, F05-AC4/AC5,
//! F06-AC6/AC7, F07, F14-AC4.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use ferry_app::body_view;
use ferry_app::{
    App, AuthKind, BodyFormat, BodyMode, Client, Config, Edit, Effect, FileTarget, Finished,
    Header, Message, Method, Phase, RawLanguage, RequestDraft, RequestTab, Response, ResponseTab,
    RowEdit, Session, SidebarSection, TabId, Table, received, run,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn response(status: u16, reason: &str, content_type: &str, body: &'static [u8]) -> Response {
    Response {
        status,
        reason: reason.into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: content_type.into(),
        }],
        body: Bytes::from_static(body),
        elapsed: Duration::from_millis(42),
        ..Response::default()
    }
}

fn ok(body: &'static [u8]) -> Finished {
    Finished::Received(Box::new(
        received(response(200, "OK", "text/plain", body)).unwrap(),
    ))
}

fn finished(tab: TabId, id: u64, result: Finished) -> Message {
    Message::Finished {
        tab,
        id,
        result: Arc::new(result),
    }
}

fn edit(app: &mut App, e: Edit) {
    assert!(app.update(Message::Edit(e)).is_none());
}

/// Type `url` and press Send; returns the send effect's (tab, id).
fn send(app: &mut App, url: &str) -> (TabId, u64) {
    edit(app, Edit::Url(url.into()));
    match app.update(Message::SendPressed) {
        Some(Effect::Send { tab, id, .. }) => (tab, id),
        other => panic!(
            "expected a send effect, got {other:?}, phase {:?}",
            app.tab().phase
        ),
    }
}

/// The request in the next send effect.
fn sent_request(app: &mut App) -> ferry_app::Request {
    match app.update(Message::SendPressed) {
        Some(Effect::Send { request, .. }) => request,
        other => panic!(
            "expected a send effect, got {other:?}, phase {:?}",
            app.tab().phase
        ),
    }
}

fn failed_text(app: &App) -> &str {
    match &app.tab().phase {
        Phase::Failed(text) => text,
        other => panic!("expected failure, got {other:?}"),
    }
}

// --- Phase 1 behavior -------------------------------------------------------------

#[test]
fn f01_ac1_new_request_is_get_with_empty_url() {
    let app = App::default();
    let tab = app.tab();
    assert_eq!(tab.draft.method, Method::Get);
    assert_eq!(tab.method_text, "GET");
    assert_eq!(tab.draft.url, "");
    assert!(matches!(tab.phase, Phase::Idle));
    assert_eq!(tab.response_tab, ResponseTab::Body);
    assert_eq!(tab.request_tab, RequestTab::Params);
    assert!(!tab.is_sending());
}

#[test]
fn f01_ac1_method_selection_is_used_for_send() {
    let mut app = App::default();
    edit(&mut app, Edit::MethodSelected(Method::Delete));
    edit(&mut app, Edit::Url("http://a.test/x".into()));
    let request = sent_request(&mut app);
    assert_eq!(request.method, Method::Delete);
    assert_eq!(request.url.as_str(), "http://a.test/x");
    assert_eq!(app.tab().method_text, "DELETE");
}

#[test]
fn f01_ac5_invalid_url_blocks_send_with_message() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("example.com".into()));
    assert!(app.update(Message::SendPressed).is_none());
    assert_eq!(
        failed_text(&app),
        "Enter a URL starting with http:// or https://"
    );
}

#[test]
fn f05_ac1_send_emits_effect_and_enters_sending() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    assert_eq!(tab, app.tab().id);
    assert_eq!(id, 1);
    assert!(app.tab().is_sending());
}

#[test]
fn f05_ac1_each_send_gets_a_new_id() {
    let mut app = App::default();
    let (tab, first) = send(&mut app, "http://a.test/");
    app.update(finished(tab, first, ok(b"x")));
    let (_, second) = send(&mut app, "http://a.test/");
    assert!(second > first);
}

#[test]
fn f05_ac7_send_while_sending_is_ignored() {
    let mut app = App::default();
    send(&mut app, "http://a.test/");
    assert!(app.update(Message::SendPressed).is_none());
    assert!(app.tab().is_sending());
}

#[test]
fn f05_ac2_cancel_fires_token_and_shows_cancelled() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    let Some(Effect::Send { cancel, .. }) = app.update(Message::SendPressed) else {
        panic!("expected a send");
    };
    app.update(Message::CancelPressed);
    assert!(cancel.is_cancelled());
    assert!(matches!(app.tab().phase, Phase::Cancelled));
}

#[test]
fn f05_ac2_cancel_when_idle_does_nothing() {
    let mut app = App::default();
    app.update(Message::CancelPressed);
    assert!(matches!(app.tab().phase, Phase::Idle));
}

#[test]
fn f05_ac7_stale_results_are_ignored() {
    let mut app = App::default();
    let (tab, first) = send(&mut app, "http://a.test/");
    app.update(Message::CancelPressed);
    let (_, second) = send(&mut app, "http://a.test/");
    app.update(finished(tab, first, ok(b"stale")));
    assert!(
        app.tab().is_sending(),
        "stale result must not end the new send"
    );
    app.update(finished(tab, second, ok(b"fresh")));
    assert!(matches!(app.tab().phase, Phase::Received(_)));
}

#[test]
fn f05_ac7_result_after_cancel_is_ignored() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(Message::CancelPressed);
    app.update(finished(tab, id, ok(b"late")));
    assert!(matches!(app.tab().phase, Phase::Cancelled));
}

#[test]
fn f06_ac1_received_response_is_shown() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(finished(tab, id, ok(b"hello")));
    let Phase::Received(r) = &app.tab().phase else {
        panic!("expected a response");
    };
    assert_eq!(r.response.status_line(), "200 OK");
    assert_eq!(r.format, BodyFormat::Raw);
    assert_eq!(r.view().visible(5)[0].1, "hello");
}

#[test]
fn f05_ac6_failure_is_shown() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(finished(
        tab,
        id,
        Finished::Failed("DNS lookup failed for a.test".into()),
    ));
    assert_eq!(failed_text(&app), "DNS lookup failed for a.test");
}

#[test]
fn f05_ac2_cancelled_result_is_shown() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(finished(tab, id, Finished::Cancelled));
    assert!(matches!(app.tab().phase, Phase::Cancelled));
}

#[test]
fn f14_ac3_response_and_request_tab_selection() {
    let mut app = App::default();
    for t in ResponseTab::ALL {
        app.update(Message::ResponseTabSelected(t));
        assert_eq!(app.tab().response_tab, t);
    }
    for t in RequestTab::ALL {
        app.update(Message::RequestTabSelected(t));
        assert_eq!(app.tab().request_tab, t);
    }
}

#[test]
fn f06_ac10_body_messages_scroll_the_selected_view() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(finished(tab, id, ok(b"a\nb\nc\nd")));
    app.update(Message::Body(body_view::Message::ScrolledBy(2)));
    let Phase::Received(r) = &app.tab().phase else {
        panic!()
    };
    assert_eq!(r.view().top(), 2);
    assert_eq!(r.hex.top(), 0, "other views keep their own position");
}

#[test]
fn f06_ac10_body_messages_without_a_response_are_ignored() {
    let mut app = App::default();
    app.update(Message::Body(body_view::Message::ScrolledBy(3)));
    app.update(Message::BodyFormatSelected(BodyFormat::Hex));
    assert!(matches!(app.tab().phase, Phase::Idle));
}

// --- F06-AC6/AC7 body formats -------------------------------------------------------

#[test]
fn f06_ac6_json_defaults_to_pretty_and_raw_is_available() {
    let r = received(response(200, "OK", "application/json", br#"{"a":[1,2]}"#)).unwrap();
    assert_eq!(r.format, BodyFormat::Pretty);
    assert_eq!(
        r.formats(),
        [BodyFormat::Pretty, BodyFormat::Raw, BodyFormat::Hex],
        "no Preview for JSON"
    );
    let lines: Vec<String> = r
        .view()
        .visible(10)
        .into_iter()
        .map(|(_, l)| l.into_owned())
        .collect();
    assert_eq!(lines, ["{", "  \"a\": [", "    1,", "    2", "  ]", "}"]);
    assert_eq!(r.raw.visible(1)[0].1, r#"{"a":[1,2]}"#);
}

#[test]
fn f06_ac6_json_detected_by_content_or_suffix_type() {
    let by_content = received(response(200, "OK", "text/plain", b"  [1]")).unwrap();
    assert_eq!(by_content.format, BodyFormat::Pretty);
    let by_suffix = received(response(200, "OK", "application/problem+json", b"{}")).unwrap();
    assert_eq!(by_suffix.format, BodyFormat::Pretty);
    let invalid = received(response(200, "OK", "application/json", b"{oops")).unwrap();
    assert_eq!(invalid.format, BodyFormat::Raw);
    assert_eq!(invalid.formats(), [BodyFormat::Raw, BodyFormat::Hex]);
}

#[test]
fn f06_ac7_binary_defaults_to_hex() {
    let r = received(response(200, "OK", "image/png", b"\x89PNG\r\n\x1a\n\0")).unwrap();
    assert_eq!(r.format, BodyFormat::Hex);
    assert!(
        r.view().visible(1)[0]
            .1
            .starts_with("00000000  89 50 4e 47")
    );
}

#[test]
fn f06_ac6_format_selection_only_offers_available_formats() {
    let mut app = App::default();
    let (tab, id) = send(&mut app, "http://a.test/");
    app.update(finished(tab, id, ok(b"plain")));
    app.update(Message::BodyFormatSelected(BodyFormat::Pretty));
    let format = |app: &App| match &app.tab().phase {
        Phase::Received(r) => r.format,
        _ => panic!(),
    };
    assert_eq!(format(&app), BodyFormat::Raw, "no Pretty for non-JSON");
    app.update(Message::BodyFormatSelected(BodyFormat::Hex));
    assert_eq!(format(&app), BodyFormat::Hex);
}

// --- F01 method and params ------------------------------------------------------------

#[test]
fn f01_ac7_typed_methods() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    edit(&mut app, Edit::MethodText("propfind".into()));
    assert_eq!(app.tab().draft.method, Method::Custom("propfind".into()));
    edit(&mut app, Edit::MethodText("post".into()));
    assert_eq!(app.tab().draft.method, Method::Post);
    edit(&mut app, Edit::MethodText("BAD METHOD".into()));
    assert_eq!(app.tab().method_text, "BAD METHOD");
    assert!(app.update(Message::SendPressed).is_none());
    assert_eq!(failed_text(&app), "Method contains invalid character ' '");
    edit(&mut app, Edit::MethodSelected(Method::Put));
    assert_eq!(app.tab().method_error, None);
    assert_eq!(sent_request(&mut app).method, Method::Put);
}

#[test]
fn f01_ac3_ac8_param_rows_and_url_stay_in_sync() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/p?a=1".into()));
    assert_eq!(app.tab().draft.params.len(), 1);
    // Typing into the trailing blank row adds a row.
    edit(
        &mut app,
        Edit::Row(Table::Params, 1, RowEdit::Key("b".into())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Params, 1, RowEdit::Value("2".into())),
    );
    assert_eq!(app.tab().draft.url, "http://a.test/p?a=1&b=2");
    edit(
        &mut app,
        Edit::Row(Table::Params, 0, RowEdit::Enabled(false)),
    );
    assert_eq!(app.tab().draft.url, "http://a.test/p?b=2");
    edit(
        &mut app,
        Edit::Row(Table::Params, 1, RowEdit::Description("doc".into())),
    );
    assert_eq!(app.tab().draft.url, "http://a.test/p?b=2");
    edit(&mut app, Edit::Row(Table::Params, 1, RowEdit::Remove));
    assert_eq!(app.tab().draft.url, "http://a.test/p");
    // Empty text in the blank row does not create a row; far indexes are ignored.
    edit(
        &mut app,
        Edit::Row(Table::Params, 1, RowEdit::Key(String::new())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Params, 9, RowEdit::Key("x".into())),
    );
    edit(&mut app, Edit::Row(Table::Params, 9, RowEdit::Remove));
    assert_eq!(app.tab().draft.params.len(), 1);
}

// --- F02 headers ------------------------------------------------------------------------

#[test]
fn f02_ac2_invalid_header_blocks_send() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("Bad Name".into())),
    );
    assert!(app.update(Message::SendPressed).is_none());
    assert_eq!(
        failed_text(&app),
        "Header name \"Bad Name\" contains invalid character ' '"
    );
}

#[test]
fn f02_ac4_bulk_edit_round_trip() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("A".into())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Value("1".into())),
    );
    edit(&mut app, Edit::ToggleBulk(Table::Headers));
    assert_eq!(app.tab().bulk_text(Table::Headers), Some("A: 1\n"));
    edit(
        &mut app,
        Edit::BulkText(Table::Headers, "A: 1\n//B: 2\nC: 3".into()),
    );
    let keys: Vec<(&str, bool)> = app
        .tab()
        .draft
        .headers
        .iter()
        .map(|h| (h.key.as_str(), h.enabled))
        .collect();
    assert_eq!(keys, [("A", true), ("B", false), ("C", true)]);
    edit(&mut app, Edit::ToggleBulk(Table::Headers));
    assert_eq!(app.tab().bulk_text(Table::Headers), None);
    // Text for a table that is not in bulk mode is ignored.
    edit(&mut app, Edit::BulkText(Table::Headers, "Z: 9".into()));
    assert_eq!(app.tab().draft.headers.len(), 3);
}

#[test]
fn f02_ac4_bulk_edit_params_rewrites_url_and_form_keeps_files() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/?x=1".into()));
    edit(&mut app, Edit::ToggleBulk(Table::Params));
    edit(
        &mut app,
        Edit::BulkText(Table::Params, "x: 1\ny: 2\n".into()),
    );
    assert_eq!(app.tab().draft.url, "http://a.test/?x=1&y=2");

    let tab = app.tab().id;
    app.update(Message::FilePicked {
        tab,
        target: FileTarget::FormField(0),
        path: Some(PathBuf::from("a.png")),
    });
    edit(&mut app, Edit::ToggleBulk(Table::Form));
    edit(
        &mut app,
        Edit::BulkText(Table::Form, "upload: \nnote: hi\n".into()),
    );
    let form = &app.tab().draft.body.form;
    assert_eq!(form[0].file, Some(PathBuf::from("a.png")));
    assert_eq!(form[1].row.value, "hi");
    edit(&mut app, Edit::ToggleBulk(Table::UrlEncoded));
    edit(&mut app, Edit::BulkText(Table::UrlEncoded, "k: v".into()));
    assert_eq!(app.tab().draft.body.urlencoded[0].key, "k");
}

#[test]
fn f02_ac3_auto_headers_toggle() {
    let mut app = App::default();
    assert!(!app.tab().show_auto_headers);
    edit(&mut app, Edit::ToggleAutoHeaders);
    assert!(app.tab().show_auto_headers);
}

// --- F03 body ------------------------------------------------------------------------------

#[test]
fn f03_ac3_json_validation_and_beautify() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Json));
    edit(&mut app, Edit::RawText("{\"a\":".into()));
    let error = app.tab().json_error.clone().unwrap();
    assert_eq!(error.line, 1);
    // Invalid JSON can still be sent.
    let request = sent_request(&mut app);
    assert!(matches!(request.body, ferry_app::RequestBody::Bytes(_)));
    app.update(Message::CancelPressed);

    let revision = app.tab().raw_revision;
    edit(&mut app, Edit::Beautify);
    assert_eq!(
        app.tab().raw_revision,
        revision,
        "invalid JSON is left alone"
    );
    edit(&mut app, Edit::RawText("{\"a\":[1]}".into()));
    assert_eq!(app.tab().json_error, None);
    edit(&mut app, Edit::Beautify);
    assert_eq!(app.tab().draft.body.raw, "{\n  \"a\": [\n    1\n  ]\n}");
    assert_eq!(app.tab().raw_revision, revision + 1);
    // Other languages are not validated.
    edit(&mut app, Edit::RawText("{".into()));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Text));
    assert_eq!(app.tab().json_error, None);
}

#[test]
fn f03_ac5_ac6_file_picking() {
    let mut app = App::default();
    let tab = app.tab().id;
    match app.update(Message::PickFile(FileTarget::Binary)) {
        Some(Effect::PickFile { tab: t, target }) => {
            assert_eq!((t, target), (tab, FileTarget::Binary));
        }
        other => panic!("expected a file dialog, got {other:?}"),
    }
    // Cancelled dialogs change nothing.
    app.update(Message::FilePicked {
        tab,
        target: FileTarget::Binary,
        path: None,
    });
    assert_eq!(app.tab().draft.body.binary, None);
    app.update(Message::FilePicked {
        tab,
        target: FileTarget::Binary,
        path: Some(PathBuf::from("blob.bin")),
    });
    assert_eq!(app.tab().draft.body.binary, Some(PathBuf::from("blob.bin")));
    edit(&mut app, Edit::ClearBinary);
    assert_eq!(app.tab().draft.body.binary, None);

    app.update(Message::FilePicked {
        tab,
        target: FileTarget::FormField(0),
        path: Some(PathBuf::from("a.txt")),
    });
    edit(
        &mut app,
        Edit::Row(Table::Form, 0, RowEdit::Key("doc".into())),
    );
    assert_eq!(
        app.tab().draft.body.form[0].file,
        Some(PathBuf::from("a.txt"))
    );
    edit(&mut app, Edit::Row(Table::Form, 0, RowEdit::ClearFile));
    assert_eq!(app.tab().draft.body.form[0].file, None);
    // Files for closed tabs are dropped.
    app.update(Message::FilePicked {
        tab: TabId(999),
        target: FileTarget::Binary,
        path: Some(PathBuf::from("x")),
    });
    assert_eq!(app.tab().draft.body.binary, None);
}

// --- F04 auth and F05 options --------------------------------------------------------------

#[test]
fn f04_auth_edits_reach_the_request() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    edit(&mut app, Edit::AuthKind(AuthKind::Basic));
    edit(&mut app, Edit::Username("u".into()));
    edit(&mut app, Edit::Password("p".into()));
    let header = |r: &ferry_app::Request| {
        r.headers
            .iter()
            .map(|h| (h.name.clone(), h.value.clone()))
            .collect::<Vec<_>>()
    };
    let r = sent_request(&mut app);
    assert_eq!(header(&r), [("Authorization".into(), "Basic dTpw".into())]);
    app.update(Message::CancelPressed);
    edit(&mut app, Edit::AuthKind(AuthKind::Bearer));
    edit(&mut app, Edit::Token("t".into()));
    let r = sent_request(&mut app);
    assert_eq!(header(&r), [("Authorization".into(), "Bearer t".into())]);
    app.update(Message::CancelPressed);
    edit(&mut app, Edit::AuthKind(AuthKind::ApiKey));
    edit(&mut app, Edit::ApiKey("k".into()));
    edit(&mut app, Edit::ApiValue("v".into()));
    edit(
        &mut app,
        Edit::ApiLocation(ferry_app::ApiKeyLocation::Query),
    );
    let r = sent_request(&mut app);
    assert_eq!(r.url.as_str(), "http://a.test/?k=v");
}

#[test]
fn f05_ac4_ac5_options_reach_the_request() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("https://a.test/".into()));
    edit(&mut app, Edit::FollowRedirects(false));
    edit(&mut app, Edit::VerifyTls(false));
    let r = sent_request(&mut app);
    assert!(!r.options.follow_redirects);
    assert!(!r.options.verify_tls);
}

// --- F07 tabs ---------------------------------------------------------------------------------

#[test]
fn f07_ac1_new_tabs_are_independent_and_titled() {
    let mut app = App::default();
    assert_eq!(app.tabs().len(), 1);
    assert_eq!(app.tab().title(), "Untitled Request");
    edit(&mut app, Edit::Url("http://one.test/".into()));
    app.update(Message::NewTab);
    assert_eq!(app.tabs().len(), 2);
    assert_eq!(app.active_index(), 1);
    assert_eq!(app.tab().draft.url, "");
    assert_eq!(app.tabs()[0].title(), "http://one.test/");
    assert_ne!(app.tabs()[0].id, app.tabs()[1].id);
}

#[test]
fn f07_ac2_closing_clean_and_dirty_tabs() {
    let mut app = App::default();
    app.update(Message::NewTab);
    app.update(Message::NewTab);
    let ids: Vec<TabId> = app.tabs().iter().map(|t| t.id).collect();
    // Closing a clean, inactive tab before the active one keeps the active tab.
    app.update(Message::CloseTab(ids[0]));
    assert_eq!(app.tabs().len(), 2);
    assert_eq!(app.tab().id, ids[2]);
    // A dirty tab asks first.
    edit(&mut app, Edit::Url("http://dirty.test/".into()));
    app.update(Message::CloseActiveTab);
    assert_eq!(app.confirm_close(), Some(ids[2]));
    app.update(Message::ConfirmClose(false));
    assert_eq!(app.confirm_close(), None);
    assert_eq!(app.tabs().len(), 2);
    app.update(Message::CloseActiveTab);
    app.update(Message::ConfirmClose(true));
    assert_eq!(app.tabs().len(), 1);
    assert_eq!(app.tab().id, ids[1], "the nearest tab becomes active");
    // Closing the last tab opens a fresh one.
    app.update(Message::CloseActiveTab);
    assert_eq!(app.tabs().len(), 1);
    assert_ne!(app.tab().id, ids[1]);
    // Unknown ids are ignored.
    app.update(Message::CloseTab(TabId(999)));
    app.update(Message::ActivateTab(TabId(999)));
    assert_eq!(app.tabs().len(), 1);
}

#[test]
fn f07_ac2_closing_a_sending_tab_cancels_it() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/".into()));
    let Some(Effect::Send {
        cancel, tab, id, ..
    }) = app.update(Message::SendPressed)
    else {
        panic!()
    };
    app.update(Message::CloseActiveTab);
    app.update(Message::ConfirmClose(true));
    assert!(cancel.is_cancelled());
    // Its result arrives later and is dropped.
    app.update(finished(tab, id, ok(b"late")));
    assert!(matches!(app.tab().phase, Phase::Idle));
}

#[test]
fn f07_ac3_drag_to_reorder_and_cycle() {
    let mut app = App::default();
    app.update(Message::NewTab);
    app.update(Message::NewTab);
    let ids: Vec<TabId> = app.tabs().iter().map(|t| t.id).collect();
    app.update(Message::TabPressed(ids[0]));
    assert_eq!(app.dragging(), Some(ids[0]));
    app.update(Message::TabEntered(ids[2]));
    app.update(Message::TabReleased);
    let order: Vec<TabId> = app.tabs().iter().map(|t| t.id).collect();
    assert_eq!(order, [ids[1], ids[2], ids[0]]);
    assert_eq!(app.tab().id, ids[0]);
    assert_eq!(app.dragging(), None);
    // Entering a tab without dragging does nothing.
    app.update(Message::TabEntered(ids[1]));
    assert_eq!(app.tabs()[2].id, ids[0]);
    app.update(Message::NextTab);
    assert_eq!(app.tab().id, ids[1]);
    app.update(Message::PreviousTab);
    assert_eq!(app.tab().id, ids[0]);
    app.update(Message::ActivateTab(ids[2]));
    assert_eq!(app.active_index(), 1);
}

#[test]
fn f07_ac4_dirty_until_back_to_saved_state() {
    let mut app = App::default();
    assert!(!app.tab().is_dirty());
    edit(&mut app, Edit::Url("x".into()));
    assert!(app.tab().is_dirty());
    edit(&mut app, Edit::Url(String::new()));
    assert!(!app.tab().is_dirty());
}

#[test]
fn f07_ac5_session_saved_only_when_changed_and_restored() {
    let mut app = App::default();
    assert!(
        app.update(Message::Autosave).is_none(),
        "nothing changed yet"
    );
    edit(&mut app, Edit::Url("http://a.test/".into()));
    app.update(Message::NewTab);
    app.update(Message::ToggleSidebar);
    let Some(Effect::SaveSession(session)) = app.update(Message::Autosave) else {
        panic!("expected a save");
    };
    assert!(app.update(Message::Autosave).is_none(), "saved already");
    assert_eq!(session.tabs.len(), 2);
    assert_eq!(session.tabs[0].draft.url, "http://a.test/");
    assert_eq!(session.active, 1);
    assert!(!session.sidebar_open);

    let restored = App::restore(session.clone());
    assert_eq!(restored.session(), session);
    assert_eq!(restored.tab().title(), "Untitled Request");
    assert!(restored.tabs()[0].is_dirty(), "unsaved requests stay dirty");
}

#[test]
fn f07_ac5_restore_sanitizes_bad_values() {
    let app = App::restore(Session {
        tabs: vec![RequestDraft::default().into()],
        active: 7,
        split: f32::NAN,
        ..Session::default()
    });
    assert_eq!(app.active_index(), 0);
    assert!((app.split() - Session::DEFAULT_SPLIT).abs() < f32::EPSILON);
    let app = App::restore(Session {
        split: 5.0,
        ..Session::default()
    });
    assert_eq!(app.tabs().len(), 1);
    assert!((app.split() - 0.8).abs() < f32::EPSILON);
}

#[test]
fn f07_ac6_response_lands_in_the_sending_tab() {
    let mut app = App::default();
    let (first, id) = send(&mut app, "http://a.test/");
    app.update(Message::NewTab);
    app.update(finished(first, id, ok(b"for tab one")));
    assert!(matches!(app.tab().phase, Phase::Idle));
    assert!(matches!(app.tabs()[0].phase, Phase::Received(_)));
}

// --- F14 layout -----------------------------------------------------------------------

#[test]
fn f14_ac4_sidebar_and_split() {
    let mut app = App::default();
    assert!(app.sidebar_open());
    app.update(Message::ToggleSidebar);
    assert!(!app.sidebar_open());
    app.update(Message::SidebarSection(SidebarSection::History));
    assert!(app.sidebar_open(), "choosing a section opens the sidebar");
    assert_eq!(app.sidebar(), SidebarSection::History);
    app.update(Message::SplitDragged(0.05));
    assert!((app.split() - 0.2).abs() < f32::EPSILON);
    app.update(Message::SplitDragged(0.5));
    assert!((app.split() - 0.5).abs() < f32::EPSILON);
    app.update(Message::SplitDragged(f32::INFINITY));
    assert!((app.split() - 0.5).abs() < f32::EPSILON);
    assert!((app.session().split - 0.5).abs() < f32::EPSILON);
}

// --- end to end through the real client -----------------------------------------------------------

async fn round_trip(app: &mut App, client: &Client) {
    let Some(effect) = app.update(Message::SendPressed) else {
        panic!("expected a send, phase {:?}", app.tab().phase)
    };
    let message = run(effect, client.clone()).await.unwrap();
    app.update(message);
}

#[tokio::test]
async fn f05_ac1_end_to_end_post_with_headers_body_and_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(201).set_body_raw(r#"{"id":7}"#, "application/json"))
        .mount(&server)
        .await;
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    edit(&mut app, Edit::MethodSelected(Method::Post));
    edit(
        &mut app,
        Edit::Url(format!("{}/items?debug=1", server.uri())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Key("X-Trace".into())),
    );
    edit(
        &mut app,
        Edit::Row(Table::Headers, 0, RowEdit::Value("abc".into())),
    );
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawLanguage(RawLanguage::Json));
    edit(&mut app, Edit::RawText(r#"{"name":"x"}"#.into()));
    edit(&mut app, Edit::AuthKind(AuthKind::Bearer));
    edit(&mut app, Edit::Token("secret".into()));
    round_trip(&mut app, &client).await;

    let Phase::Received(r) = &app.tab().phase else {
        panic!("expected a response, got {:?}", app.tab().phase)
    };
    assert_eq!(r.response.status_line(), "201 Created");
    assert_eq!(r.format, BodyFormat::Pretty);
    assert!(r.response.timing.connect.is_some());
    let got = &server.received_requests().await.unwrap()[0];
    assert_eq!(got.url.query(), Some("debug=1"));
    assert_eq!(got.headers.get("x-trace").unwrap(), "abc");
    assert_eq!(got.headers.get("authorization").unwrap(), "Bearer secret");
    assert_eq!(got.headers.get("content-type").unwrap(), "application/json");
    assert_eq!(got.body, br#"{"name":"x"}"#);
}

#[tokio::test]
async fn f05_ac6_end_to_end_connection_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    edit(&mut app, Edit::Url(format!("http://127.0.0.1:{port}/")));
    round_trip(&mut app, &client).await;
    assert_eq!(
        failed_text(&app),
        format!("Connection refused by 127.0.0.1:{port}")
    );
}

#[tokio::test]
async fn f05_ac2_end_to_end_cancelled_before_send() {
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://127.0.0.1:9/".into()));
    let Some(effect) = app.update(Message::SendPressed) else {
        panic!()
    };
    app.update(Message::CancelPressed);
    let message = run(effect, client).await.unwrap();
    app.update(message);
    assert!(matches!(app.tab().phase, Phase::Cancelled));
}

#[tokio::test]
async fn f07_ac5_run_ignores_effects_the_ui_handles() {
    let client = Client::new(Config::default()).unwrap();
    let effect = Effect::SaveSession(Session::default());
    assert!(run(effect, client).await.is_none());
}

// --- edge cases found by mutation testing -------------------------------------------------

#[test]
fn f07_ac2_closing_activates_the_right_neighbour_and_keeps_the_active_tab() {
    let four = || {
        let mut app = App::default();
        for _ in 0..3 {
            app.update(Message::NewTab);
        }
        let ids: Vec<TabId> = app.tabs().iter().map(|t| t.id).collect();
        (app, ids)
    };
    // Closing a tab before the active one keeps the same tab active.
    let (mut app, ids) = four();
    app.update(Message::ActivateTab(ids[2]));
    app.update(Message::CloseTab(ids[0]));
    assert_eq!(app.tab().id, ids[2]);
    // Closing a tab after the active one changes nothing.
    let (mut app, ids) = four();
    app.update(Message::ActivateTab(ids[1]));
    app.update(Message::CloseTab(ids[3]));
    assert_eq!(app.tab().id, ids[1]);
    // Closing the active tab activates the one to its right.
    let (mut app, ids) = four();
    app.update(Message::ActivateTab(ids[1]));
    app.update(Message::CloseTab(ids[1]));
    assert_eq!(app.tab().id, ids[2]);
    // ... or the new last tab when it was last.
    let (mut app, ids) = four();
    app.update(Message::CloseTab(ids[3]));
    assert_eq!(app.tab().id, ids[2]);
}

#[test]
fn f06_ac6_only_bodies_that_look_like_json_are_pretty_printed() {
    // Valid JSON scalars in a text response stay raw.
    for body in [&b"42"[..], b"\"text\"", b"true"] {
        let r = received(Response {
            body: Bytes::copy_from_slice(body),
            ..response(200, "OK", "text/plain", b"")
        })
        .unwrap();
        assert_eq!(r.format, BodyFormat::Raw, "{body:?}");
    }
    // A header value mentioning json does not make the body JSON.
    let mut r = response(200, "OK", "text/plain", b"42");
    r.headers.push(Header {
        name: "x-format".into(),
        value: "json".into(),
    });
    assert_eq!(received(r).unwrap().format, BodyFormat::Raw);
    // An object in a text response is detected by content.
    let r = received(response(200, "OK", "text/plain", br#"{"a":1}"#)).unwrap();
    assert_eq!(r.format, BodyFormat::Pretty);
    let labels: Vec<String> = BodyFormat::ALL.iter().map(ToString::to_string).collect();
    assert_eq!(labels, ["Pretty", "Raw", "Hex", "Preview"]);
}

#[test]
fn f07_ac5_edits_that_change_nothing_do_not_trigger_a_save() {
    let mut app = App::default();
    edit(&mut app, Edit::ToggleAutoHeaders);
    app.update(Message::RequestTabSelected(RequestTab::Body));
    assert!(app.update(Message::Autosave).is_none());
    edit(&mut app, Edit::Url("http://a.test/".into()));
    assert!(matches!(
        app.update(Message::Autosave),
        Some(Effect::SaveSession(_))
    ));
}

#[test]
fn f03_ac5_file_for_a_far_row_index_goes_into_a_new_row() {
    let mut app = App::default();
    let tab = app.tab().id;
    app.update(Message::FilePicked {
        tab,
        target: FileTarget::FormField(5),
        path: Some(PathBuf::from("far.bin")),
    });
    let form = &app.tab().draft.body.form;
    assert_eq!(form.len(), 1);
    assert_eq!(form[0].file, Some(PathBuf::from("far.bin")));
}

#[test]
fn f01_ac8_removing_the_blank_row_does_nothing() {
    let mut app = App::default();
    edit(&mut app, Edit::Url("http://a.test/?a=1".into()));
    edit(&mut app, Edit::Row(Table::Params, 1, RowEdit::Remove));
    edit(&mut app, Edit::Row(Table::Headers, 0, RowEdit::Remove));
    assert_eq!(app.tab().draft.params.len(), 1);
    assert_eq!(app.tab().draft.url, "http://a.test/?a=1");
}
