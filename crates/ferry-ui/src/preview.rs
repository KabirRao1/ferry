//! The Preview web view (F22-AC1, F22-AC2): a native child window laid over
//! the body area, created when Preview is shown and destroyed when the user
//! leaves it. Linux has no web view; Preview offers Open in Browser there
//! (F22-AC3, ADR-0041).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ferry_app::{PreviewTarget, TabId};
use iced::Task;
use iced::widget::selector;

use crate::shell::Message;

/// Id of the area the web view covers.
pub const PREVIEW_AREA: &str = "preview-area";

/// Keeps the web view in step with [`ferry_app::App::preview`].
#[derive(Debug, Default)]
pub struct Preview {
    /// The response shown, if any.
    shown: Option<(TabId, u64)>,
    /// Bumped by every change. Creating a web view is asynchronous, so a
    /// create that finds a newer generation was overtaken and does nothing.
    generation: Arc<AtomicU64>,
}

impl Preview {
    /// Create, replace or destroy the web view so it shows `want`.
    pub fn sync(&mut self, want: Option<PreviewTarget>) -> Task<Message> {
        let key = want.as_ref().map(|t| (t.tab, t.send));
        if key == self.shown {
            return Task::none();
        }
        self.shown = key;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let latest = Arc::clone(&self.generation);
        // A later change runs its own destroy first, so a late one from an
        // older change must not remove the newer web view.
        let current = Arc::clone(&latest);
        let destroy = on_window(move |window| {
            if current.load(Ordering::SeqCst) == generation {
                platform::destroy(window);
            }
        })
        .discard();
        match want {
            None => destroy,
            Some(target) => destroy.chain(area_bounds().then(move |bounds| {
                let target = target.clone();
                let latest = Arc::clone(&latest);
                on_window(move |window| {
                    if latest.load(Ordering::SeqCst) != generation {
                        // Overtaken by a later change, which has its own
                        // destroy and, if needed, create.
                        return Ok(());
                    }
                    platform::create(window, &target, bounds)
                })
                .map(|result| match result {
                    Ok(()) => Message::PreviewMoved,
                    Err(error) => Message::App(ferry_app::Message::PreviewFailed(error)),
                })
            })),
        }
    }

    /// Move the web view over the body area after a layout change.
    pub fn place(&self) -> Task<Message> {
        if self.shown.is_none() {
            return Task::none();
        }
        area_bounds()
            .then(|bounds| match bounds {
                Some(bounds) => on_window(move |_| platform::place(bounds)),
                None => Task::none(),
            })
            .discard()
    }
}

/// Where the body area is, in window coordinates.
fn area_bounds() -> Task<Option<iced::Rectangle>> {
    selector::find(selector::id(PREVIEW_AREA))
        .map(|target| target.map(|t| t.visible_bounds().unwrap_or_else(|| t.bounds())))
}

/// Run `f` with the main window, on the thread that owns it.
fn on_window<T: Send + 'static>(
    f: impl FnOnce(&dyn iced::window::Window) -> T + Send + 'static,
) -> Task<T> {
    // `then` may call its closure more than once; `f` runs at most once.
    let mut f = Some(f);
    iced::window::latest().then(move |id| match (id, f.take()) {
        (Some(id), Some(f)) => iced::window::run(id, f),
        _ => Task::none(),
    })
}

/// Write `body` to a new temporary file with `extension` and open it with
/// the system's default application (F22-AC3).
///
/// # Errors
/// The text of a write or launch failure.
pub fn open_in_browser(body: &[u8], extension: &str) -> Result<(), String> {
    let path = write_temp(body, extension)?;
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(&path)
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not open {}: {e}", path.display()))
}

/// Write `body` to a new file in the temporary directory, named
/// `ferry-preview-<process>-<n>.<extension>`.
///
/// # Errors
/// The text of a write failure.
pub fn write_temp(body: &[u8], extension: &str) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "ferry-preview-{}-{n}.{extension}",
        std::process::id()
    ));
    std::fs::write(&path, body).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(any(windows, target_os = "macos"))]
mod platform {
    use std::borrow::Cow;
    use std::cell::RefCell;

    use ferry_app::PreviewTarget;
    use wry::http::{Response, StatusCode, header};
    use wry::{NewWindowResponse, Rect, WebContext, WebView, WebViewBuilder};

    /// The page's address: the custom `ferry` protocol, which WebView2
    /// spells as an http host.
    const URL: &str = if cfg!(windows) {
        "http://ferry.preview/"
    } else {
        "ferry://preview/"
    };

    struct Shown {
        // Dropped before the context it was built with.
        view: WebView,
        _context: WebContext,
    }

    thread_local! {
        static SHOWN: RefCell<Option<Shown>> = const { RefCell::new(None) };
    }

    fn rect(bounds: iced::Rectangle) -> Rect {
        Rect {
            position: wry::dpi::LogicalPosition::new(f64::from(bounds.x), f64::from(bounds.y))
                .into(),
            size: wry::dpi::LogicalSize::new(
                f64::from(bounds.width.max(0.0)),
                f64::from(bounds.height.max(0.0)),
            )
            .into(),
        }
    }

    /// Only the preview page itself may load as the top-level document.
    fn is_preview(url: &str) -> bool {
        url == URL
            || url
                .strip_prefix(URL)
                .is_some_and(|rest| rest.starts_with('?'))
    }

    pub(super) fn create(
        window: &dyn iced::window::Window,
        target: &PreviewTarget,
        bounds: Option<iced::Rectangle>,
    ) -> Result<(), String> {
        // Profile data goes to the temporary directory, never next to the
        // executable (which may not be writable).
        let mut context = WebContext::new(Some(std::env::temp_dir().join("ferry-webview")));
        let (mime, body) = (target.mime.clone(), target.body.clone());
        let view = WebViewBuilder::new_with_web_context(&mut context)
            // The body, from memory, with its own type (F22-AC2). Anything
            // else the page asks Ferry for does not exist.
            .with_custom_protocol("ferry".to_owned(), move |_, request| {
                let response = if request.uri().path() == "/" {
                    Response::builder()
                        .header(header::CONTENT_TYPE, mime.as_str())
                        .body(Cow::Owned(body.to_vec()))
                } else {
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Cow::Borrowed(&[][..]))
                };
                response.unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
            })
            .with_url(URL)
            .with_bounds(rect(bounds.unwrap_or_default()))
            // No windows, no navigation, no downloads, no scripts, and no
            // IPC handler: the page has no way to reach Ferry (F22-AC2).
            .with_navigation_handler(|url| is_preview(&url))
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_javascript_disabled()
            .with_devtools(false)
            .with_clipboard(false)
            .with_autoplay(false)
            .with_background_color((255, 255, 255, 255))
            .build_as_child(&window)
            .map_err(|e| format!("The web view could not start: {e}"))?;
        SHOWN.with(|shown| {
            *shown.borrow_mut() = Some(Shown {
                view,
                _context: context,
            });
        });
        Ok(())
    }

    pub(super) fn place(bounds: iced::Rectangle) {
        SHOWN.with(|shown| {
            if let Some(shown) = shown.borrow().as_ref() {
                let _ = shown.view.set_bounds(rect(bounds));
            }
        });
    }

    pub(super) fn destroy(_: &dyn iced::window::Window) {
        SHOWN.with(|shown| drop(shown.borrow_mut().take()));
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use ferry_app::PreviewTarget;

    pub(super) fn create(
        _: &dyn iced::window::Window,
        _: &PreviewTarget,
        _: Option<iced::Rectangle>,
    ) -> Result<(), String> {
        Err(ferry_app::PREVIEW_UNAVAILABLE.to_owned())
    }

    pub(super) fn place(_: iced::Rectangle) {}

    pub(super) fn destroy(_: &dyn iced::window::Window) {}
}
