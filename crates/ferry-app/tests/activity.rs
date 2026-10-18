//! What sends leave behind, as message flows: history (F10-AC1..AC5),
//! cookies (F17-AC2..AC6), the console (F18-AC1..AC4) and saving a
//! response as an example (F32-AC1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use bytes::Bytes;
use ferry_app::console::{CONSOLE_BODY, CONSOLE_MAX, ConsoleState, describe_body, preview};
use ferry_app::{
    App, BodyMode, CookieManager, Dialog, Edit, Effect, Finished, FixedAmbient, Header,
    HistoryEntry, HistoryOutcome, Jar, MemFs, Message, Phase, RequestBody, Response, SetCookie,
    TRUNCATED_NOTE, TabId, WorkspaceStore, collection, history_response, received,
};

/// 2026-10-17T19:07:20Z.
const NOW_MS: i64 = 1_792_264_040_000;

fn app() -> App {
    App::default().with_ambient(Box::new(FixedAmbient::new(
        UNIX_EPOCH + Duration::from_millis(1_792_264_040_000),
        0,
    )))
}

fn edit(app: &mut App, e: Edit) {
    app.update(Message::Edit(e));
}

/// Press Send on `url`; returns the effect's tab, id and jar snapshot.
fn send(app: &mut App, url: &str) -> (TabId, u64, Jar) {
    edit(app, Edit::Url(url.into()));
    match app.update(Message::SendPressed) {
        Some(Effect::Send {
            tab, id, cookies, ..
        }) => (tab, id, cookies.jar),
        other => panic!("{other:?}"),
    }
}

fn response(status: u16, body: &'static [u8]) -> Response {
    Response {
        status,
        reason: "OK".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "application/json".into(),
        }],
        body: Bytes::from_static(body),
        elapsed: Duration::from_millis(85),
        url: "https://api.test/users".into(),
        sent: vec![Header {
            name: "user-agent".into(),
            value: "ferry".into(),
        }],
        ..Response::default()
    }
}

fn finish(app: &mut App, tab: TabId, id: u64, result: Finished) -> Option<Effect> {
    app.update(Message::Finished {
        tab,
        id,
        result: Arc::new(result),
    })
}

fn ok(r: Response) -> Finished {
    Finished::Received(Box::new(received(r).unwrap()))
}

// --- F10 history --------------------------------------------------------------------

#[test]
fn f10_ac1_responses_are_recorded_with_their_body() {
    let mut app = app();
    edit(&mut app, Edit::Url("{{nope}}".into()));
    let (tab, id, _) = send(&mut app, "https://api.test/users");
    let effect = finish(&mut app, tab, id, ok(response(200, b"[1,2]")));
    let Some(Effect::History {
        history,
        body,
        remove,
    }) = effect
    else {
        panic!("{effect:?}");
    };
    assert!(remove.is_empty());
    let entry = &history.entries[0];
    assert_eq!(entry.at, NOW_MS);
    assert_eq!(entry.method, "GET");
    assert_eq!(entry.url, "https://api.test/users");
    assert_eq!(entry.request.url, "https://api.test/users");
    match &entry.outcome {
        HistoryOutcome::Response {
            status,
            elapsed_ms,
            size,
            truncated,
            headers,
            ..
        } => {
            assert_eq!(
                (*status, *elapsed_ms, *size, *truncated),
                (200, 85, 5, false)
            );
            assert_eq!(headers[0].name, "content-type");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(body, Some((entry.id, Bytes::from_static(b"[1,2]"))));
    assert_eq!(app.history().entries.len(), 1);
}

#[test]
fn f10_ac1_the_url_is_resolved_and_the_request_keeps_placeholders() {
    let mut app = app();
    edit(
        &mut app,
        Edit::Url("https://api.test/{{$timestamp}}".into()),
    );
    let (tab, id) = match app.update(Message::SendPressed) {
        Some(Effect::Send { tab, id, .. }) => (tab, id),
        other => panic!("{other:?}"),
    };
    finish(
        &mut app,
        tab,
        id,
        Finished::Failed("DNS lookup failed for api.test".into()),
    );
    let entry = &app.history().entries[0];
    assert_eq!(entry.url, "https://api.test/1792264040");
    assert_eq!(entry.request.url, "https://api.test/{{$timestamp}}");
    assert_eq!(
        entry.outcome,
        HistoryOutcome::Failed {
            error: "DNS lookup failed for api.test".into()
        }
    );
}

#[test]
fn f10_ac1_cancelled_sends_are_not_recorded() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://api.test/");
    app.update(Message::CancelPressed);
    assert!(finish(&mut app, tab, id, Finished::Cancelled).is_none());
    assert!(app.history().entries.is_empty());
}

#[test]
fn f10_ac1_results_for_closed_tabs_are_still_recorded() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://api.test/");
    app.update(Message::NewTab);
    app.update(Message::CloseTab(tab));
    finish(&mut app, tab, id, ok(response(200, b"x")));
    assert_eq!(app.history().entries.len(), 1);
    // Unknown ids are ignored.
    assert!(finish(&mut app, tab, 99, ok(response(200, b"x"))).is_none());
    assert_eq!(app.history().entries.len(), 1);
}

#[test]
fn f10_ac2_bodies_over_1_mb_are_truncated() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://api.test/big");
    let big = vec![b'a'; ferry_app::HISTORY_MAX_BODY + 10];
    let mut r = response(200, b"");
    r.body = Bytes::from(big);
    let Some(Effect::History { history, body, .. }) = finish(&mut app, tab, id, ok(r)) else {
        panic!()
    };
    assert_eq!(body.unwrap().1.len(), ferry_app::HISTORY_MAX_BODY);
    match &history.entries[0].outcome {
        HistoryOutcome::Response {
            truncated, size, ..
        } => {
            assert!(*truncated);
            assert_eq!(*size, (ferry_app::HISTORY_MAX_BODY + 10) as u64);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f10_ac2_dropped_entries_lose_their_bodies() {
    let mut history = ferry_app::History::default();
    for i in 0..100 {
        history.record(HistoryEntry {
            id: 0,
            at: 0,
            method: "GET".into(),
            url: format!("https://x/{i}"),
            request: ferry_app::RequestDraft::default(),
            outcome: HistoryOutcome::Failed { error: "e".into() },
        });
    }
    let mut app = app().with_history(history);
    let (tab, id, _) = send(&mut app, "https://api.test/");
    let Some(Effect::History { remove, .. }) = finish(&mut app, tab, id, ok(response(200, b"x")))
    else {
        panic!()
    };
    assert_eq!(remove, [0]);
}

fn with_entries() -> App {
    let mut app = app();
    for (url, status) in [
        ("https://api.test/users", 200),
        ("https://api.test/orders", 500),
    ] {
        let (tab, id, _) = send(&mut app, url);
        finish(&mut app, tab, id, ok(response(status, b"x")));
    }
    app
}

#[test]
fn f10_ac3_search_and_groups() {
    let mut app = with_entries();
    let groups = app.history_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].0, "Today");
    let urls: Vec<&str> = groups[0].1.iter().map(|e| e.url.as_str()).collect();
    assert_eq!(urls, ["https://api.test/orders", "https://api.test/users"]);
    app.update(Message::HistorySearch("USERS".into()));
    assert_eq!(app.history_search(), "USERS");
    assert_eq!(app.history_groups()[0].1.len(), 1);
    app.update(Message::HistorySearch("zzz".into()));
    assert!(app.history_groups().is_empty());
}

#[test]
fn f10_ac4_opening_an_entry_loads_its_response_in_a_new_tab() {
    let mut app = with_entries();
    let id = app.history().entries[1].id;
    let tabs = app.tabs().len();
    let Some(Effect::LoadHistory {
        tab,
        id: send,
        entry,
    }) = app.update(Message::OpenHistory(id))
    else {
        panic!()
    };
    assert_eq!(app.tabs().len(), tabs + 1);
    assert_eq!(app.tab().id, tab);
    assert_eq!(app.tab().draft.url, "https://api.test/users");
    assert!(app.tab().link.is_none());
    assert!(matches!(app.tab().phase, Phase::Sending { .. }));
    assert_eq!(entry.id, id);
    let result = history_response(&entry, Some(b"x".to_vec()));
    assert!(
        finish(&mut app, tab, send, result).is_none(),
        "loading is not a new send"
    );
    match &app.tab().phase {
        Phase::Received(r) => {
            assert_eq!(r.response.status, 200);
            assert_eq!(&r.response.body[..], b"x");
            assert_eq!(r.note, None);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(app.history().entries.len(), 2, "nothing new recorded");
    assert!(app.update(Message::OpenHistory(999)).is_none());
}

#[test]
fn f10_ac4_history_responses() {
    let entry = HistoryEntry {
        id: 3,
        at: NOW_MS,
        method: "GET".into(),
        url: "https://a.test/".into(),
        request: ferry_app::RequestDraft::default(),
        outcome: HistoryOutcome::Response {
            status: 404,
            reason: "Not Found".into(),
            elapsed_ms: 1500,
            size: 3_000_000,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            truncated: true,
        },
    };
    match history_response(&entry, Some(b"partial".to_vec())) {
        Finished::Received(r) => {
            assert_eq!(r.response.status_line(), "404 Not Found");
            assert_eq!(r.response.elapsed, Duration::from_millis(1500));
            assert_eq!(r.response.url, "https://a.test/");
            assert_eq!(r.note.as_deref(), Some(TRUNCATED_NOTE));
            assert_eq!(&r.response.body[..], b"partial");
        }
        other => panic!("{other:?}"),
    }
    match history_response(&entry, None) {
        Finished::Received(r) => assert!(r.response.body.is_empty()),
        other => panic!("{other:?}"),
    }
    let failed = HistoryEntry {
        outcome: HistoryOutcome::Failed {
            error: "Request timed out after 30 s".into(),
        },
        ..entry
    };
    assert!(matches!(
        history_response(&failed, None),
        Finished::Failed(text) if text == "Request timed out after 30 s"
    ));
}

#[test]
fn f10_ac5_delete_and_clear() {
    let mut app = with_entries();
    let first = app.history().entries[0].id;
    let Some(Effect::History {
        history,
        body,
        remove,
    }) = app.update(Message::DeleteHistory(first))
    else {
        panic!()
    };
    assert_eq!(history.entries.len(), 1);
    assert_eq!((body, remove), (None, vec![first]));
    assert!(app.update(Message::ClearHistory).is_none());
    assert_eq!(app.dialog(), Some(&Dialog::ClearHistory));
    app.update(Message::DialogCancel);
    assert_eq!(app.history().entries.len(), 1);
    app.update(Message::ClearHistory);
    let Some(Effect::History {
        history, remove, ..
    }) = app.update(Message::DialogConfirm)
    else {
        panic!()
    };
    assert!(history.entries.is_empty());
    assert_eq!(remove.len(), 1);
    assert!(app.dialog().is_none());
    assert!(app.update(Message::DeleteHistory(12345)).is_none());
}

// --- F17 cookies --------------------------------------------------------------------

#[test]
fn f17_ac2_sends_carry_a_snapshot_of_the_jar() {
    let mut jar = Jar::default();
    jar.store("sid=1", "api.test", "/", NOW_MS).unwrap();
    let mut app = app().with_cookies(jar.clone());
    let (_, _, snapshot) = send(&mut app, "https://api.test/");
    assert_eq!(snapshot, jar);
}

#[test]
fn f17_ac3_the_setting_reaches_the_request() {
    let mut app = app();
    assert!(app.tab().draft.options.cookies, "on by default");
    edit(&mut app, Edit::SendCookies(false));
    assert!(!app.tab().draft.options.cookies);
    edit(&mut app, Edit::Url("https://api.test/".into()));
    let Some(Effect::Send { request, .. }) = app.update(Message::SendPressed) else {
        panic!("no send");
    };
    assert!(!request.options.cookies, "ferry-http then skips the jar");
}

#[test]
fn f17_ac1_received_cookies_go_into_the_jar_and_are_saved() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://api.test/login");
    let mut r = response(200, b"");
    r.set_cookies = vec![
        SetCookie {
            url: "https://api.test/login".into(),
            header: "sid=abc".into(),
        },
        SetCookie {
            url: "https://api.test/login".into(),
            header: "bad".into(),
        },
    ];
    finish(&mut app, tab, id, ok(r));
    assert_eq!(app.cookies().cookies.len(), 1);
    assert_eq!(app.cookies().cookies[0].name, "sid");
    match app.update(Message::Autosave) {
        Some(Effect::Batch(effects)) => {
            assert!(
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::SaveCookies(j) if j.cookies.len() == 1))
            );
            assert!(effects.iter().any(|e| matches!(e, Effect::SaveSession(_))));
        }
        other => panic!("{other:?}"),
    }
    assert!(app.update(Message::Autosave).is_none(), "saved already");
}

#[test]
fn f17_ac6_cookies_alone_save_on_their_own() {
    let mut app = app();
    app.update(Message::Autosave);
    app.update(Message::OpenCookies);
    app.update(Message::CookieNewDomain("api.test".into()));
    app.update(Message::CookieAddDomain);
    app.update(Message::CookieEdit(None));
    app.update(Message::CookieText("a=1".into()));
    app.update(Message::CookieSave);
    assert!(matches!(
        app.update(Message::Autosave),
        Some(Effect::SaveCookies(_))
    ));
}

#[test]
fn f17_ac4_the_cookies_tab_lists_what_the_response_set() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://api.test/login");
    let mut r = response(200, b"");
    r.url = "https://api.test/v1/login".into();
    r.headers.push(Header {
        name: "set-cookie".into(),
        value: "sid=abc; HttpOnly; Max-Age=60".into(),
    });
    r.headers.push(Header {
        name: "set-cookie".into(),
        value: "broken".into(),
    });
    finish(&mut app, tab, id, ok(r));
    let Phase::Received(received) = &app.tab().phase else {
        panic!()
    };
    assert_eq!(received.cookies.len(), 1);
    let c = &received.cookies[0];
    assert_eq!(
        (c.name.as_str(), c.domain.as_str(), c.path.as_str()),
        ("sid", "api.test", "/v1")
    );
    assert!(c.http_only);
    assert_eq!(c.expires, Some(NOW_MS + 60_000));
}

fn manager(app: &App) -> CookieManager {
    match app.dialog() {
        Some(Dialog::Cookies(m)) => m.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f17_ac5_cookie_manager() {
    let mut jar = Jar::default();
    jar.store("sid=1; Path=/", "b.test", "/", NOW_MS).unwrap();
    let mut app = app().with_cookies(jar);
    app.update(Message::OpenCookies);
    assert_eq!(
        manager(&app).selected.as_deref(),
        Some("b.test"),
        "the first domain"
    );
    assert_eq!(app.cookie_domains(), ["b.test"]);
    // Add a domain, then a cookie to it.
    app.update(Message::CookieNewDomain("  A.test ".into()));
    app.update(Message::CookieAddDomain);
    assert_eq!(app.cookie_domains(), ["a.test", "b.test"]);
    assert_eq!(manager(&app).selected.as_deref(), Some("a.test"));
    assert_eq!(manager(&app).new_domain, "");
    app.update(Message::CookieEdit(None));
    assert_eq!(manager(&app).editor.unwrap().text, "name=value; Path=/");
    app.update(Message::CookieText("token".into()));
    app.update(Message::CookieSave);
    assert_eq!(
        manager(&app).editor.unwrap().error.as_deref(),
        Some("Enter a cookie as name=value; attributes")
    );
    app.update(Message::CookieText("token=t; Path=/api".into()));
    app.update(Message::CookieSave);
    assert!(manager(&app).editor.is_none());
    assert_eq!(
        app.cookies()
            .header_for("a.test", "/api/x", false, NOW_MS)
            .as_deref(),
        Some("token=t")
    );
    // Edit it: renaming replaces the old one.
    app.update(Message::CookieEdit(Some(("token".into(), "/api".into()))));
    assert_eq!(manager(&app).editor.unwrap().text, "token=t; Path=/api");
    app.update(Message::CookieText("renamed=t; Path=/api".into()));
    app.update(Message::CookieSave);
    let names: Vec<String> = app
        .cookies()
        .for_domain("a.test")
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, ["renamed"]);
    // A cookie for another domain is refused.
    app.update(Message::CookieEdit(None));
    app.update(Message::CookieText("x=1; Domain=other.test".into()));
    app.update(Message::CookieSave);
    assert!(
        manager(&app)
            .editor
            .unwrap()
            .error
            .unwrap()
            .contains("other.test")
    );
    app.update(Message::CookieEdit(None)); // starting over clears it
    assert_eq!(manager(&app).editor.unwrap().error, None);
    // Delete one, then a whole domain.
    app.update(Message::CookieDelete("renamed".into(), "/api".into()));
    assert!(app.cookies().for_domain("a.test").is_empty());
    app.update(Message::CookieDomain("b.test".into()));
    app.update(Message::CookieDeleteDomain("b.test".into()));
    assert!(app.cookies().cookies.is_empty());
    app.update(Message::CookieNewDomain("".into()));
    app.update(Message::CookieAddDomain);
    assert!(
        !app.cookie_domains().contains(&String::new()),
        "blank domains are ignored"
    );
    app.update(Message::DialogCancel);
    assert!(app.dialog().is_none());
}

// --- F18 console --------------------------------------------------------------------

#[test]
fn f18_ac1_sends_are_logged_and_completed() {
    let mut app = app();
    edit(&mut app, Edit::MethodSelected(ferry_app::Method::Post));
    edit(&mut app, Edit::BodyMode(BodyMode::Raw));
    edit(&mut app, Edit::RawText("{\"a\":1}".into()));
    let (tab, id, _) = send(&mut app, "https://api.test/items");
    let entry = app.console().entries.last().unwrap().clone();
    assert_eq!(entry.method, "POST");
    assert_eq!(entry.url, "https://api.test/items");
    assert_eq!(entry.request_body, "{\"a\":1}");
    assert!(
        entry
            .request_headers
            .iter()
            .any(|h| h.name == "Content-Type")
    );
    assert_eq!(entry.state, ConsoleState::Pending);
    assert_eq!(entry.summary(), "POST https://api.test/items");
    finish(&mut app, tab, id, ok(response(201, b"{\"id\":7}")));
    let entry = app.console().entries.last().unwrap();
    assert_eq!(
        entry.request_headers[0].name, "user-agent",
        "replaced by what was sent"
    );
    match &entry.state {
        ConsoleState::Done {
            status,
            elapsed,
            size,
            body,
            headers,
            ..
        } => {
            assert_eq!(
                (*status, *elapsed, *size),
                (201, Duration::from_millis(85), 8)
            );
            assert_eq!(body, "{\"id\":7}");
            assert_eq!(headers[0].name, "content-type");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(entry.summary(), "POST https://api.test/items 201 · 85 ms");
}

#[test]
fn f18_ac1_failures_and_cancels_are_logged() {
    let mut app = app();
    let (tab, id, _) = send(&mut app, "https://down.test/");
    finish(
        &mut app,
        tab,
        id,
        Finished::Failed("Connection refused by down.test:443".into()),
    );
    let entry = app.console().entries.last().unwrap();
    assert_eq!(
        entry.state,
        ConsoleState::Failed("Connection refused by down.test:443".into())
    );
    assert_eq!(entry.summary(), "GET https://down.test/ · Failed");
    assert!(entry.is_error());
    let (tab, id, _) = send(&mut app, "https://slow.test/");
    app.update(Message::CancelPressed);
    finish(&mut app, tab, id, Finished::Cancelled);
    let entry = app.console().entries.last().unwrap();
    assert_eq!(entry.state, ConsoleState::Cancelled);
    assert_eq!(entry.summary(), "GET https://slow.test/ · Cancelled");
    assert!(!entry.is_error());
}

#[test]
fn f18_ac1_body_descriptions() {
    assert_eq!(describe_body(&RequestBody::Empty), "");
    assert_eq!(
        describe_body(&RequestBody::Bytes(Bytes::from_static(b"hi"))),
        "hi"
    );
    assert_eq!(
        describe_body(&RequestBody::Bytes(Bytes::from_static(&[0, 1, 2, 255]))),
        "4 bytes"
    );
    assert_eq!(
        describe_body(&RequestBody::Multipart(vec![
            ferry_app::Part {
                name: "a".into(),
                value: ferry_app::PartValue::Text("1".into())
            },
            ferry_app::Part {
                name: "b".into(),
                value: ferry_app::PartValue::File(PathBuf::from("x.png"))
            },
        ])),
        "multipart/form-data, 2 fields"
    );
    assert_eq!(
        describe_body(&RequestBody::File(PathBuf::from("dir/photo.png"))),
        "file photo.png"
    );
    let long = "é".repeat(CONSOLE_BODY);
    let shown = preview(long.as_bytes());
    assert!(shown.len() < long.len());
    assert!(shown.ends_with("\n… (20.0 KB in total)"), "{shown}");
    assert!(shown.starts_with('é'));
    assert_eq!(preview(b""), "");
}

#[test]
fn f18_ac2_the_panel_toggles_and_entries_expand() {
    let mut app = app();
    assert!(!app.console().open);
    app.update(Message::ToggleConsole);
    assert!(app.console().open);
    let (tab, id, _) = send(&mut app, "https://a.test/");
    finish(&mut app, tab, id, ok(response(200, b"")));
    let entry = app.console().entries[0].id;
    app.update(Message::ConsoleExpand(entry));
    assert!(app.console().expanded.contains(&entry));
    app.update(Message::ConsoleExpand(entry));
    assert!(!app.console().expanded.contains(&entry));
    app.update(Message::ToggleConsole);
    assert!(!app.console().open);
}

#[test]
fn f18_ac3_filters_and_clear() {
    let mut app = app();
    for (url, result) in [
        ("https://a.test/ok", ok(response(200, b""))),
        ("https://a.test/missing", ok(response(404, b""))),
        ("https://b.test/down", Finished::Failed("x".into())),
    ] {
        let (tab, id, _) = send(&mut app, url);
        finish(&mut app, tab, id, result);
    }
    let urls = |app: &App| -> Vec<String> {
        app.console()
            .visible()
            .iter()
            .map(|e| e.url.clone())
            .collect()
    };
    assert_eq!(urls(&app).len(), 3);
    app.update(Message::ConsoleErrorsOnly(true));
    assert_eq!(
        urls(&app),
        ["https://a.test/missing", "https://b.test/down"]
    );
    app.update(Message::ConsoleFilter("A.TEST".into()));
    assert_eq!(urls(&app), ["https://a.test/missing"]);
    app.update(Message::ConsoleErrorsOnly(false));
    app.update(Message::ConsoleFilter("get".into()));
    assert_eq!(urls(&app).len(), 3, "the method matches too");
    app.update(Message::ConsoleClear);
    assert!(app.console().entries.is_empty());
}

#[test]
fn f18_ac4_only_the_newest_500_are_kept() {
    let mut app = app();
    for i in 0..CONSOLE_MAX + 3 {
        let (tab, id, _) = send(&mut app, &format!("https://a.test/{i}"));
        finish(&mut app, tab, id, Finished::Failed("x".into()));
    }
    let console = app.console();
    assert_eq!(console.entries.len(), CONSOLE_MAX);
    assert_eq!(console.entries[0].url, "https://a.test/3");
}

// --- F32-AC1 saving a response as an example --------------------------------------------

#[test]
fn f32_ac1_save_as_example_appends_to_the_request_file() {
    let fs = MemFs::new();
    fs.put(
        "/ws/collections/api/r.toml",
        "name = \"List\"\nurl = \"https://api.test/users\"\n",
    );
    let store = WorkspaceStore::new(fs.clone(), &PathBuf::from("/ws"));
    let mut app = app().with_workspace(store.load());
    app.update(Message::OpenItem("collections/api/r.toml".into()));
    let (tab, id, _) = send(&mut app, "https://api.test/users");
    finish(&mut app, tab, id, ok(response(200, b"[1]")));
    for round in 0..2 {
        let Some(Effect::Workspace { action, ops }) = app.update(Message::SaveAsExample) else {
            panic!()
        };
        assert_eq!(action, "save the example");
        store.apply(&ops).unwrap();
        app.update(Message::WorkspaceApplied {
            action,
            error: None,
            workspace: Arc::new(store.load()),
        });
        let file = app
            .workspace()
            .request("collections/api/r.toml")
            .unwrap()
            .clone();
        assert_eq!(file.examples.len(), round + 1);
        let example = &file.examples[round];
        assert_eq!(example.name, if round == 0 { "List" } else { "List (2)" });
        assert_eq!(example.status, 200);
        assert_eq!(&example.body[..], b"[1]");
        assert_eq!(example.request.url, "https://api.test/users");
    }
    assert!(app.is_expanded("collections/api/r.toml"));
    let _ = collection::file_name("x");
}

#[test]
fn f32_ac1_needs_a_linked_tab_with_a_response() {
    let mut app = app();
    assert!(app.update(Message::SaveAsExample).is_none());
    let (tab, id, _) = send(&mut app, "https://api.test/");
    finish(&mut app, tab, id, ok(response(200, b"")));
    assert!(app.update(Message::SaveAsExample).is_none(), "not linked");
}
