//! The request/response screen: state, messages, `update()` and effects
//! (F01-AC1/AC5, F05-AC1/AC2/AC6/AC7, F06-AC1..AC5).

use std::num::NonZeroU32;
use std::sync::Arc;

use ferry_core::request::{Method, Request, parse_url};
use ferry_core::response::Response;
use ferry_http::{CancellationToken, Client, Outcome};

use crate::body_view::{self, BodyView};

/// Display lines longer than this many bytes wrap (F06-AC9).
pub const WRAP_BYTES: NonZeroU32 = match NonZeroU32::new(4096) {
    Some(n) => n,
    None => NonZeroU32::MIN,
};

// CONTRACT
/// Where the current request is in its life cycle.
#[derive(Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    Sending {
        id: u64,
        cancel: CancellationToken,
    },
    Received(Box<Received>),
    /// User-facing error text.
    Failed(String),
    Cancelled,
}

// CONTRACT
/// A completed response plus its indexed body.
#[derive(Debug, Clone)]
pub struct Received {
    pub response: Response,
    pub body: BodyView,
}

// CONTRACT
/// Which part of the response is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseTab {
    #[default]
    Body,
    Headers,
}

// CONTRACT
/// How a send ended, as reported back to `update()`.
#[derive(Debug, Clone)]
pub enum Finished {
    Received(Received),
    /// User-facing error text.
    Failed(String),
    Cancelled,
}

// CONTRACT
/// Everything that can happen on the request screen.
#[derive(Debug, Clone)]
pub enum Message {
    MethodSelected(Method),
    UrlChanged(String),
    /// Send button, Enter in the URL field, or Ctrl/Cmd+Enter.
    SendPressed,
    CancelPressed,
    TabSelected(ResponseTab),
    Body(body_view::Message),
    /// A send started by [`Effect::Send`] with this `id` ended.
    Finished {
        id: u64,
        result: Arc<Finished>,
    },
}

// CONTRACT
/// Side effects requested by `update()`, executed by [`run`].
#[derive(Debug)]
pub enum Effect {
    Send {
        id: u64,
        request: Request,
        cancel: CancellationToken,
    },
}

// CONTRACT
/// The request screen's state.
#[derive(Debug, Default)]
pub struct App {
    method: Method,
    url: String,
    phase: Phase,
    tab: ResponseTab,
    last_id: u64,
}

impl App {
    #[must_use]
    pub fn method(&self) -> &Method {
        &self.method
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    #[must_use]
    pub fn tab(&self) -> ResponseTab {
        self.tab
    }

    #[must_use]
    pub fn is_sending(&self) -> bool {
        matches!(self.phase, Phase::Sending { .. })
    }

    // CONTRACT
    /// Apply `message`, returning a side effect to run, if any.
    pub fn update(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::MethodSelected(method) => self.method = method,
            Message::UrlChanged(url) => self.url = url,
            Message::SendPressed => return self.send(),
            Message::CancelPressed => {
                if let Phase::Sending { cancel, .. } = &self.phase {
                    cancel.cancel();
                    self.phase = Phase::Cancelled;
                }
            }
            Message::TabSelected(tab) => self.tab = tab,
            Message::Body(message) => {
                if let Phase::Received(received) = &mut self.phase {
                    received.body.update(message);
                }
            }
            Message::Finished { id, result } => {
                if matches!(self.phase, Phase::Sending { id: current, .. } if current == id) {
                    let result = Arc::try_unwrap(result).unwrap_or_else(|shared| (*shared).clone());
                    self.phase = match result {
                        Finished::Received(received) => Phase::Received(Box::new(received)),
                        Finished::Failed(text) => Phase::Failed(text),
                        Finished::Cancelled => Phase::Cancelled,
                    };
                }
            }
        }
        None
    }

    fn send(&mut self) -> Option<Effect> {
        if self.is_sending() {
            return None;
        }
        let url = match parse_url(&self.url) {
            Ok(url) => url,
            Err(error) => {
                self.phase = Phase::Failed(error.to_string());
                return None;
            }
        };
        self.last_id += 1;
        let cancel = CancellationToken::new();
        self.phase = Phase::Sending {
            id: self.last_id,
            cancel: cancel.clone(),
        };
        Some(Effect::Send {
            id: self.last_id,
            request: Request {
                method: self.method.clone(),
                url,
            },
            cancel,
        })
    }
}

// CONTRACT
/// Execute `effect` off the UI thread, including indexing the response body,
/// and report the result as a message.
pub async fn run(effect: Effect, client: Client) -> Message {
    let Effect::Send {
        id,
        request,
        cancel,
    } = effect;
    let result = match client.send(&request, &cancel).await {
        Outcome::Completed(response) => match BodyView::new(response.body.clone(), WRAP_BYTES) {
            Ok(body) => Finished::Received(Received { response, body }),
            Err(error) => Finished::Failed(error.to_string()),
        },
        Outcome::Failed(error) => Finished::Failed(error.to_string()),
        Outcome::Cancelled => Finished::Cancelled,
    };
    Message::Finished {
        id,
        result: Arc::new(result),
    }
}
