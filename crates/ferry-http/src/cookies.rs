//! The cookie jar during a send (F17, ADR-0038).
//!
//! reqwest consults one cookie provider per client, shared by every request.
//! The provider here reads and writes the jar of the request being sent,
//! which travels with the request future in a task-local, as the timing
//! marks do (ADR-0029). Redirect hops therefore store and send cookies, and
//! concurrent sends never share a jar.

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use ferry_core::cookies::Jar;
use ferry_core::response::SetCookie;
use reqwest::header::HeaderValue;

tokio::task_local! {
    static SCOPE: Scope;
}

/// The jar of one send and what happened to it.
#[derive(Clone)]
pub(crate) struct Scope(Arc<Mutex<State>>);

struct State {
    jar: Jar,
    now: i64,
    received: Vec<SetCookie>,
    /// The first `Cookie` header the jar produced: the first hop's.
    first_sent: Option<String>,
}

impl Scope {
    pub(crate) fn new(jar: Jar, now: i64) -> Self {
        Self(Arc::new(Mutex::new(State {
            jar,
            now,
            received: Vec::new(),
            first_sent: None,
        })))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The `Cookie` header for `url` from the jar.
    pub(crate) fn header_for(&self, url: &url::Url) -> Option<String> {
        let state = self.state();
        state.jar.header_for(
            url.host_str().unwrap_or_default(),
            url.path(),
            url.scheme() == "https",
            state.now,
        )
    }

    /// `(cookies received on every hop, the first hop's Cookie header)`.
    pub(crate) fn take(&self) -> (Vec<SetCookie>, Option<String>) {
        let mut state = self.state();
        (std::mem::take(&mut state.received), state.first_sent.take())
    }
}

/// Run `future` with `scope` as the jar for cookies, or with no jar.
pub(crate) async fn with<F: Future>(scope: Option<Scope>, future: F) -> F::Output {
    match scope {
        Some(scope) => SCOPE.scope(scope, future).await,
        None => future.await,
    }
}

/// The provider installed on every client.
pub(crate) struct Provider;

impl reqwest::cookie::CookieStore for Provider {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let Ok(scope) = SCOPE.try_with(Scope::clone) else {
            return;
        };
        let mut state = scope.state();
        let now = state.now;
        for value in headers {
            let header = String::from_utf8_lossy(value.as_bytes()).into_owned();
            // Ignored cookies (F17-AC1) are simply not stored.
            let _ = state
                .jar
                .store(&header, url.host_str().unwrap_or_default(), url.path(), now);
            state.received.push(SetCookie {
                url: url.to_string(),
                header,
            });
        }
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let scope = SCOPE.try_with(Scope::clone).ok()?;
        let header = scope.header_for(url)?;
        let mut state = scope.state();
        if state.first_sent.is_none() {
            state.first_sent = Some(header.clone());
        }
        HeaderValue::from_str(&header).ok()
    }
}
