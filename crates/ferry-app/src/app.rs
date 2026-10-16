//! The workbench: open tabs, layout, messages, `update()` and effects
//! (F05, F06, F07, F14).

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;

use ferry_core::draft::RequestDraft;
use ferry_core::json;
use ferry_core::request::Request;
use ferry_core::session::Session;
use ferry_core::text::looks_binary;
use ferry_http::{CancellationToken, Client, Outcome};

use crate::body_view::{self, BodyView};
use crate::tab::{
    BodyFormat, Edit, FileTarget, Phase, Received, RequestTab, ResponseTab, Tab, TabId,
};

/// Display lines longer than this many bytes wrap (F06-AC9).
pub const WRAP_BYTES: NonZeroU32 = match NonZeroU32::new(4096) {
    Some(n) => n,
    None => NonZeroU32::MIN,
};

// CONTRACT
/// How a send ended, as reported back to `update()`.
#[derive(Debug, Clone)]
pub enum Finished {
    Received(Box<Received>),
    /// User-facing error text.
    Failed(String),
    Cancelled,
}

// CONTRACT
/// The left sidebar's sections (F14-AC1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSection {
    #[default]
    Collections,
    Environments,
    History,
}

impl SidebarSection {
    pub const ALL: [SidebarSection; 3] = [
        SidebarSection::Collections,
        SidebarSection::Environments,
        SidebarSection::History,
    ];
}

// CONTRACT
/// Everything that can happen in the main window.
#[derive(Debug, Clone)]
pub enum Message {
    /// + button or Ctrl/Cmd+T (F07-AC1).
    NewTab,
    /// ×, middle-click or Ctrl/Cmd+W on a tab (F07-AC2).
    CloseTab(TabId),
    CloseActiveTab,
    /// Answer to `Discard unsaved changes?`: `true` discards.
    ConfirmClose(bool),
    ActivateTab(TabId),
    /// Ctrl+Tab / Ctrl+Shift+Tab (F07-AC3).
    NextTab,
    PreviousTab,
    /// Pressing on a tab starts a possible drag; entering another tab while
    /// dragging moves it there; releasing ends it (F07-AC3).
    TabPressed(TabId),
    TabEntered(TabId),
    TabReleased,
    ToggleSidebar,
    SidebarSection(SidebarSection),
    /// The request/response divider was dragged to this share of the height.
    SplitDragged(f32),
    /// Periodic tick: save the session if it changed (F07-AC5).
    Autosave,
    /// An edit to the active tab's request.
    Edit(Edit),
    /// Send button, Enter in the URL field, or Ctrl/Cmd+Enter.
    SendPressed,
    CancelPressed,
    RequestTabSelected(RequestTab),
    ResponseTabSelected(ResponseTab),
    BodyFormatSelected(BodyFormat),
    Body(body_view::Message),
    /// Ask for a file for the active tab.
    PickFile(FileTarget),
    /// The file dialog closed; `None` if cancelled.
    FilePicked {
        tab: TabId,
        target: FileTarget,
        path: Option<PathBuf>,
    },
    /// A send started by [`Effect::Send`] ended.
    Finished {
        tab: TabId,
        id: u64,
        result: Arc<Finished>,
    },
}

// CONTRACT
/// Side effects requested by `update()`, executed by the UI.
#[derive(Debug)]
pub enum Effect {
    /// Run with [`run`].
    Send {
        tab: TabId,
        id: u64,
        request: Request,
        cancel: CancellationToken,
    },
    SaveSession(Session),
    /// Show a file dialog, then report [`Message::FilePicked`].
    PickFile {
        tab: TabId,
        target: FileTarget,
    },
}

// CONTRACT
/// The main window's state.
#[derive(Debug)]
pub struct App {
    tabs: Vec<Tab>,
    active: usize,
    next_tab: u64,
    last_send: u64,
    sidebar_open: bool,
    sidebar: SidebarSection,
    split: f32,
    /// Tab waiting for `Discard unsaved changes?`.
    confirm_close: Option<TabId>,
    dragging: Option<TabId>,
    session_changed: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::restore(Session::default())
    }
}

impl App {
    // CONTRACT
    /// The workbench for a saved session; one new tab if it has none.
    #[must_use]
    pub fn restore(session: Session) -> Self {
        let (low, high) = Session::SPLIT_RANGE;
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            next_tab: 0,
            last_send: 0,
            sidebar_open: session.sidebar_open,
            sidebar: SidebarSection::default(),
            split: if session.split.is_finite() {
                session.split.clamp(low, high)
            } else {
                Session::DEFAULT_SPLIT
            },
            confirm_close: None,
            dragging: None,
            session_changed: false,
        };
        for draft in session.tabs {
            app.push_tab(draft);
        }
        if app.tabs.is_empty() {
            app.push_tab(RequestDraft::default());
        }
        app.active = session.active.min(app.tabs.len() - 1);
        app
    }

    // CONTRACT
    /// What to save (F07-AC5).
    #[must_use]
    pub fn session(&self) -> Session {
        Session {
            tabs: self.tabs.iter().map(|t| t.draft.clone()).collect(),
            active: self.active,
            sidebar_open: self.sidebar_open,
            split: self.split,
            ..Session::default()
        }
    }

    fn push_tab(&mut self, draft: RequestDraft) -> usize {
        self.next_tab += 1;
        self.tabs.push(Tab::new(TabId(self.next_tab), draft));
        self.tabs.len() - 1
    }

    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    // CONTRACT
    /// The active tab. There is always one.
    #[must_use]
    pub fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    #[must_use]
    pub fn sidebar_open(&self) -> bool {
        self.sidebar_open
    }

    #[must_use]
    pub fn sidebar(&self) -> SidebarSection {
        self.sidebar
    }

    #[must_use]
    pub fn split(&self) -> f32 {
        self.split
    }

    #[must_use]
    pub fn confirm_close(&self) -> Option<TabId> {
        self.confirm_close
    }

    #[must_use]
    pub fn dragging(&self) -> Option<TabId> {
        self.dragging
    }

    fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    // CONTRACT
    /// Apply `message`, returning a side effect to run, if any.
    pub fn update(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::NewTab => {
                self.active = self.push_tab(RequestDraft::default());
                self.session_changed = true;
            }
            Message::CloseTab(id) => self.request_close(id),
            Message::CloseActiveTab => self.request_close(self.tab().id),
            Message::ConfirmClose(discard) => {
                if let Some(id) = self.confirm_close.take()
                    && discard
                {
                    self.close(id);
                }
            }
            Message::ActivateTab(id) => {
                if let Some(i) = self.index_of(id) {
                    self.active = i;
                    self.session_changed = true;
                }
            }
            Message::NextTab => self.cycle(1),
            Message::PreviousTab => self.cycle(self.tabs.len() - 1),
            Message::TabPressed(id) => {
                if let Some(i) = self.index_of(id) {
                    self.active = i;
                    self.dragging = Some(id);
                    self.session_changed = true;
                }
            }
            Message::TabEntered(target) => self.drag_over(target),
            Message::TabReleased => self.dragging = None,
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                self.session_changed = true;
            }
            Message::SidebarSection(section) => {
                self.sidebar = section;
                self.sidebar_open = true;
            }
            Message::SplitDragged(split) => {
                if split.is_finite() {
                    let (low, high) = Session::SPLIT_RANGE;
                    self.split = split.clamp(low, high);
                    self.session_changed = true;
                }
            }
            Message::Autosave => {
                if self.session_changed {
                    self.session_changed = false;
                    return Some(Effect::SaveSession(self.session()));
                }
            }
            Message::Edit(edit) => {
                if self.tab_mut().edit(edit) {
                    self.session_changed = true;
                }
            }
            Message::SendPressed => return self.send(),
            Message::CancelPressed => self.tab_mut().cancel(),
            Message::RequestTabSelected(t) => self.tab_mut().request_tab = t,
            Message::ResponseTabSelected(t) => self.tab_mut().response_tab = t,
            Message::BodyFormatSelected(f) => self.tab_mut().select_format(f),
            Message::Body(m) => self.tab_mut().body_message(m),
            Message::PickFile(target) => {
                return Some(Effect::PickFile {
                    tab: self.tab().id,
                    target,
                });
            }
            Message::FilePicked { tab, target, path } => {
                if let (Some(i), Some(path)) = (self.index_of(tab), path) {
                    self.tabs[i].file_picked(target, path);
                    self.session_changed = true;
                }
            }
            Message::Finished { tab, id, result } => {
                if let Some(i) = self.index_of(tab) {
                    self.tabs[i].finished(id, result);
                }
            }
        }
        None
    }

    fn cycle(&mut self, step: usize) {
        self.active = (self.active + step) % self.tabs.len();
        self.session_changed = true;
    }

    fn drag_over(&mut self, target: TabId) {
        let Some(dragged) = self.dragging else {
            return;
        };
        let (Some(from), Some(to)) = (self.index_of(dragged), self.index_of(target)) else {
            return;
        };
        if from != to {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
            self.active = to;
            self.session_changed = true;
        }
    }

    fn request_close(&mut self, id: TabId) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        if self.tabs[i].is_dirty() {
            self.confirm_close = Some(id);
        } else {
            self.close(id);
        }
    }

    fn close(&mut self, id: TabId) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let mut tab = self.tabs.remove(i);
        tab.cancel();
        if self.tabs.is_empty() {
            self.push_tab(RequestDraft::default());
        }
        if i < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        self.session_changed = true;
    }

    fn send(&mut self) -> Option<Effect> {
        let tab = &mut self.tabs[self.active];
        if tab.is_sending() {
            return None;
        }
        if let Some(error) = &tab.method_error {
            tab.phase = Phase::Failed(error.to_string());
            return None;
        }
        let request = match tab.draft.build() {
            Ok(request) => request,
            Err(error) => {
                tab.phase = Phase::Failed(error.to_string());
                return None;
            }
        };
        self.last_send += 1;
        let cancel = CancellationToken::new();
        tab.phase = Phase::Sending {
            id: self.last_send,
            cancel: cancel.clone(),
        };
        Some(Effect::Send {
            tab: tab.id,
            id: self.last_send,
            request,
            cancel,
        })
    }
}

// CONTRACT
/// Execute a send off the UI thread, including indexing and pretty-printing
/// the response body, and report the result as a message. Other effects
/// return `None`; the UI runs them.
pub async fn run(effect: Effect, client: Client) -> Option<Message> {
    let Effect::Send {
        tab,
        id,
        request,
        cancel,
    } = effect
    else {
        return None;
    };
    let result = match client.send(&request, &cancel).await {
        Outcome::Completed(response) => match received(response) {
            Ok(received) => Finished::Received(Box::new(received)),
            Err(error) => Finished::Failed(error),
        },
        Outcome::Failed(error) => Finished::Failed(error.to_string()),
        Outcome::Cancelled => Finished::Cancelled,
    };
    Some(Message::Finished {
        tab,
        id,
        result: Arc::new(result),
    })
}

// CONTRACT
/// Build a response's body views and pick the default format: Pretty for
/// JSON, Hex for binary, Raw otherwise (F06-AC6, F06-AC7).
///
/// # Errors
/// The viewer's error text if the body is too large to index.
pub fn received(response: ferry_core::response::Response) -> Result<Received, String> {
    let body = response.body.clone();
    let raw = BodyView::new(body.clone(), WRAP_BYTES).map_err(|e| e.to_string())?;
    let pretty = if is_json(&response) {
        json::pretty(&body).and_then(|bytes| BodyView::new(bytes.into(), WRAP_BYTES).ok())
    } else {
        None
    };
    let format = if pretty.is_some() {
        BodyFormat::Pretty
    } else if looks_binary(&body) {
        BodyFormat::Hex
    } else {
        BodyFormat::Raw
    };
    Ok(Received {
        hex: BodyView::hex(body),
        response,
        raw,
        pretty,
        format,
    })
}

/// JSON by `Content-Type` (`application/json`, `+json`), or by content.
fn is_json(response: &ferry_core::response::Response) -> bool {
    let declared = response.headers.iter().any(|h| {
        h.name.eq_ignore_ascii_case("content-type") && h.value.to_ascii_lowercase().contains("json")
    });
    declared
        || response
            .body
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_some_and(|b| *b == b'{' || *b == b'[')
}
