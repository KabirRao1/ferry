//! Per-request timing marks (F06-AC2, ADR-0029).
//!
//! The marks travel with the request future in a task-local, so the resolver
//! and connector hooks, which reqwest shares across requests, record into the
//! request that triggered them. Work reqwest moves to a background task (a
//! connect that loses a race with a pooled connection) records nothing.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

tokio::task_local! {
    static MARKS: Marks;
}

/// Accumulated DNS and connect durations for one request, summed across
/// redirect hops.
#[derive(Clone, Default)]
pub(crate) struct Marks(Arc<Mutex<Recorded>>);

#[derive(Default)]
struct Recorded {
    dns: Option<Duration>,
    connect: Option<Duration>,
}

impl Marks {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add_dns(&self, d: Duration) {
        let mut r = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        r.dns = Some(r.dns.unwrap_or_default() + d);
    }

    /// A whole connector call: DNS, TCP and TLS.
    pub(crate) fn add_connect(&self, d: Duration) {
        let mut r = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        r.connect = Some(r.connect.unwrap_or_default() + d);
    }

    /// `(dns, connect including dns)`.
    pub(crate) fn take(&self) -> (Option<Duration>, Option<Duration>) {
        let r = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        (r.dns, r.connect)
    }
}

/// Run `fut` with `marks` as the current request's marks.
pub(crate) fn scope<F: Future>(marks: &Marks, fut: F) -> impl Future<Output = F::Output> {
    MARKS.scope(marks.clone(), fut)
}

/// The marks of the request being polled, if any.
pub(crate) fn current() -> Option<Marks> {
    MARKS.try_with(Clone::clone).ok()
}

/// Times every connector call (reqwest's `connector_layer`).
#[derive(Clone, Copy)]
pub(crate) struct TimingLayer;

impl<S> tower_layer::Layer<S> for TimingLayer {
    type Service = TimingService<S>;

    fn layer(&self, inner: S) -> TimingService<S> {
        TimingService(inner)
    }
}

#[derive(Clone)]
pub(crate) struct TimingService<S>(S);

impl<S, R> tower_service::Service<R> for TimingService<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let marks = current();
        let connecting = self.0.call(request);
        Box::pin(async move {
            let start = Instant::now();
            let result = connecting.await;
            if let Some(marks) = marks {
                marks.add_connect(start.elapsed());
            }
            result
        })
    }
}
