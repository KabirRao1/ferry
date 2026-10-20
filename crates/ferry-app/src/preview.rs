//! Preview of HTML, image, audio, video and PDF bodies (F22-AC1..AC3).
//!
//! The app decides what should be previewed; the UI owns the web view and
//! keeps it in step with [`App::preview`].

use bytes::Bytes;

use crate::app::{App, Effect, Message};
use crate::doc::Doc;
use crate::tab::{BodyFormat, Phase, Received, ResponseTab, TabId};

/// Shown in place of the web view on Linux or when it fails (F22-AC3).
pub const PREVIEW_UNAVAILABLE: &str = "Preview is not available here";

// CONTRACT
/// The MIME type (lowercase, without parameters) of a `Content-Type` that
/// Preview shows: HTML, images, audio, video and PDF (F22-AC1).
#[must_use]
pub fn previewable(content_type: &str) -> Option<String> {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let shown = matches!(
        essence.as_str(),
        "text/html" | "application/xhtml+xml" | "application/pdf"
    ) || ["image/", "audio/", "video/"]
        .iter()
        .any(|prefix| essence.len() > prefix.len() && essence.starts_with(prefix));
    shown.then_some(essence)
}

// CONTRACT
/// The file extension `Open in Browser` gives a body of this MIME type, so
/// the system opens it with the right application (F22-AC3); `bin` when
/// unknown. Only these fixed names are ever used.
#[must_use]
pub fn extension(mime: &str) -> &'static str {
    match mime {
        "text/html" => "html",
        "application/xhtml+xml" => "xhtml",
        "application/pdf" => "pdf",
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/avif" => "avif",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/webm" => "weba",
        "audio/aac" => "aac",
        "audio/flac" => "flac",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/ogg" => "ogv",
        "video/quicktime" => "mov",
        _ => "bin",
    }
}

// CONTRACT
/// What the web view should show: one response's body, served with its
/// type. A different `tab` or `send` is a different preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewTarget {
    pub tab: TabId,
    /// The send that produced the response.
    pub send: u64,
    pub mime: String,
    pub body: Bytes,
    /// A Visualize page: its inline scripts run (F22-AC5, ADR-0046).
    pub scripts: bool,
}

impl Received {
    // CONTRACT
    /// The response's MIME type, if Preview can show it (F22-AC1).
    #[must_use]
    pub fn preview_type(&self) -> Option<String> {
        self.response
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .and_then(|h| previewable(&h.value))
    }
}

impl App {
    // CONTRACT
    /// The preview to show now: the active request tab's response, on the
    /// Body tab, in the Preview format, with no dialog over it (the web view
    /// would cover it) and a web view available. `None` destroys any shown
    /// web view (F22-AC1).
    #[must_use]
    pub fn preview(&self) -> Option<PreviewTarget> {
        if !self.preview_available || self.dialog.is_some() || self.confirm_close().is_some() {
            return None;
        }
        let tab = self.tab();
        if tab.doc != Doc::Request || tab.response_tab != ResponseTab::Body {
            return None;
        }
        let Phase::Received(received) = &tab.phase else {
            return None;
        };
        match received.format {
            BodyFormat::Preview => Some(PreviewTarget {
                tab: tab.id,
                send: received.send,
                mime: received.preview_type()?,
                body: received.response.body.clone(),
                scripts: false,
            }),
            BodyFormat::Visualize => match &received.visualized {
                Some(Ok(page)) => Some(PreviewTarget {
                    tab: tab.id,
                    send: received.send,
                    mime: "text/html".to_owned(),
                    body: page.clone(),
                    scripts: true,
                }),
                _ => None,
            },
            _ => None,
        }
    }

    // CONTRACT
    /// Whether an embedded web view can show previews here (F22-AC3).
    #[must_use]
    pub fn preview_available(&self) -> bool {
        self.preview_available
    }

    // CONTRACT
    /// Why the web view could not start, if it failed.
    #[must_use]
    pub fn preview_error(&self) -> Option<&str> {
        self.preview_error.as_deref()
    }

    // CONTRACT
    /// Mark the embedded web view as unavailable, as on Linux (F22-AC3).
    #[must_use]
    pub fn without_preview(mut self) -> Self {
        self.preview_available = false;
        self
    }

    pub(crate) fn preview_message(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::PreviewFailed(error) => {
                self.preview_available = false;
                self.preview_error = Some(error);
                None
            }
            Message::OpenInBrowser => {
                let Phase::Received(received) = &self.tab().phase else {
                    return None;
                };
                if received.format == BodyFormat::Visualize {
                    let Some(Ok(page)) = &received.visualized else {
                        return None;
                    };
                    return Some(Effect::OpenInBrowser {
                        body: page.clone(),
                        extension: "html",
                    });
                }
                let mime = received.preview_type()?;
                Some(Effect::OpenInBrowser {
                    body: received.response.body.clone(),
                    extension: extension(&mime),
                })
            }
            _ => None,
        }
    }
}
