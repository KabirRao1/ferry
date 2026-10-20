//! The workbench: open tabs, layout, messages, `update()` and effects
//! (F05, F06, F07, F08, F09, F14).

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ferry_core::auth::{AuthDraft, AuthKind};
use ferry_core::collection::{self, AuthSource};
use ferry_core::cookies::Jar;
use ferry_core::draft::RequestDraft;
use ferry_core::history::{Entry, History};
use ferry_core::json;
use ferry_core::request::Request;
use ferry_core::scripts::Phase as ScriptPhase;
use ferry_core::session::{Session, SessionTab};
use ferry_core::text::looks_binary;
use ferry_core::variables::{Resolver, ScopeKind, Vars};
use ferry_core::workspace::{Op, Workspace};
use ferry_http::{CancellationToken, Client, Cookies, Outcome};

use crate::activity::Pending;
use crate::ambient::{Ambient, SystemAmbient};
use crate::body_view::{self, BodyView};
use crate::console::Console;
use crate::doc::{Doc, EnvTarget};
use crate::scripting::{ScriptReport, ScriptedSend};
use crate::tab::{
    BodyFormat, Edit, FileTarget, Phase, Received, RequestTab, ResponseTab, Tab, TabId, TestFilter,
};
use crate::workspace::{Dialog, Target};

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
    /// Show the pre-request or post-response script (F15-AC1).
    ScriptPhaseSelected(ScriptPhase),
    /// Narrow the Test Results list (F15-AC10).
    TestFilterSelected(TestFilter),
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
    /// A send with scripts ended (F15).
    ScriptedFinished {
        tab: TabId,
        id: u64,
        result: Arc<Finished>,
        report: Arc<ScriptReport>,
    },
    Finished {
        tab: TabId,
        id: u64,
        result: Arc<Finished>,
    },

    // --- workspace (F08, F09) ---
    /// The workspace was (re)loaded, at startup or after an outside change.
    WorkspaceLoaded(Arc<Workspace>),
    /// [`Effect::Workspace`] ran; `error` is the reason it failed.
    WorkspaceApplied {
        action: String,
        error: Option<String>,
        workspace: Arc<Workspace>,
    },
    /// `Open Workspace…` (F09-AC14).
    OpenWorkspace,
    /// The folder dialog closed; `None` if cancelled.
    WorkspacePicked(Option<PathBuf>),
    /// Save button or Ctrl/Cmd+S on the active tab (F08-AC8, F09-AC5).
    Save,
    /// Typed into the header search (F09-AC13).
    Search(String),
    /// Expand or collapse a collection or folder.
    ToggleExpanded(String),
    /// Open a request, or a collection's or folder's settings (F09-AC4,
    /// F09-AC11).
    OpenItem(String),
    /// Open or close the `…` menu of a sidebar entry.
    Menu(Option<String>),
    NewCollection,
    /// A new folder in this collection or folder.
    NewFolder(String),
    /// A new request in this collection or folder.
    NewRequest(String),
    Rename(Target),
    Duplicate(Target),
    Delete(Target),
    /// Drag and drop in the tree (F09-AC9): pressing an item, the pointer
    /// entering another, and releasing. Releasing on the pressed item
    /// opens it.
    ItemPressed(String),
    ItemEntered(String),
    ItemReleased,
    /// Open an example of the request at this path (F32-AC3).
    OpenExample(String, usize),
    /// The environment selector (F08-AC9).
    SelectEnvironment(Option<String>),
    NewEnvironment,
    OpenEnvironment(EnvTarget),
    /// Text typed into the open dialog's name field.
    DialogInput(String),
    /// The collection or folder picked in the Save dialog.
    DialogPick(String),
    /// The name of the collection the Save dialog creates when there is none.
    DialogCollectionName(String),
    DialogConfirm,
    DialogCancel,
    /// Hide the footer's error.
    DismissStatus,

    // --- history (F10) ---
    HistorySearch(String),
    /// Open an entry in a new tab (F10-AC4).
    OpenHistory(u64),
    DeleteHistory(u64),
    /// Asks `Clear all history?` first (F10-AC5).
    ClearHistory,

    // --- cookies (F17) ---
    /// Open the cookie manager (F17-AC5).
    OpenCookies,
    CookieDomain(String),
    /// Text of the manager's new-domain field.
    CookieNewDomain(String),
    CookieAddDomain,
    /// Edit a cookie of the selected domain by name and path, or start a
    /// new one with `None`.
    CookieEdit(Option<(String, String)>),
    CookieText(String),
    CookieSave,
    /// Delete a cookie of the selected domain by name and path.
    CookieDelete(String, String),
    CookieDeleteDomain(String),

    // --- console (F18) ---
    ToggleConsole,
    ConsoleFilter(String),
    ConsoleErrorsOnly(bool),
    ConsoleClear,
    /// Expand or collapse an entry.
    ConsoleExpand(u64),

    // --- examples (F32) ---
    /// Save the active tab's response as an example of its request.
    SaveAsExample,

    // --- search and filter (F06-AC8, F22-AC4) ---
    /// Ctrl/Cmd+F in the response pane, or its search button.
    FindOpen,
    /// Escape in the find bar.
    FindClose,
    FindQuery(String),
    /// Enter or Down.
    FindNext,
    /// Shift+Enter or Up.
    FindPrevious,
    /// [`Effect::Find`] ran.
    Found {
        tab: TabId,
        job: u64,
        matches: Arc<Vec<usize>>,
        capped: bool,
    },
    /// Typed into the Pretty view's filter field.
    FilterChanged(String),
    /// [`Effect::Filter`] ran: the results, or the error text.
    Filtered {
        tab: TabId,
        job: u64,
        result: Arc<Result<BodyView, String>>,
    },

    // --- preview (F22) ---
    /// The web view could not be created; Preview falls back to Open in
    /// Browser from now on (F22-AC3).
    PreviewFailed(String),
    /// Open the active tab's response body with the system's application.
    OpenInBrowser,
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
        /// A snapshot of the jar (F17).
        cookies: Cookies,
    },
    /// Run a request's scripts around its send with
    /// [`crate::execute_scripted`] and report [`Message::ScriptedFinished`]
    /// (F15-AC2).
    ScriptedSend {
        tab: TabId,
        id: u64,
        send: Box<ScriptedSend>,
        cancel: CancellationToken,
        cookies: Cookies,
    },
    SaveSession(Session),
    /// Show a file dialog, then report [`Message::FilePicked`].
    PickFile {
        tab: TabId,
        target: FileTarget,
    },
    /// Apply `ops` to the workspace folder, reload it, and report
    /// [`Message::WorkspaceApplied`]. `action` completes `Could not …`.
    Workspace {
        action: String,
        ops: Vec<Op>,
    },
    /// Show a folder dialog, then report [`Message::WorkspacePicked`].
    PickWorkspace,
    /// Switch to the workspace in this folder and report
    /// [`Message::WorkspaceLoaded`] (F09-AC14).
    SwitchWorkspace(PathBuf),
    /// Save the history, write a new entry's body, and delete the bodies
    /// of removed entries (F10-AC6).
    History {
        history: History,
        body: Option<(u64, bytes::Bytes)>,
        remove: Vec<u64>,
    },
    /// Read an entry's body and report [`Message::Finished`] for `tab` and
    /// `id`, built with [`crate::history_response`] (F10-AC4).
    LoadHistory {
        tab: TabId,
        id: u64,
        entry: Box<Entry>,
    },
    SaveCookies(Jar),
    /// Run every effect.
    Batch(Vec<Effect>),
    /// Search `body` for `query` with [`crate::run_find`] and report [`Message::Found`]
    /// (F06-AC8).
    Find {
        tab: TabId,
        job: u64,
        body: bytes::Bytes,
        query: String,
    },
    /// Apply the `JSONPath` `expression` to `body` with [`crate::run_find`] and report
    /// [`Message::Filtered`] (F22-AC4).
    Filter {
        tab: TabId,
        job: u64,
        body: bytes::Bytes,
        expression: String,
    },
    /// Write `body` to a temporary file named with `extension` and open it
    /// with the system's default application (F22-AC3).
    OpenInBrowser {
        body: bytes::Bytes,
        extension: &'static str,
    },
}

// CONTRACT
/// The main window's state.
#[derive(Debug)]
pub struct App {
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    next_tab: u64,
    pub(crate) last_send: u64,
    sidebar_open: bool,
    sidebar: SidebarSection,
    split: f32,
    /// Tab waiting for `Discard unsaved changes?`.
    confirm_close: Option<TabId>,
    dragging: Option<TabId>,
    pub(crate) session_changed: bool,
    pub(crate) workspace: Arc<Workspace>,
    /// The folder chosen with `Open Workspace…`, if any; saved.
    pub(crate) workspace_dir: Option<PathBuf>,
    /// The folder in use, for display; not saved.
    pub(crate) workspace_root: Option<PathBuf>,
    /// Path of the selected environment.
    pub(crate) environment: Option<String>,
    pub(crate) expanded: BTreeSet<String>,
    pub(crate) search: String,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) menu: Option<String>,
    /// Tree drag and drop: the pressed item and the one under the pointer.
    pub(crate) tree_drag: Option<String>,
    pub(crate) tree_target: Option<String>,
    /// The last workspace error, shown in the footer.
    pub(crate) status: Option<String>,
    pub(crate) ambient: Box<dyn Ambient>,
    pub(crate) history: History,
    pub(crate) history_search: String,
    pub(crate) cookies: Jar,
    pub(crate) cookies_changed: bool,
    pub(crate) console: Console,
    /// Sends in flight, by send id, for history and the console.
    pub(crate) pending: std::collections::HashMap<u64, Pending>,
    /// The last search or filter job id.
    pub(crate) next_job: u64,
    /// An embedded web view can show previews (F22-AC3).
    pub(crate) preview_available: bool,
    pub(crate) preview_error: Option<String>,
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
            workspace: Arc::new(Workspace::default()),
            workspace_root: session.workspace.clone(),
            workspace_dir: session.workspace,
            environment: session.environment,
            expanded: BTreeSet::new(),
            search: String::new(),
            dialog: None,
            menu: None,
            tree_drag: None,
            tree_target: None,
            status: None,
            ambient: Box::new(SystemAmbient::default()),
            history: History::default(),
            history_search: String::new(),
            cookies: Jar::default(),
            cookies_changed: false,
            console: Console::default(),
            pending: std::collections::HashMap::new(),
            next_job: 0,
            preview_available: true,
            preview_error: None,
        };
        for tab in session.tabs {
            let i = app.push_tab(tab.draft);
            app.tabs[i].link = tab.link;
        }
        if app.tabs.is_empty() {
            app.push_tab(RequestDraft::default());
        }
        app.active = session.active.min(app.tabs.len() - 1);
        app
    }

    // CONTRACT
    /// Load `workspace` into a freshly restored app: linked tabs get their
    /// saved state, and a selected environment that is gone is cleared.
    #[must_use]
    pub fn with_workspace(mut self, workspace: Workspace) -> Self {
        self.workspace_loaded(Arc::new(workspace));
        self.session_changed = false;
        self
    }

    // CONTRACT
    /// The workspace folder in use, shown in the header.
    #[must_use]
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_root = Some(root);
        self
    }

    // CONTRACT
    /// Start with this history (F10-AC6).
    #[must_use]
    pub fn with_history(mut self, history: History) -> Self {
        self.history = history;
        self
    }

    // CONTRACT
    /// Start with this cookie jar (F17-AC6).
    #[must_use]
    pub fn with_cookies(mut self, jar: Jar) -> Self {
        self.cookies = jar;
        self
    }

    // CONTRACT
    /// Use `ambient` for the clock, randomness and time zone.
    #[must_use]
    pub fn with_ambient(mut self, ambient: Box<dyn Ambient>) -> Self {
        self.ambient = ambient;
        self
    }

    // CONTRACT
    /// What to save (F07-AC5, F07-AC7): request tabs with their links, the
    /// environment and the workspace folder. The active index counts
    /// request tabs only.
    #[must_use]
    pub fn session(&self) -> Session {
        let requests: Vec<&Tab> = self.tabs.iter().filter(|t| t.doc == Doc::Request).collect();
        let active = requests
            .iter()
            .position(|t| t.id == self.tab().id)
            .unwrap_or(0);
        Session {
            tabs: requests
                .iter()
                .map(|t| SessionTab {
                    link: t.link.clone(),
                    draft: t.draft.clone(),
                })
                .collect(),
            active,
            sidebar_open: self.sidebar_open,
            split: self.split,
            environment: self.environment.clone(),
            workspace: self.workspace_dir.clone(),
            ..Session::default()
        }
    }

    fn push_tab(&mut self, draft: RequestDraft) -> usize {
        self.next_tab += 1;
        self.tabs.push(Tab::new(TabId(self.next_tab), draft));
        self.tabs.len() - 1
    }

    /// Open a tab holding `doc` and make it active.
    pub(crate) fn push_doc(&mut self, doc: Doc) {
        let i = self.push_tab(RequestDraft::default());
        self.tabs[i].doc = doc;
        self.active = i;
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

    #[must_use]
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// The folder chosen with `Open Workspace…`, if any.
    #[must_use]
    pub fn workspace_dir(&self) -> Option<&Path> {
        self.workspace_dir.as_deref()
    }

    /// The workspace folder in use, if known.
    #[must_use]
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Path of the selected environment.
    #[must_use]
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }

    #[must_use]
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded.contains(path)
    }

    #[must_use]
    pub fn search(&self) -> &str {
        &self.search
    }

    #[must_use]
    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    /// The sidebar entry whose `…` menu is open.
    #[must_use]
    pub fn menu(&self) -> Option<&str> {
        self.menu.as_deref()
    }

    /// The tree item being dragged and the one under the pointer.
    #[must_use]
    pub fn tree_drag(&self) -> (Option<&str>, Option<&str>) {
        (self.tree_drag.as_deref(), self.tree_target.as_deref())
    }

    /// The last workspace error (F09-AC15).
    #[must_use]
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    // CONTRACT
    /// What Inherit means for a request tab: the nearest parent's auth and
    /// where it comes from, or `None` for an unlinked tab (F04-AC6).
    #[must_use]
    pub fn inherited_auth(&self, tab: &Tab) -> Option<(AuthDraft, AuthSource)> {
        let link = tab.link.as_deref()?;
        collection::inherited_auth(&self.workspace.collections, link)
    }

    pub(crate) fn index_of(&self, id: TabId) -> Option<usize> {
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
            Message::Autosave => return self.autosave(),
            Message::Edit(edit) => {
                if self.tab_mut().edit(edit) {
                    self.session_changed = true;
                }
            }
            Message::SendPressed => return self.send(),
            Message::CancelPressed => self.tab_mut().cancel(),
            Message::RequestTabSelected(t) => self.tab_mut().request_tab = t,
            Message::ResponseTabSelected(t) => self.tab_mut().response_tab = t,
            Message::ScriptPhaseSelected(p) => self.tab_mut().script_phase = p,
            Message::TestFilterSelected(f) => self.tab_mut().test_filter = f,
            Message::BodyFormatSelected(f) => {
                self.tab_mut().select_format(f);
                // The shown text changed.
                return self.search_active();
            }
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
            Message::ScriptedFinished {
                tab,
                id,
                result,
                report,
            } => {
                let saves = self.scripts_finished(id, &report);
                let effect = self.send_finished(id, &result);
                if let Some(i) = self.index_of(tab) {
                    let now = self.now_ms();
                    self.tabs[i].finished(id, result);
                    self.tabs[i].received_cookies(now);
                    self.tabs[i].scripts_ran(id, &report);
                }
                return match (saves, effect) {
                    (Some(a), Some(b)) => Some(Effect::Batch(vec![a, b])),
                    (a, b) => a.or(b),
                };
            }
            Message::Finished { tab, id, result } => {
                let effect = self.send_finished(id, &result);
                if let Some(i) = self.index_of(tab) {
                    let now = self.now_ms();
                    self.tabs[i].finished(id, result);
                    self.tabs[i].received_cookies(now);
                }
                return effect;
            }
            Message::HistorySearch(_)
            | Message::OpenHistory(_)
            | Message::DeleteHistory(_)
            | Message::ClearHistory
            | Message::OpenCookies
            | Message::CookieDomain(_)
            | Message::CookieNewDomain(_)
            | Message::CookieAddDomain
            | Message::CookieEdit(_)
            | Message::CookieText(_)
            | Message::CookieSave
            | Message::CookieDelete(..)
            | Message::CookieDeleteDomain(_)
            | Message::ToggleConsole
            | Message::ConsoleFilter(_)
            | Message::ConsoleErrorsOnly(_)
            | Message::ConsoleClear
            | Message::ConsoleExpand(_)
            | Message::SaveAsExample => return self.activity_message(message),
            Message::FindOpen
            | Message::FindClose
            | Message::FindQuery(_)
            | Message::FindNext
            | Message::FindPrevious
            | Message::Found { .. }
            | Message::FilterChanged(_)
            | Message::Filtered { .. } => return self.find_message(message),
            Message::PreviewFailed(_) | Message::OpenInBrowser => {
                return self.preview_message(message);
            }
            other => return self.workspace_message(other),
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

    /// The request tab `index` sends: inherited auth filled in, variables
    /// resolved (F04-AC6, F08-AC3), then built.
    fn prepare(&mut self, index: usize) -> Result<Request, String> {
        let tab = &self.tabs[index];
        let mut draft = tab.draft.clone();
        if draft.auth.kind == AuthKind::Inherit {
            draft.auth = self
                .inherited_auth(tab)
                .map(|(auth, _)| auth)
                .unwrap_or_default();
        }
        let workspace = Arc::clone(&self.workspace);
        let resolver_vars = vars_for(&workspace, self.environment.as_deref(), tab.link.as_deref());
        let mut resolver = Resolver::new(&resolver_vars, self.ambient.as_mut());
        let resolved = draft.resolve(&mut resolver);
        resolver.finish().map_err(|e| e.to_string())?;
        resolved.build().map_err(|e| e.to_string())
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
        if let Some(send) = self.scripted(self.active) {
            return Some(self.send_scripted(send));
        }
        let request = match self.prepare(self.active) {
            Ok(request) => request,
            Err(error) => {
                self.tabs[self.active].phase = Phase::Failed(error);
                return None;
            }
        };
        self.last_send += 1;
        let id = self.last_send;
        self.started(id, &request);
        let cookies = Cookies {
            jar: self.cookies.clone(),
            now: self.now_ms(),
        };
        let tab = &mut self.tabs[self.active];
        let cancel = CancellationToken::new();
        tab.phase = Phase::Sending {
            id,
            cancel: cancel.clone(),
        };
        Some(Effect::Send {
            tab: tab.id,
            id,
            request,
            cancel,
            cookies,
        })
    }
}

/// The variables in effect for a request linked to `link`, with the
/// environment at `environment` selected (F08-AC2).
pub(crate) fn vars_for<'a>(
    workspace: &'a Workspace,
    environment: Option<&str>,
    link: Option<&str>,
) -> Vars<'a> {
    let mut vars = Vars::new();
    if let Some(env) = environment
        .and_then(|p| workspace.environment(p))
        .and_then(|e| e.file.as_ref().ok())
    {
        vars = vars.with(ScopeKind::Environment, &env.variables);
    }
    if let Some(link) = link {
        vars = vars.with(
            ScopeKind::Collection,
            collection::collection_variables(&workspace.collections, link),
        );
    }
    if let Ok(globals) = &workspace.globals {
        vars = vars.with(ScopeKind::Global, &globals.variables);
    }
    vars
}

// CONTRACT
/// Execute a send off the UI thread, including indexing and pretty-printing
/// the response body, and report the result as a message. Other effects
/// return `None`; the UI runs them.
pub async fn run(effect: Effect, client: Client) -> Option<Message> {
    if let Effect::ScriptedSend {
        tab,
        id,
        send,
        cancel,
        cookies,
    } = effect
    {
        let (result, report) = crate::scripting::execute(*send, cancel, cookies, client).await;
        return Some(Message::ScriptedFinished {
            tab,
            id,
            result: Arc::new(result),
            report: Arc::new(report),
        });
    }
    let Effect::Send {
        tab,
        id,
        request,
        cancel,
        cookies,
    } = effect
    else {
        return None;
    };
    let result = match client.send_with(&request, Some(cookies), &cancel).await {
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
        cookies: Vec::new(),
        note: None,
        find: None,
        filter: crate::find::Filter::default(),
        send: 0,
        tests: Vec::new(),
        script_error: None,
        visualizer: None,
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
