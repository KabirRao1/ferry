//! Search in the response body (F06-AC8) and the `JSONPath` filter
//! (F22-AC4), as message flows.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;

use bytes::Bytes;
use ferry_app::body_view::Message as BodyMessage;
use ferry_app::{
    App, BodyFormat, Effect, FilterState, Finished, Header, Message, NO_RESULTS, Phase, Received,
    Response, TabId, received, run_find,
};
use ferry_core::json::MAX_FILTER_BODY;
use ferry_core::text::MAX_MATCHES;

fn response(content_type: &str, body: impl Into<Bytes>) -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: content_type.into(),
        }],
        body: body.into(),
        url: "https://api.test/".into(),
        ..Response::default()
    }
}

/// An app whose active tab shows `response`.
fn showing(response: Response) -> (App, TabId) {
    let mut app = App::default();
    app.update(Message::Edit(ferry_app::Edit::Url(
        "https://api.test/".into(),
    )));
    let Some(Effect::Send { tab, id, .. }) = app.update(Message::SendPressed) else {
        panic!("no send");
    };
    app.update(Message::Finished {
        tab,
        id,
        result: Arc::new(Finished::Received(Box::new(received(response).unwrap()))),
    });
    (app, tab)
}

fn shown(app: &App) -> &Received {
    match &app.tab().phase {
        Phase::Received(r) => r,
        other => panic!("{other:?}"),
    }
}

/// Apply a message and run the searches and filters it starts, until none
/// are left (a filter's results start a search when the bar is open).
fn drive(app: &mut App, message: Message) {
    let mut next = app.update(message);
    while let Some(effect) = next {
        let reply = run_find(effect).expect("a search or filter effect");
        next = app.update(reply);
    }
}

fn label(app: &App) -> String {
    shown(app).find.as_ref().unwrap().label()
}

fn find(app: &mut App, query: &str) {
    drive(app, Message::FindOpen);
    drive(app, Message::FindQuery(query.into()));
}

// --- F06-AC8 search ------------------------------------------------------------------

#[test]
fn f06_ac8_counts_matches_ignoring_ascii_case() {
    let (mut app, _) = showing(response("text/plain", "Error one\nerror two\nERROR three"));
    assert!(shown(&app).find.is_none());
    drive(&mut app, Message::FindOpen);
    assert_eq!(label(&app), "", "nothing typed yet");
    drive(&mut app, Message::FindQuery("error".into()));
    let find = shown(&app).find.as_ref().unwrap();
    assert_eq!(find.matches, vec![0, 10, 20]);
    assert_eq!(label(&app), "1 of 3");
    drive(&mut app, Message::FindQuery("warning".into()));
    assert_eq!(label(&app), NO_RESULTS);
    drive(&mut app, Message::FindQuery(String::new()));
    assert_eq!(label(&app), "");
    assert!(shown(&app).find.as_ref().unwrap().matches.is_empty());
}

#[test]
fn f06_ac8_searches_the_pretty_text_that_is_shown() {
    let (mut app, _) = showing(response("application/json", r#"{"name":"x"}"#));
    assert_eq!(shown(&app).format, BodyFormat::Pretty);
    drive(&mut app, Message::FindOpen);
    let effect = app.update(Message::FindQuery("\"name\": ".into()));
    let Some(Effect::Find { body, query, .. }) = &effect else {
        panic!("{effect:?}");
    };
    assert_eq!(body, shown(&app).view().body(), "the pretty-printed text");
    assert_eq!(query, "\"name\": ");
    assert!(shown(&app).find.as_ref().unwrap().searching);
    app.update(run_find(effect.unwrap()).unwrap());
    assert_eq!(
        label(&app),
        "1 of 1",
        "the key-value space is only in Pretty"
    );
}

#[test]
fn f06_ac8_next_and_previous_wrap_around() {
    let (mut app, _) = showing(response("text/plain", "a.a.a"));
    find(&mut app, "a");
    let current = |app: &App| shown(app).find.as_ref().unwrap().current;
    assert_eq!(current(&app), 0);
    drive(&mut app, Message::FindNext);
    drive(&mut app, Message::FindNext);
    assert_eq!(label(&app), "3 of 3");
    drive(&mut app, Message::FindNext);
    assert_eq!(current(&app), 0, "wraps to the first");
    drive(&mut app, Message::FindPrevious);
    assert_eq!(current(&app), 2, "wraps to the last");
    drive(&mut app, Message::FindPrevious);
    assert_eq!(label(&app), "2 of 3");
}

#[test]
fn f06_ac8_next_with_no_matches_does_nothing() {
    let (mut app, _) = showing(response("text/plain", "abc"));
    find(&mut app, "z");
    drive(&mut app, Message::FindNext);
    drive(&mut app, Message::FindPrevious);
    assert_eq!(label(&app), NO_RESULTS);
}

#[test]
fn f06_ac8_counting_stops_at_10000() {
    let (mut app, _) = showing(response("text/plain", "a".repeat(MAX_MATCHES + 5)));
    find(&mut app, "a");
    let find = shown(&app).find.as_ref().unwrap();
    assert_eq!(find.matches.len(), MAX_MATCHES);
    assert!(find.capped);
    assert_eq!(label(&app), "1 of 10000+");
}

#[test]
fn f06_ac8_late_results_for_an_older_query_are_ignored() {
    let (mut app, _) = showing(response("text/plain", "ab abc"));
    drive(&mut app, Message::FindOpen);
    let old = app.update(Message::FindQuery("ab".into())).unwrap();
    let new = app.update(Message::FindQuery("abc".into())).unwrap();
    app.update(run_find(old).unwrap());
    let find = shown(&app).find.as_ref().unwrap();
    assert!(find.searching && find.matches.is_empty(), "{find:?}");
    assert_eq!(label(&app), "");
    app.update(run_find(new).unwrap());
    assert_eq!(shown(&app).find.as_ref().unwrap().matches, vec![3]);
}

#[test]
fn f06_ac8_results_for_another_tab_or_a_closed_bar_are_ignored() {
    let (mut app, tab) = showing(response("text/plain", "abc"));
    drive(&mut app, Message::FindOpen);
    let effect = app.update(Message::FindQuery("b".into())).unwrap();
    drive(&mut app, Message::FindClose);
    assert!(shown(&app).find.is_none(), "Escape closes the bar");
    app.update(run_find(effect).unwrap());
    assert!(shown(&app).find.is_none());
    // A reply for a tab that is gone.
    app.update(Message::Found {
        tab: TabId(tab.0 + 99),
        job: 1,
        matches: Arc::new(vec![0]),
        capped: false,
    });
}

#[test]
fn f06_ac8_typing_with_the_bar_closed_does_nothing() {
    let (mut app, _) = showing(response("text/plain", "abc"));
    assert!(app.update(Message::FindQuery("a".into())).is_none());
    assert!(shown(&app).find.is_none());
}

#[test]
fn f06_ac8_no_find_bar_for_hex_or_without_a_response() {
    let (mut app, _) = showing(response("application/octet-stream", vec![0_u8, 1, 2, 0xff]));
    assert_eq!(shown(&app).format, BodyFormat::Hex);
    assert!(!shown(&app).searchable());
    drive(&mut app, Message::FindOpen);
    assert!(shown(&app).find.is_none());

    let mut idle = App::default();
    assert!(idle.update(Message::FindOpen).is_none());
    assert!(idle.update(Message::FindQuery("x".into())).is_none());
}

#[test]
fn f06_ac8_changing_format_searches_the_new_text_and_hex_closes_the_bar() {
    let (mut app, _) = showing(response("application/json", r#"{"a":"b"}"#));
    find(&mut app, "\"a\": ");
    assert_eq!(label(&app), "1 of 1");
    drive(&mut app, Message::BodyFormatSelected(BodyFormat::Raw));
    assert_eq!(label(&app), NO_RESULTS, "Raw has no space after the colon");
    drive(&mut app, Message::BodyFormatSelected(BodyFormat::Hex));
    assert!(shown(&app).find.is_none());
}

/// 1000 lines: `line 0` … `line 999`, with `needle` on lines 500 and 505.
fn long_body() -> String {
    (0..1000)
        .map(|i| {
            if i == 500 || i == 505 {
                format!("line {i} needle\n")
            } else {
                format!("line {i}\n")
            }
        })
        .collect()
}

#[test]
fn f06_ac8_the_current_match_is_scrolled_into_view() {
    let (mut app, _) = showing(response("text/plain", long_body()));
    drive(&mut app, Message::Body(BodyMessage::Resized(20)));
    find(&mut app, "needle");
    assert_eq!(label(&app), "1 of 2");
    // 19 lines are fully visible; the match lands a third of the way down.
    assert_eq!(shown(&app).view().top(), 500 - 6);
    drive(&mut app, Message::FindNext);
    assert_eq!(shown(&app).view().top(), 494, "line 505 is already visible");
    drive(&mut app, Message::Body(BodyMessage::ScrolledTo(0)));
    drive(&mut app, Message::FindPrevious);
    assert_eq!(shown(&app).view().top(), 494);
}

#[test]
fn f06_ac8_searching_starts_from_the_first_match_below_the_top_line() {
    let (mut app, _) = showing(response("text/plain", long_body()));
    drive(&mut app, Message::Body(BodyMessage::Resized(20)));
    drive(&mut app, Message::Body(BodyMessage::ScrolledTo(503)));
    find(&mut app, "needle");
    assert_eq!(label(&app), "2 of 2");
    assert_eq!(shown(&app).view().top(), 503, "already visible");
    drive(&mut app, Message::Body(BodyMessage::ScrolledTo(900)));
    drive(&mut app, Message::FindQuery("needl".into()));
    assert_eq!(label(&app), "1 of 2", "none below: the first");
}

#[test]
fn f06_ac8_the_viewport_size_reaches_every_format() {
    let (mut app, _) = showing(response("application/json", "[1]"));
    drive(&mut app, Message::Body(BodyMessage::Resized(7)));
    let r = shown(&app);
    assert_eq!(r.raw.rows(), 7);
    assert_eq!(r.hex.rows(), 7);
    assert_eq!(r.pretty.as_ref().unwrap().rows(), 7);
}

// --- F22-AC4 JSONPath filter ---------------------------------------------------------

const PEOPLE: &str = r#"{"people":[{"name":"Ada","age":36},{"name":"Alan","age":41}]}"#;

fn shown_text(app: &App) -> String {
    String::from_utf8(shown(app).view().body().to_vec()).unwrap()
}

#[test]
fn f22_ac4_a_filter_shows_its_results_as_a_pretty_array() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    assert!(shown(&app).filter_available());
    let effect = app.update(Message::FilterChanged("$.people[*].name".into()));
    assert!(matches!(shown(&app).filter.state, FilterState::Running));
    let Some(Effect::Filter {
        body, expression, ..
    }) = &effect
    else {
        panic!("{effect:?}");
    };
    assert_eq!(
        body,
        &shown(&app).response.body,
        "filters the original body"
    );
    assert_eq!(expression, "$.people[*].name");
    app.update(run_find(effect.unwrap()).unwrap());
    assert_eq!(shown_text(&app), "[\n  \"Ada\",\n  \"Alan\"\n]");
    assert_eq!(shown(&app).filter.expression, "$.people[*].name");
}

#[test]
fn f22_ac4_an_invalid_expression_says_why() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    drive(&mut app, Message::FilterChanged("$.people[".into()));
    let FilterState::Invalid(error) = &shown(&app).filter.state else {
        panic!("{:?}", shown(&app).filter.state);
    };
    assert!(error.starts_with("Invalid JSONPath: "), "{error}");
    assert!(
        shown_text(&app).contains("\"people\": ["),
        "the body stays shown"
    );
}

#[test]
fn f22_ac4_clearing_the_filter_shows_the_body_again() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    let pretty = shown_text(&app);
    drive(&mut app, Message::FilterChanged("$..age".into()));
    assert_eq!(shown_text(&app), "[\n  36,\n  41\n]");
    drive(&mut app, Message::FilterChanged("  ".into()));
    assert!(matches!(shown(&app).filter.state, FilterState::Off));
    assert_eq!(shown_text(&app), pretty);
}

#[test]
fn f22_ac4_late_results_for_an_older_expression_are_ignored() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    let old = app
        .update(Message::FilterChanged("$..name".into()))
        .unwrap();
    let new = app.update(Message::FilterChanged("$..age".into())).unwrap();
    app.update(run_find(new).unwrap());
    app.update(run_find(old).unwrap());
    assert_eq!(shown_text(&app), "[\n  36,\n  41\n]");
}

#[test]
fn f22_ac4_the_filter_applies_only_to_pretty() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    drive(&mut app, Message::FilterChanged("$..age".into()));
    drive(&mut app, Message::BodyFormatSelected(BodyFormat::Raw));
    assert_eq!(shown_text(&app), PEOPLE);
    drive(&mut app, Message::BodyFormatSelected(BodyFormat::Pretty));
    assert_eq!(shown_text(&app), "[\n  36,\n  41\n]");
}

#[test]
fn f22_ac4_the_find_bar_searches_the_filtered_text() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    find(&mut app, "a");
    assert_eq!(label(&app), "1 of 8", "name, Ada, Alan and age, twice each");
    drive(&mut app, Message::FilterChanged("$..name".into()));
    assert_eq!(label(&app), "1 of 4", "Ada and Alan");
    drive(&mut app, Message::FilterChanged(String::new()));
    assert_eq!(label(&app), "1 of 8");
}

#[test]
fn f22_ac4_the_results_inherit_the_viewport_size() {
    let (mut app, _) = showing(response("application/json", PEOPLE));
    drive(&mut app, Message::Body(BodyMessage::Resized(12)));
    drive(&mut app, Message::FilterChanged("$..name".into()));
    assert_eq!(shown(&app).view().rows(), 12);
}

#[test]
fn f22_ac4_no_filter_for_large_or_non_json_bodies() {
    let mut big = b"[".to_vec();
    big.resize(MAX_FILTER_BODY, b' ');
    big.push(b']');
    let (mut app, _) = showing(response("application/json", big));
    assert!(shown(&app).pretty.is_some());
    assert!(!shown(&app).filter_available());
    assert!(app.update(Message::FilterChanged("$".into())).is_none());
    assert!(shown(&app).filter.expression.is_empty());

    let mut exact = b"[".to_vec();
    exact.resize(MAX_FILTER_BODY - 1, b' ');
    exact.push(b']');
    let (app, _) = showing(response("application/json", exact));
    assert!(shown(&app).filter_available(), "10 MB exactly is allowed");

    let (mut app, _) = showing(response("text/plain", "plain"));
    assert!(!shown(&app).filter_available());
    assert!(app.update(Message::FilterChanged("$".into())).is_none());
}
