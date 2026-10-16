//! F05-AC1/AC2/AC3/AC6, F06-AC1/AC4 against local servers only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferry_core::request::{Method, Request};
use ferry_core::response::Response;
use ferry_http::{CancellationToken, Client, Config, Outcome, Resolver, Resolving, SendError};
use tokio::io::AsyncWriteExt;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(m: Method, url: &str) -> Request {
    Request {
        method: m,
        url: url.parse().unwrap(),
    }
}

fn client() -> Client {
    Client::new(Config::default()).unwrap()
}

async fn send(c: &Client, r: &Request) -> Outcome {
    c.send(r, &CancellationToken::new()).await
}

fn completed(o: Outcome) -> Response {
    match o {
        Outcome::Completed(r) => r,
        other => panic!("expected a response, got {other:?}"),
    }
}

fn failed(o: Outcome) -> SendError {
    match o {
        Outcome::Failed(e) => e,
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// Resolves names from a fixed table; anything else fails like an unknown host.
struct TableResolver(HashMap<&'static str, SocketAddr>);

impl Resolver for TableResolver {
    fn resolve(&self, host: &str) -> Resolving {
        let found = self.0.get(host).copied();
        Box::pin(async move {
            found
                .map(|a| vec![a])
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no such host"))
        })
    }
}

#[tokio::test]
async fn f05_ac1_get_returns_status_headers_body_and_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users"))
        .respond_with(
            // `set_body_string` would force text/plain; set the MIME type explicitly.
            ResponseTemplate::new(200).set_body_raw("[1,2,3]", "application/json"),
        )
        .mount(&server)
        .await;
    let url = format!("{}/users", server.uri());
    let r = completed(send(&client(), &request(Method::Get, &url)).await);
    assert_eq!(r.status, 200);
    assert_eq!(r.status_line(), "200 OK");
    assert_eq!(&r.body[..], b"[1,2,3]");
    assert!(
        r.headers
            .iter()
            .any(|h| h.name == "content-type" && h.value == "application/json")
    );
    assert!(r.elapsed > Duration::ZERO);
}

#[tokio::test]
async fn f05_ac1_sends_selected_method() {
    let server = MockServer::start().await;
    for m in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS", "PURGE"] {
        Mock::given(method(m))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
    }
    let c = client();
    for m in [
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
        Method::Custom("PURGE".into()),
    ] {
        let r = completed(send(&c, &request(m.clone(), &server.uri())).await);
        assert_eq!(r.status, 204, "{m}");
    }
}

#[tokio::test]
async fn f05_ac1_head_has_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let r = completed(send(&client(), &request(Method::Head, &server.uri())).await);
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty());
}

#[tokio::test]
async fn f05_ac1_identifies_as_ferry() {
    let server = MockServer::start().await;
    Mock::given(header(
        "user-agent",
        concat!("ferry/", env!("CARGO_PKG_VERSION")),
    ))
    .respond_with(ResponseTemplate::new(200))
    .mount(&server)
    .await;
    let r = completed(send(&client(), &request(Method::Get, &server.uri())).await);
    assert_eq!(r.status, 200, "user-agent header missing or wrong");
}

#[tokio::test]
async fn f05_ac1_decodes_gzip_body() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(b"hello gzip").unwrap();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-encoding", "gzip")
                .set_body_bytes(gz.finish().unwrap()),
        )
        .mount(&server)
        .await;
    let r = completed(send(&client(), &request(Method::Get, &server.uri())).await);
    assert_eq!(&r.body[..], b"hello gzip");
}

#[tokio::test]
async fn f06_ac1_status_without_reason_phrase() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(299))
        .mount(&server)
        .await;
    let r = completed(send(&client(), &request(Method::Get, &server.uri())).await);
    assert_eq!(r.status_line(), "299");
}

#[tokio::test]
async fn f06_ac4_duplicate_headers_kept_in_order() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("x-dup", "first")
                .append_header("x-dup", "second"),
        )
        .mount(&server)
        .await;
    let r = completed(send(&client(), &request(Method::Get, &server.uri())).await);
    let dups: Vec<&str> = r
        .headers
        .iter()
        .filter(|h| h.name == "x-dup")
        .map(|h| h.value.as_str())
        .collect();
    assert_eq!(dups, ["first", "second"]);
}

#[tokio::test]
async fn f05_ac6_hostnames_go_through_the_resolver() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("via resolver"))
        .mount(&server)
        .await;
    let resolver = TableResolver(HashMap::from([("api.ferry.test", *server.address())]));
    let c = Client::new(Config {
        resolver: Arc::new(resolver),
        ..Config::default()
    })
    .unwrap();
    let url = format!("http://api.ferry.test:{}/", server.address().port());
    let r = completed(send(&c, &request(Method::Get, &url)).await);
    assert_eq!(&r.body[..], b"via resolver");
}

#[tokio::test]
async fn f05_ac6_dns_failure_names_host() {
    let c = Client::new(Config {
        resolver: Arc::new(TableResolver(HashMap::new())),
        ..Config::default()
    })
    .unwrap();
    let e = failed(send(&c, &request(Method::Get, "http://nowhere.ferry.test/x")).await);
    assert_eq!(
        e,
        SendError::Dns {
            host: "nowhere.ferry.test".into()
        }
    );
    assert_eq!(e.to_string(), "DNS lookup failed for nowhere.ferry.test");
}

#[tokio::test]
async fn f05_ac6_system_resolver_resolves_localhost() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let url = format!("http://localhost:{}/", server.address().port());
    let r = completed(send(&client(), &request(Method::Get, &url)).await);
    assert_eq!(r.status, 200);
}

#[tokio::test]
async fn f05_ac6_connection_refused_names_host_and_port() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("http://127.0.0.1:{port}/");
    let e = failed(send(&client(), &request(Method::Get, &url)).await);
    assert_eq!(
        e,
        SendError::ConnectionRefused {
            host: "127.0.0.1".into(),
            port
        }
    );
}

#[tokio::test]
async fn f05_ac6_tls_handshake_failure() {
    // A plain-HTTP server cannot complete a TLS handshake.
    let server = MockServer::start().await;
    let url = format!("https://127.0.0.1:{}/", server.address().port());
    match failed(send(&client(), &request(Method::Get, &url)).await) {
        SendError::Tls { detail } => assert!(!detail.is_empty()),
        other => panic!("expected a TLS error, got {other:?}"),
    }
}

#[tokio::test]
async fn f05_ac6_truncated_response_is_reported() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        // Promise 100 bytes, send 5, hang up.
        let _ = sock
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\nhello")
            .await;
    });
    let url = format!("http://{addr}/");
    match failed(send(&client(), &request(Method::Get, &url)).await) {
        SendError::Other { detail } => assert!(!detail.is_empty()),
        other => panic!("expected Other, got {other:?}"),
    }
}

#[tokio::test]
async fn f05_ac3_timeout_names_duration() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let c = Client::new(Config {
        timeout: Duration::from_millis(200),
        ..Config::default()
    })
    .unwrap();
    let e = failed(send(&c, &request(Method::Get, &server.uri())).await);
    assert_eq!(
        e,
        SendError::Timeout {
            after: Duration::from_millis(200)
        }
    );
}

#[tokio::test]
async fn f05_ac2_cancel_mid_flight_stops_promptly() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let _ = accepted_tx.send(());
        // Never respond; hold the socket open.
        std::future::pending::<()>().await;
        drop(sock);
    });
    let c = client();
    let cancel = CancellationToken::new();
    let req = request(Method::Get, &format!("http://{addr}/"));
    let task = {
        let (c, cancel) = (c.clone(), cancel.clone());
        tokio::spawn(async move { c.send(&req, &cancel).await })
    };
    accepted_rx.await.unwrap();
    let t = Instant::now();
    cancel.cancel();
    assert_eq!(task.await.unwrap(), Outcome::Cancelled);
    assert!(
        t.elapsed() < Duration::from_secs(5),
        "cancel took {:?}",
        t.elapsed()
    );
}

#[tokio::test]
async fn f05_ac2_already_cancelled_token_sends_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let outcome = client()
        .send(&request(Method::Get, &server.uri()), &cancel)
        .await;
    assert_eq!(outcome, Outcome::Cancelled);
}
