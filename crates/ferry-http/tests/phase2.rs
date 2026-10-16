//! Sending what Phase 2 builds: F02-AC1 headers, F03-AC2..AC6 bodies,
//! F05-AC4 redirects, F05-AC5 TLS verification, F06-AC2 timing.
//! Local servers only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use ferry_core::request::{Method, Part, PartValue, Request, RequestBody, SendOptions};
use ferry_core::response::{Header, Response};
use ferry_http::{CancellationToken, Client, Config, Outcome, Resolver, Resolving, SendError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn request(m: Method, url: &str) -> Request {
    Request::new(m, url.parse().unwrap())
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

fn h(name: &str, value: &str) -> Header {
    Header {
        name: name.into(),
        value: value.into(),
    }
}

/// A file in a fresh temp directory, removed on drop.
struct TempFile {
    dir: PathBuf,
    path: PathBuf,
}

impl TempFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ferry-http-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        Self { dir, path }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The single request the mock server received.
async fn received(server: &MockServer) -> wiremock::Request {
    let mut all = server.received_requests().await.unwrap();
    assert_eq!(all.len(), 1, "expected exactly one request");
    all.remove(0)
}

async fn echo_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

// --- F02 ----------------------------------------------------------------------

#[tokio::test]
async fn f02_ac1_headers_sent_in_order_with_duplicates() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut r = request(Method::Get, &server.uri());
    r.headers = vec![h("X-Multi", "one"), h("X-Single", "s"), h("X-Multi", "two")];
    completed(send(&client(), &r).await);
    let got = received(&server).await;
    let multi: Vec<&str> = got
        .headers
        .get_all("x-multi")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(multi, ["one", "two"]);
    assert_eq!(got.headers.get("x-single").unwrap(), "s");
}

#[tokio::test]
async fn f02_ac3_user_headers_replace_client_defaults() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut r = request(Method::Get, &server.uri());
    r.headers = vec![h("User-Agent", "curl/8.0"), h("Accept", "application/json")];
    completed(send(&client(), &r).await);
    let got = received(&server).await;
    let agents: Vec<_> = got.headers.get_all("user-agent").iter().collect();
    assert_eq!(agents, ["curl/8.0"]);
    assert_eq!(got.headers.get("accept").unwrap(), "application/json");
}

// --- F03 ----------------------------------------------------------------------

#[tokio::test]
async fn f03_ac2_raw_bytes_sent_exactly() {
    let server = echo_server().await;
    let mut r = request(Method::Post, &server.uri());
    r.headers = vec![h("Content-Type", "application/json")];
    r.body = RequestBody::Bytes(Bytes::from("{\"é\": [1, 2]}\r\n"));
    completed(send(&client(), &r).await);
    let got = received(&server).await;
    assert_eq!(got.body, "{\"é\": [1, 2]}\r\n".as_bytes());
    assert_eq!(got.headers.get("content-type").unwrap(), "application/json");
    assert_eq!(got.headers.get("content-length").unwrap(), "16");
}

#[tokio::test]
async fn f03_ac5_multipart_text_and_file_parts() {
    let file = TempFile::new("hello.txt", b"file contents\n");
    let server = echo_server().await;
    let mut r = request(Method::Post, &server.uri());
    r.body = RequestBody::Multipart(vec![
        Part {
            name: "title".into(),
            value: PartValue::Text("hi there".into()),
        },
        Part {
            name: "upload".into(),
            value: PartValue::File(file.path.clone()),
        },
    ]);
    completed(send(&client(), &r).await);
    let got = received(&server).await;
    let content_type = got.headers.get("content-type").unwrap().to_str().unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("multipart content type with boundary");
    let body = String::from_utf8(got.body).unwrap();
    let expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nhi there\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\nfile contents\n\r\n--{boundary}--\r\n"
    );
    assert_eq!(body, expected);
}

#[tokio::test]
async fn f03_ac5_unreadable_file_fails_with_path() {
    let missing = std::env::temp_dir().join("ferry-definitely-missing-file.bin");
    let mut r = request(Method::Post, "http://127.0.0.1:9/");
    r.body = RequestBody::Multipart(vec![Part {
        name: "f".into(),
        value: PartValue::File(missing.clone()),
    }]);
    match failed(send(&client(), &r).await) {
        SendError::File { path, detail } => {
            assert_eq!(path, missing.display().to_string());
            assert!(!detail.is_empty());
        }
        other => panic!("expected a file error, got {other:?}"),
    }
    r.body = RequestBody::File(missing.clone());
    let e = failed(send(&client(), &r).await);
    assert!(
        e.to_string()
            .starts_with(&format!("Could not read file {}: ", missing.display())),
        "{e}"
    );
}

#[tokio::test]
async fn f03_ac6_binary_file_is_the_body() {
    let data: Vec<u8> = (0..=255).collect();
    let file = TempFile::new("blob.bin", &data);
    let server = echo_server().await;
    let mut r = request(Method::Post, &server.uri());
    r.headers = vec![h("Content-Type", "application/octet-stream")];
    r.body = RequestBody::File(file.path.clone());
    completed(send(&client(), &r).await);
    assert_eq!(received(&server).await.body, data);
}

// --- F05-AC4 redirects ----------------------------------------------------------

async fn redirect_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/old"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("final"))
        .mount(&server)
        .await;
    Mock::given(path("/loop"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn f05_ac4_redirects_followed_by_default() {
    let server = redirect_server().await;
    let r = completed(
        send(
            &client(),
            &request(Method::Get, &format!("{}/old", server.uri())),
        )
        .await,
    );
    assert_eq!(r.status, 200);
    assert_eq!(&r.body[..], b"final");
}

#[tokio::test]
async fn f05_ac4_redirects_shown_as_is_when_off() {
    let server = redirect_server().await;
    let mut r = request(Method::Get, &format!("{}/old", server.uri()));
    r.options.follow_redirects = false;
    let c = client();
    let resp = completed(send(&c, &r).await);
    assert_eq!(resp.status, 302);
    assert!(
        resp.headers
            .iter()
            .any(|h| h.name == "location" && h.value == "/new")
    );
    // The same client still follows when asked.
    r.options.follow_redirects = true;
    assert_eq!(completed(send(&c, &r).await).status, 200);
}

#[tokio::test]
async fn f05_ac4_redirect_loop_stops_with_an_error() {
    let server = redirect_server().await;
    let e = failed(
        send(
            &client(),
            &request(Method::Get, &format!("{}/loop", server.uri())),
        )
        .await,
    );
    assert!(e.to_string().contains("redirect"), "{e}");
}

// --- F05-AC5 TLS verification ----------------------------------------------------

/// An HTTPS server with a self-signed certificate for `localhost`, answering
/// every connection with `200 ok`.
async fn self_signed_server() -> SocketAddr {
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert = certified.cert.der().clone();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(certified.signing_key.serialize_der().into());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = [0u8; 4096];
                let _ = tls.read(&mut buf).await;
                let _ = tls
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await;
                let _ = tls.shutdown().await;
            });
        }
    });
    addr
}

/// Resolves every name to one address.
struct Fixed(SocketAddr);

impl Resolver for Fixed {
    fn resolve(&self, _host: &str) -> Resolving {
        let addr = self.0;
        Box::pin(async move { Ok(vec![addr]) })
    }
}

fn client_resolving_to(addr: SocketAddr) -> Client {
    Client::new(Config {
        resolver: Arc::new(Fixed(addr)),
        ..Config::default()
    })
    .unwrap()
}

#[tokio::test]
async fn f05_ac5_untrusted_certificate_fails_naming_the_problem() {
    let addr = self_signed_server().await;
    let c = client_resolving_to(addr);
    let url = format!("https://localhost:{}/", addr.port());
    match failed(send(&c, &request(Method::Get, &url)).await) {
        SendError::Tls { detail } => {
            assert!(detail.to_lowercase().contains("certificate"), "{detail}");
        }
        other => panic!("expected a TLS error, got {other:?}"),
    }
}

#[tokio::test]
async fn f05_ac5_verification_off_accepts_untrusted_certificate() {
    let addr = self_signed_server().await;
    let c = client_resolving_to(addr);
    let mut r = request(Method::Get, &format!("https://localhost:{}/", addr.port()));
    r.options = SendOptions {
        verify_tls: false,
        ..SendOptions::default()
    };
    let resp = completed(send(&c, &r).await);
    assert_eq!(resp.status, 200);
    assert_eq!(&resp.body[..], b"ok");
}

// --- F06-AC2 timing -----------------------------------------------------------------

#[tokio::test]
async fn f06_ac2_timing_breakdown_and_reused_connection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1000)))
        .mount(&server)
        .await;
    let c = client_resolving_to(*server.address());
    let url = format!("http://timing.test:{}/", server.address().port());
    let first = completed(send(&c, &request(Method::Get, &url)).await);
    let t = first.timing;
    assert!(t.dns.is_some(), "a host name goes through DNS");
    assert!(t.connect.is_some(), "the first request connects");
    let parts = t.dns.unwrap() + t.connect.unwrap() + t.waiting + t.download;
    assert!(parts <= first.elapsed, "{parts:?} > {:?}", first.elapsed);

    let second = completed(send(&c, &request(Method::Get, &url)).await);
    assert_eq!(second.timing.dns, None, "pooled connection: no DNS");
    assert_eq!(second.timing.connect, None, "pooled connection: no connect");
}

#[tokio::test]
async fn f06_ac2_ip_literal_has_connect_but_no_dns() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let r = completed(send(&client(), &request(Method::Get, &server.uri())).await);
    assert_eq!(r.timing.dns, None);
    assert!(r.timing.connect.is_some());
}

#[tokio::test]
async fn f06_ac2_concurrent_requests_keep_their_own_marks() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_millis(50)))
        .mount(&server)
        .await;
    let c = client_resolving_to(*server.address());
    let url = format!("http://many.test:{}/", server.address().port());
    let sends: Vec<_> = (0..4)
        .map(|_| {
            let c = c.clone();
            let r = request(Method::Get, &url);
            tokio::spawn(async move { completed(c.send(&r, &CancellationToken::new()).await) })
        })
        .collect();
    for handle in sends {
        let resp = handle.await.unwrap();
        // Each concurrent request opened its own connection with one lookup.
        assert!(resp.timing.dns.is_some());
        assert!(resp.timing.connect.is_some());
        assert!(resp.timing.connect.unwrap() < resp.elapsed);
    }
}
