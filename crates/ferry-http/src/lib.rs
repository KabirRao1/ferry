//! HTTP execution for Ferry: sending, timing capture and cancellation (F05).
//!
//! Tested only against local servers. DNS goes through an injectable
//! [`Resolver`], so lookup failures are testable without a network.

mod timing;

use std::error::Error;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ferry_core::body::guess_content_type;
use ferry_core::request::{PartValue, RequestBody, SendOptions};
use ferry_core::response::{Header, Timing};

pub use ferry_core::error::SendError;
pub use ferry_core::request::Request;
pub use ferry_core::response::Response;
pub use tokio_util::sync::CancellationToken;

use timing::{Marks, TimingLayer};

/// Redirects followed when [`SendOptions::follow_redirects`] is on (F05-AC4).
pub const MAX_REDIRECTS: usize = 10;

/// Future returned by [`Resolver::resolve`].
pub type Resolving = Pin<Box<dyn Future<Output = io::Result<Vec<SocketAddr>>> + Send>>;

// CONTRACT
/// Resolves host names to socket addresses. IP-literal hosts never reach it.
pub trait Resolver: Send + Sync + 'static {
    /// Addresses for `host`. The port of each address is ignored.
    fn resolve(&self, host: &str) -> Resolving;
}

// CONTRACT
/// The operating system's resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str) -> Resolving {
        let host = host.to_owned();
        Box::pin(async move { Ok(tokio::net::lookup_host((host, 0)).await?.collect()) })
    }
}

// CONTRACT
/// Client settings.
#[derive(Clone)]
pub struct Config {
    /// Whole-request timeout (F05-AC3). Default 30 s.
    pub timeout: Duration,
    pub resolver: Arc<dyn Resolver>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            resolver: Arc::new(SystemResolver),
        }
    }
}

// CONTRACT
/// How a send ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed(Response),
    Failed(SendError),
    /// The cancellation token fired before the response completed (F05-AC2).
    Cancelled,
}

/// One reqwest client per combination of [`SendOptions`], built on first use.
/// Redirect policy and certificate checks are per client in reqwest.
struct Clients {
    config: Config,
    built: [OnceLock<reqwest::Client>; 4],
}

impl Clients {
    fn slot(options: SendOptions) -> usize {
        usize::from(options.follow_redirects) * 2 + usize::from(options.verify_tls)
    }

    fn get(&self, options: SendOptions) -> Result<&reqwest::Client, SendError> {
        let slot = &self.built[Self::slot(options)];
        if let Some(client) = slot.get() {
            return Ok(client);
        }
        let client = build(&self.config, options)?;
        Ok(slot.get_or_init(|| client))
    }
}

fn build(config: &Config, options: SendOptions) -> Result<reqwest::Client, SendError> {
    // Process-wide; an error only means a provider is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let redirect = if options.follow_redirects {
        reqwest::redirect::Policy::limited(MAX_REDIRECTS)
    } else {
        reqwest::redirect::Policy::none()
    };
    reqwest::Client::builder()
        .user_agent(ferry_core::draft::USER_AGENT)
        .timeout(config.timeout)
        .redirect(redirect)
        .tls_danger_accept_invalid_certs(!options.verify_tls)
        .dns_resolver(Arc::new(ResolverAdapter(Arc::clone(&config.resolver))))
        .connector_layer(TimingLayer)
        .build()
        .map_err(|e| SendError::Other {
            detail: describe(&e),
        })
}

// CONTRACT
/// A reusable HTTP client. Cheap to clone.
#[derive(Clone)]
pub struct Client {
    clients: Arc<Clients>,
    timeout: Duration,
}

impl Client {
    // CONTRACT
    /// Build a client. The client for the default options is built now, so
    /// a broken TLS setup shows up immediately.
    ///
    /// # Errors
    /// [`SendError::Other`] if the TLS backend cannot be initialized.
    pub fn new(config: Config) -> Result<Self, SendError> {
        let timeout = config.timeout;
        let clients = Clients {
            config,
            built: Default::default(),
        };
        clients.get(SendOptions::default())?;
        Ok(Self {
            clients: Arc::new(clients),
            timeout,
        })
    }

    // CONTRACT
    /// Send `request` and read the whole body, unless `cancel` fires first.
    /// Never panics; every failure becomes [`Outcome::Failed`].
    pub async fn send(&self, request: &Request, cancel: &CancellationToken) -> Outcome {
        if cancel.is_cancelled() {
            return Outcome::Cancelled;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => Outcome::Cancelled,
            result = self.exchange(request) => match result {
                Ok(response) => Outcome::Completed(response),
                Err(error) => Outcome::Failed(error),
            },
        }
    }

    async fn exchange(&self, request: &Request) -> Result<Response, SendError> {
        let client = self.clients.get(request.options)?;
        let method =
            reqwest::Method::from_bytes(request.method.as_str().as_bytes()).map_err(|e| {
                SendError::Other {
                    detail: e.to_string(),
                }
            })?;
        let mut builder = client.request(method, request.url.clone());
        for header in &request.headers {
            builder = builder.header(header.name.as_str(), header.value.as_str());
        }
        builder = match &request.body {
            RequestBody::Empty => builder,
            RequestBody::Bytes(bytes) => builder.body(bytes.clone()),
            RequestBody::File(path) => builder.body(read_file(path).await?),
            RequestBody::Multipart(parts) => {
                let mut form = reqwest::multipart::Form::new();
                for part in parts {
                    form = match &part.value {
                        PartValue::Text(text) => form.text(part.name.clone(), text.clone()),
                        PartValue::File(path) => {
                            form.part(part.name.clone(), file_part(path).await?)
                        }
                    };
                }
                builder.multipart(form)
            }
        };

        let marks = Marks::new();
        let start = Instant::now();
        let response = timing::scope(&marks, builder.send())
            .await
            .map_err(|e| self.classify(&e, &request.url))?;
        let headers_at = Instant::now();
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| Header {
                name: name.as_str().to_owned(),
                value: String::from_utf8_lossy(value.as_bytes()).into_owned(),
            })
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| self.classify(&e, &request.url))?;
        let end = Instant::now();
        let (dns, connect_total) = marks.take();
        let connect = connect_total.map(|total| total.saturating_sub(dns.unwrap_or_default()));
        Ok(Response {
            status: status.as_u16(),
            reason: status.canonical_reason().unwrap_or_default().to_owned(),
            headers,
            body,
            elapsed: end - start,
            timing: Timing {
                dns,
                connect,
                waiting: (headers_at - start).saturating_sub(connect_total.unwrap_or_default()),
                download: end - headers_at,
            },
        })
    }

    /// Map a transport error to the most specific user-facing error (F05-AC6).
    fn classify(&self, error: &reqwest::Error, url: &url::Url) -> SendError {
        if error.is_timeout() {
            return SendError::Timeout {
                after: self.timeout,
            };
        }
        for cause in causes(error) {
            if let Some(dns) = cause.downcast_ref::<DnsFailure>() {
                return SendError::Dns {
                    host: dns.host.clone(),
                };
            }
            if let Some(tls) = cause.downcast_ref::<rustls::Error>() {
                return SendError::Tls {
                    detail: tls.to_string(),
                };
            }
            if cause
                .downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::ConnectionRefused)
            {
                return SendError::ConnectionRefused {
                    host: url.host_str().unwrap_or_default().to_owned(),
                    port: url.port_or_known_default().unwrap_or_default(),
                };
            }
        }
        SendError::Other {
            detail: describe(error),
        }
    }
}

/// Read a body file (F03-AC5, F03-AC6).
async fn read_file(path: &Path) -> Result<Vec<u8>, SendError> {
    tokio::fs::read(path).await.map_err(|e| SendError::File {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

/// A multipart file part with its name and guessed content type (F03-AC5).
async fn file_part(path: &Path) -> Result<reqwest::multipart::Part, SendError> {
    let bytes = read_file(path).await?;
    let part = reqwest::multipart::Part::bytes(bytes);
    let part = match path.file_name() {
        Some(name) => part.file_name(name.to_string_lossy().into_owned()),
        None => part,
    };
    part.mime_str(guess_content_type(path))
        .map_err(|e| SendError::Other {
            detail: describe(&e),
        })
}

/// Adapts a [`Resolver`] to reqwest, timing lookups and tagging failures so
/// they classify as DNS errors.
struct ResolverAdapter(Arc<dyn Resolver>);

impl reqwest::dns::Resolve for ResolverAdapter {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let lookup = self.0.resolve(&host);
        let marks = timing::current();
        Box::pin(async move {
            let start = Instant::now();
            let result = lookup.await;
            if let Some(marks) = marks {
                marks.add_dns(start.elapsed());
            }
            match result {
                Ok(addrs) if !addrs.is_empty() => {
                    Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
                }
                _ => Err(Box::new(DnsFailure { host }) as Box<dyn Error + Send + Sync>),
            }
        })
    }
}

#[derive(Debug)]
struct DnsFailure {
    host: String,
}

impl std::fmt::Display for DnsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DNS lookup failed for {}", self.host)
    }
}

impl Error for DnsFailure {}

/// `error` and all its causes. `io::Error::source` skips the error it wraps,
/// so wrapped errors are visited explicitly through `get_ref`.
fn causes<'a>(error: &'a (dyn Error + 'static)) -> Vec<&'a (dyn Error + 'static)> {
    let mut out = Vec::new();
    let mut next = Some(error);
    while let Some(e) = next {
        out.push(e);
        next = match e.downcast_ref::<io::Error>().and_then(io::Error::get_ref) {
            Some(inner) => Some(inner as &(dyn Error + 'static)),
            None => e.source(),
        };
    }
    out
}

/// One line describing `error` and its causes, without repeats.
fn describe(error: &(dyn Error + 'static)) -> String {
    let mut parts: Vec<String> = Vec::new();
    for cause in causes(error) {
        let text = cause.to_string();
        if !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
    }
    parts.join(": ")
}
