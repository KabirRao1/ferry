//! App flows as message sequences: F01-AC1/AC5, F05-AC1/AC2/AC6/AC7, F06-AC1/AC4/AC10.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use ferry_app::body_view::{self, BodyView};
use ferry_app::{
    App, Client, Config, Effect, Finished, Header, Message, Method, Phase, Received, Response,
    ResponseTab, WRAP_BYTES, run,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn received(status: u16, reason: &str, body: &'static [u8]) -> Received {
    let body = Bytes::from_static(body);
    Received {
        response: Response {
            status,
            reason: reason.into(),
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain".into(),
            }],
            body: body.clone(),
            elapsed: Duration::from_millis(42),
        },
        body: BodyView::new(body, WRAP_BYTES).unwrap(),
    }
}

fn finished(id: u64, result: Finished) -> Message {
    Message::Finished {
        id,
        result: Arc::new(result),
    }
}

/// Type `url` and press Send; returns the send effect's id.
fn send(app: &mut App, url: &str) -> u64 {
    app.update(Message::UrlChanged(url.into()));
    match app.update(Message::SendPressed) {
        Some(Effect::Send { id, .. }) => id,
        None => panic!("expected a send effect, phase is {:?}", app.phase()),
    }
}

#[test]
fn f01_ac1_new_request_is_get_with_empty_url() {
    let app = App::default();
    assert_eq!(app.method(), &Method::Get);
    assert_eq!(app.url(), "");
    assert!(matches!(app.phase(), Phase::Idle));
    assert_eq!(app.tab(), ResponseTab::Body);
    assert!(!app.is_sending());
}

#[test]
fn f01_ac1_method_selection_is_used_for_send() {
    let mut app = App::default();
    app.update(Message::MethodSelected(Method::Delete));
    assert_eq!(app.method(), &Method::Delete);
    app.update(Message::UrlChanged("http://127.0.0.1/items/1".into()));
    let Some(Effect::Send { request, .. }) = app.update(Message::SendPressed) else {
        panic!("no effect")
    };
    assert_eq!(request.method, Method::Delete);
    assert_eq!(request.url.as_str(), "http://127.0.0.1/items/1");
}

#[test]
fn f01_ac5_invalid_url_blocks_send_with_message() {
    let mut app = App::default();
    app.update(Message::UrlChanged("example.com/users".into()));
    assert!(app.update(Message::SendPressed).is_none());
    match app.phase() {
        Phase::Failed(msg) => assert_eq!(msg, "Enter a URL starting with http:// or https://"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn f05_ac1_send_emits_effect_and_enters_sending() {
    let mut app = App::default();
    let id = send(&mut app, " https://api.example.test/users ");
    assert!(app.is_sending());
    assert!(matches!(app.phase(), Phase::Sending { id: i, .. } if *i == id));
}

#[test]
fn f05_ac1_each_send_gets_a_new_id() {
    let mut app = App::default();
    let first = send(&mut app, "http://127.0.0.1/");
    app.update(finished(first, Finished::Cancelled));
    let second = send(&mut app, "http://127.0.0.1/");
    assert_ne!(first, second);
}

#[test]
fn f05_ac7_send_while_sending_is_ignored() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    assert!(app.update(Message::SendPressed).is_none());
    assert!(matches!(app.phase(), Phase::Sending { id: i, .. } if *i == id));
}

#[test]
fn f05_ac2_cancel_fires_token_and_shows_cancelled() {
    let mut app = App::default();
    app.update(Message::UrlChanged("http://127.0.0.1/".into()));
    let Some(Effect::Send { cancel, .. }) = app.update(Message::SendPressed) else {
        panic!("no effect")
    };
    assert!(!cancel.is_cancelled());
    assert!(app.update(Message::CancelPressed).is_none());
    assert!(cancel.is_cancelled());
    assert!(matches!(app.phase(), Phase::Cancelled));
}

#[test]
fn f05_ac2_cancel_when_idle_does_nothing() {
    let mut app = App::default();
    assert!(app.update(Message::CancelPressed).is_none());
    assert!(matches!(app.phase(), Phase::Idle));
}

#[test]
fn f05_ac7_stale_results_are_ignored() {
    let mut app = App::default();
    let first = send(&mut app, "http://127.0.0.1/");
    app.update(Message::CancelPressed);
    let second = send(&mut app, "http://127.0.0.1/");
    app.update(finished(
        first,
        Finished::Received(received(200, "OK", b"old")),
    ));
    assert!(matches!(app.phase(), Phase::Sending { id, .. } if *id == second));
    app.update(finished(
        second,
        Finished::Received(received(201, "Created", b"new")),
    ));
    let Phase::Received(r) = app.phase() else {
        panic!("expected Received")
    };
    assert_eq!(r.response.status, 201);
}

#[test]
fn f05_ac7_result_after_cancel_is_ignored() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    app.update(Message::CancelPressed);
    app.update(finished(
        id,
        Finished::Received(received(200, "OK", b"late")),
    ));
    assert!(matches!(app.phase(), Phase::Cancelled));
}

#[test]
fn f06_ac1_received_response_is_shown() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    app.update(finished(
        id,
        Finished::Received(received(404, "Not Found", b"missing")),
    ));
    assert!(!app.is_sending());
    let Phase::Received(r) = app.phase() else {
        panic!("expected Received")
    };
    assert_eq!(r.response.status_line(), "404 Not Found");
    assert_eq!(r.body.visible(1)[0].1, "missing");
}

#[test]
fn f05_ac6_failure_is_shown() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    app.update(finished(
        id,
        Finished::Failed("DNS lookup failed for x".into()),
    ));
    assert!(matches!(app.phase(), Phase::Failed(m) if m == "DNS lookup failed for x"));
}

#[test]
fn f05_ac2_cancelled_result_is_shown() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    app.update(finished(id, Finished::Cancelled));
    assert!(matches!(app.phase(), Phase::Cancelled));
}

#[test]
fn f06_ac4_tab_selection() {
    let mut app = App::default();
    assert!(
        app.update(Message::TabSelected(ResponseTab::Headers))
            .is_none()
    );
    assert_eq!(app.tab(), ResponseTab::Headers);
    app.update(Message::TabSelected(ResponseTab::Body));
    assert_eq!(app.tab(), ResponseTab::Body);
}

#[test]
fn f06_ac10_body_messages_scroll_the_received_body() {
    let mut app = App::default();
    let id = send(&mut app, "http://127.0.0.1/");
    app.update(finished(
        id,
        Finished::Received(received(200, "OK", b"a\nb\nc")),
    ));
    assert!(
        app.update(Message::Body(body_view::Message::ScrolledBy(2)))
            .is_none()
    );
    let Phase::Received(r) = app.phase() else {
        panic!("expected Received")
    };
    assert_eq!(r.body.top(), 2);
}

#[test]
fn f06_ac10_body_messages_without_a_response_are_ignored() {
    let mut app = App::default();
    app.update(Message::Body(body_view::Message::ScrolledBy(2)));
    assert!(matches!(app.phase(), Phase::Idle));
}

/// Drive one full send: update → run(effect) → update.
async fn round_trip(app: &mut App, client: &Client, url: &str) {
    app.update(Message::UrlChanged(url.into()));
    let effect = app.update(Message::SendPressed).expect("send effect");
    let message = run(effect, client.clone()).await;
    assert!(app.update(message).is_none());
}

#[tokio::test]
async fn f05_ac1_end_to_end_get_shows_status_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello\nworld"))
        .mount(&server)
        .await;
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    round_trip(&mut app, &client, &format!("{}/hello", server.uri())).await;
    let Phase::Received(r) = app.phase() else {
        panic!("expected Received, got {:?}", app.phase())
    };
    assert_eq!(r.response.status_line(), "200 OK");
    let lines: Vec<String> = r
        .body
        .visible(10)
        .into_iter()
        .map(|(_, l)| l.into_owned())
        .collect();
    assert_eq!(lines, ["hello", "world"]);
}

#[tokio::test]
async fn f05_ac6_end_to_end_connection_refused() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    round_trip(&mut app, &client, &format!("http://127.0.0.1:{port}/")).await;
    assert!(
        matches!(app.phase(), Phase::Failed(m) if *m == format!("Connection refused by 127.0.0.1:{port}")),
        "{:?}",
        app.phase()
    );
}

#[tokio::test]
async fn f05_ac2_end_to_end_cancelled_before_send() {
    let client = Client::new(Config::default()).unwrap();
    let mut app = App::default();
    app.update(Message::UrlChanged("http://127.0.0.1:9/".into()));
    let Some(Effect::Send {
        id,
        request,
        cancel,
    }) = app.update(Message::SendPressed)
    else {
        panic!("no effect")
    };
    cancel.cancel();
    let message = run(
        Effect::Send {
            id,
            request,
            cancel,
        },
        client,
    )
    .await;
    let Message::Finished { id: got, result } = message else {
        panic!("expected Finished")
    };
    assert_eq!(got, id);
    assert!(matches!(*result, Finished::Cancelled));
}
