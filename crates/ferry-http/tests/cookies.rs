//! The cookie jar on real sends: F17-AC1..AC3, and what the console shows
//! (F18-AC1): headers as sent and the final URL.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::cookies::Jar;
use ferry_core::request::{Method, Request, SendOptions};
use ferry_core::response::{Header, Response};
use ferry_http::{CancellationToken, Client, Config, Cookies, Outcome};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 2026-10-17T19:07:20Z.
const NOW: i64 = 1_792_264_040_000;

fn client() -> Client {
    Client::new(Config::default()).unwrap()
}

fn get(url: &str) -> Request {
    Request::new(Method::Get, url::Url::parse(url).unwrap())
}

async fn send(request: &Request, jar: Option<Jar>) -> Response {
    let cookies = jar.map(|jar| Cookies { jar, now: NOW });
    match client()
        .send_with(request, cookies, &CancellationToken::new())
        .await
    {
        Outcome::Completed(r) => r,
        other => panic!("{other:?}"),
    }
}

fn sent<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response
        .sent
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

#[tokio::test]
async fn f17_ac1_set_cookie_headers_come_back_for_the_jar() {
    let server = MockServer::start().await;
    Mock::given(path("/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("set-cookie", "sid=abc; Path=/")
                .append_header("set-cookie", "theme=dark"),
        )
        .mount(&server)
        .await;
    let url = format!("{}/login", server.uri());
    let r = send(&get(&url), Some(Jar::default())).await;
    let headers: Vec<&str> = r.set_cookies.iter().map(|c| c.header.as_str()).collect();
    assert_eq!(headers, ["sid=abc; Path=/", "theme=dark"]);
    assert!(r.set_cookies.iter().all(|c| c.url == url));
}

#[tokio::test]
async fn f17_ac2_the_jar_sends_matching_cookies() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
        .mount(&server)
        .await;
    Mock::given(path("/me"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let mut jar = Jar::default();
    jar.store("sid=abc", "127.0.0.1", "/", NOW).unwrap();
    let r = send(&get(&format!("{}/me", server.uri())), Some(jar)).await;
    assert_eq!(r.status, 200);
    assert_eq!(sent(&r, "cookie"), Some("sid=abc"));
}

#[tokio::test]
async fn f17_ac1_cookies_set_on_a_redirect_hop_are_sent_on_the_next() {
    let server = MockServer::start().await;
    Mock::given(path("/login"))
        .respond_with(
            ResponseTemplate::new(302)
                .append_header("location", "/home")
                .append_header("set-cookie", "sid=fresh; Path=/"),
        )
        .mount(&server)
        .await;
    Mock::given(path("/home"))
        .and(header("cookie", "sid=fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_string("welcome"))
        .mount(&server)
        .await;
    Mock::given(path("/home"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let r = send(
        &get(&format!("{}/login", server.uri())),
        Some(Jar::default()),
    )
    .await;
    assert_eq!(r.status, 200);
    assert_eq!(&r.body[..], b"welcome");
    assert_eq!(r.url, format!("{}/home", server.uri()));
    assert_eq!(r.set_cookies.len(), 1);
    assert_eq!(r.set_cookies[0].url, format!("{}/login", server.uri()));
}

#[tokio::test]
async fn f17_ac3_without_the_jar_nothing_is_sent_or_kept() {
    let server = MockServer::start().await;
    Mock::given(path("/x"))
        .respond_with(ResponseTemplate::new(200).append_header("set-cookie", "a=1"))
        .mount(&server)
        .await;
    let mut jar = Jar::default();
    jar.store("old=1", "127.0.0.1", "/", NOW).unwrap();
    let mut request = get(&format!("{}/x", server.uri()));
    request.options = SendOptions {
        cookies: false,
        ..SendOptions::default()
    };
    let r = send(&request, Some(jar)).await;
    assert!(r.set_cookies.is_empty());
    assert_eq!(sent(&r, "cookie"), None);
    let received = server.received_requests().await.unwrap();
    assert!(received[0].headers.get("cookie").is_none());
    // No jar at all behaves the same.
    let r = send(&get(&format!("{}/x", server.uri())), None).await;
    assert!(r.set_cookies.is_empty());
}

#[tokio::test]
async fn f17_ac2_a_user_cookie_header_comes_first_with_the_jar_appended() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut jar = Jar::default();
    jar.store("sid=abc", "127.0.0.1", "/", NOW).unwrap();
    let mut request = get(&server.uri());
    request.headers.push(Header {
        name: "Cookie".into(),
        value: "mine=1".into(),
    });
    let r = send(&request, Some(jar)).await;
    let received = server.received_requests().await.unwrap();
    let cookies: Vec<&str> = received[0]
        .headers
        .get_all("cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(cookies, ["mine=1; sid=abc"]);
    assert_eq!(sent(&r, "cookie"), Some("mine=1; sid=abc"));
}

#[tokio::test]
async fn f18_ac1_sent_headers_include_what_the_client_adds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let mut request = Request::new(Method::Post, url::Url::parse(&server.uri()).unwrap());
    request.headers.push(Header {
        name: "X-Trace".into(),
        value: "t1".into(),
    });
    let r = send(&request, Some(Jar::default())).await;
    assert_eq!(sent(&r, "x-trace"), Some("t1"));
    assert_eq!(sent(&r, "user-agent"), Some(ferry_core::draft::USER_AGENT));
    assert_eq!(sent(&r, "accept"), Some("*/*"));
    assert_eq!(sent(&r, "accept-encoding"), Some("gzip, deflate, br, zstd"));
    assert_eq!(sent(&r, "cookie"), None, "an empty jar adds no Cookie");
    assert_eq!(r.url, format!("{}/", server.uri()));
}

#[tokio::test]
async fn f18_ac1_user_headers_replace_the_clients_defaults_in_the_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut request = get(&server.uri());
    request.headers.push(Header {
        name: "Accept".into(),
        value: "application/json".into(),
    });
    let r = send(&request, None).await;
    let accepts: Vec<&str> = r
        .sent
        .iter()
        .filter(|h| h.name.eq_ignore_ascii_case("accept"))
        .map(|h| h.value.as_str())
        .collect();
    assert_eq!(accepts, ["application/json"]);
}
