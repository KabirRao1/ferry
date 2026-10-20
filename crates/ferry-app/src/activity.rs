//! What sends leave behind: history (F10), the cookie jar (F17), the
//! console (F18), and saving a response as an example (F32-AC1).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use ferry_core::collection::{self, Example};
use ferry_core::cookies::{CookieError, Jar};
use ferry_core::draft::RequestDraft;
use ferry_core::history::{self, Entry, MAX_BODY, Outcome};
use ferry_core::request::Request;
use ferry_core::response::Response;
use ferry_core::workspace::Op;

use crate::app::{App, Effect, Finished, Message, received};
use crate::console::{ConsoleState, describe_body, preview};
use crate::tab::{Phase, Tab};
use crate::workspace::{Dialog, unique_example_name};

/// Template for a new cookie in the manager.
pub const NEW_COOKIE: &str = "name=value; Path=/";

/// Shown above a history body that was cut to 1 MB (F10-AC4).
pub const TRUNCATED_NOTE: &str = "Body truncated to 1 MB in history";

// CONTRACT
/// A send in flight, remembered for history and the console.
#[derive(Debug, Clone)]
pub struct Pending {
    pub draft: RequestDraft,
    pub method: String,
    pub url: String,
    /// The console entry logging it.
    pub console: u64,
}

// CONTRACT
/// The cookie manager's state (F17-AC5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CookieManager {
    /// The domain whose cookies are listed.
    pub selected: Option<String>,
    /// Text of the new-domain field.
    pub new_domain: String,
    /// Domains added in the manager that have no cookie yet.
    pub extra_domains: Vec<String>,
    pub editor: Option<CookieEditor>,
}

// CONTRACT
/// A cookie being added or edited as `Set-Cookie` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieEditor {
    /// The edited cookie's name and path; `None` for a new one.
    pub original: Option<(String, String)>,
    pub text: String,
    pub error: Option<String>,
}

// CONTRACT
/// The tab contents for a history entry: its response with the stored body,
/// or its error (F10-AC4).
#[must_use]
pub fn history_response(entry: &Entry, body: Option<Vec<u8>>) -> Finished {
    let (status, reason, elapsed_ms, headers, truncated) = match &entry.outcome {
        Outcome::Failed { error } => return Finished::Failed(error.clone()),
        Outcome::Response {
            status,
            reason,
            elapsed_ms,
            headers,
            truncated,
            ..
        } => (*status, reason, *elapsed_ms, headers, *truncated),
    };
    let response = Response {
        status,
        reason: reason.clone(),
        headers: headers.clone(),
        body: Bytes::from(body.unwrap_or_default()),
        elapsed: Duration::from_millis(elapsed_ms),
        url: entry.url.clone(),
        ..Response::default()
    };
    match received(response) {
        Ok(mut r) => {
            if truncated {
                r.note = Some(TRUNCATED_NOTE.to_owned());
            }
            Finished::Received(Box::new(r))
        }
        Err(error) => Finished::Failed(error),
    }
}

/// The host and path a cookie URL refers to.
fn host_and_path(url: &str) -> Option<(String, String)> {
    let url = url::Url::parse(url).ok()?;
    Some((url.host_str()?.to_owned(), url.path().to_owned()))
}

impl App {
    /// Milliseconds since the epoch on the injected clock.
    pub(crate) fn now_ms(&self) -> i64 {
        ferry_core::time::unix_millis(self.ambient.now())
    }

    /// Log a send whose scripts run first: the draft's method and URL
    /// until the request is built (F18-AC1, F15-AC2).
    pub(crate) fn started_scripted(&mut self, id: u64, draft: &RequestDraft) {
        let method = draft.method.to_string();
        let url = draft.url.clone();
        let console = self
            .console
            .push(method.clone(), url.clone(), Vec::new(), String::new());
        let pending = Pending {
            draft: self.tab().draft.clone(),
            method,
            url,
            console,
        };
        self.pending.insert(id, pending);
    }

    /// Log a send that is starting (F18-AC1).
    pub(crate) fn started(&mut self, id: u64, request: &Request) {
        let draft = self.tab().draft.clone();
        let method = request.method.to_string();
        let url = request.url.to_string();
        let console = self.console.push(
            method.clone(),
            url.clone(),
            request.headers.clone(),
            describe_body(&request.body),
        );
        let pending = Pending {
            draft,
            method,
            url,
            console,
        };
        self.pending.insert(id, pending);
    }

    /// Record a send that ended: history, cookies, console.
    pub(crate) fn send_finished(&mut self, id: u64, result: &Arc<Finished>) -> Option<Effect> {
        let pending = self.pending.remove(&id)?;
        let now = self.now_ms();
        let outcome = match result.as_ref() {
            Finished::Cancelled => {
                if let Some(entry) = self.console.entry_mut(pending.console) {
                    entry.state = ConsoleState::Cancelled;
                }
                return None;
            }
            Finished::Failed(error) => {
                if let Some(entry) = self.console.entry_mut(pending.console) {
                    entry.state = ConsoleState::Failed(error.clone());
                }
                (
                    Outcome::Failed {
                        error: error.clone(),
                    },
                    None,
                )
            }
            Finished::Received(r) => {
                let response = &r.response;
                for cookie in &response.set_cookies {
                    if let Some((host, path)) = host_and_path(&cookie.url) {
                        // Ignored cookies (F17-AC1) are simply not stored.
                        let _ = self.cookies.store(&cookie.header, &host, &path, now);
                        self.cookies_changed = true;
                    }
                }
                let elapsed_ms = u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX);
                if let Some(entry) = self.console.entry_mut(pending.console) {
                    if !response.sent.is_empty() {
                        entry.request_headers.clone_from(&response.sent);
                    }
                    entry.state = ConsoleState::Done {
                        status: response.status,
                        reason: response.reason.clone(),
                        elapsed: response.elapsed,
                        size: response.body.len() as u64,
                        headers: response.headers.clone(),
                        body: preview(&response.body),
                    };
                }
                let kept = response.body.slice(..response.body.len().min(MAX_BODY));
                (
                    Outcome::Response {
                        status: response.status,
                        reason: response.reason.clone(),
                        elapsed_ms,
                        size: response.body.len() as u64,
                        headers: response.headers.clone(),
                        truncated: kept.len() < response.body.len(),
                    },
                    Some(kept),
                )
            }
        };
        let (outcome, body) = outcome;
        let (entry_id, remove) = self.history.record(Entry {
            id: 0,
            at: now,
            method: pending.method,
            url: pending.url,
            request: pending.draft,
            outcome,
        });
        Some(Effect::History {
            history: self.history.clone(),
            body: body.map(|b| (entry_id, b)),
            remove,
        })
    }

    /// Save what changed: the session, the jar.
    pub(crate) fn autosave(&mut self) -> Option<Effect> {
        let mut effects = Vec::new();
        if self.session_changed {
            self.session_changed = false;
            effects.push(Effect::SaveSession(self.session()));
        }
        if self.cookies_changed {
            self.cookies_changed = false;
            effects.push(Effect::SaveCookies(self.cookies.clone()));
        }
        match effects.len() {
            0 => None,
            1 => effects.pop(),
            _ => Some(Effect::Batch(effects)),
        }
    }

    /// Confirm `Clear all history?`, or save the cookie being edited.
    pub(crate) fn confirm_activity_dialog(&mut self) -> Option<Effect> {
        match self.dialog.take()? {
            Dialog::ClearHistory => {
                let remove = self.history.clear();
                Some(Effect::History {
                    history: self.history.clone(),
                    body: None,
                    remove,
                })
            }
            Dialog::Cookies(manager) => {
                self.dialog = Some(Dialog::Cookies(manager));
                self.save_cookie();
                None
            }
            other => {
                self.dialog = Some(other);
                None
            }
        }
    }

    fn manager(&mut self) -> Option<&mut CookieManager> {
        match &mut self.dialog {
            Some(Dialog::Cookies(manager)) => Some(manager),
            _ => None,
        }
    }

    /// Store the cookie being edited, replacing the one it was opened from.
    fn save_cookie(&mut self) {
        let now = self.now_ms();
        let Some(manager) = self.manager() else {
            return;
        };
        let (Some(domain), Some(editor)) = (manager.selected.clone(), manager.editor.clone())
        else {
            return;
        };
        let mut jar = self.cookies.clone();
        if let Some((name, path)) = &editor.original {
            jar.remove(&domain, name, path);
        }
        match jar.store(&editor.text, &domain, "/", now) {
            Ok(()) => {
                self.cookies = jar;
                self.cookies_changed = true;
                if let Some(manager) = self.manager() {
                    manager.editor = None;
                    manager.extra_domains.retain(|d| *d != domain);
                }
            }
            Err(error) => {
                let message = match error {
                    CookieError::Syntax | CookieError::Domain { .. } => error.to_string(),
                };
                if let Some(Dialog::Cookies(manager)) = &mut self.dialog
                    && let Some(editor) = &mut manager.editor
                {
                    editor.error = Some(message);
                }
            }
        }
    }

    pub(crate) fn activity_message(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::HistorySearch(text) => self.history_search = text,
            Message::OpenHistory(id) => return self.open_history(id),
            Message::DeleteHistory(id) => {
                if self.history.remove(id) {
                    return Some(Effect::History {
                        history: self.history.clone(),
                        body: None,
                        remove: vec![id],
                    });
                }
            }
            Message::ClearHistory => self.dialog = Some(Dialog::ClearHistory),
            Message::OpenCookies => {
                self.dialog = Some(Dialog::Cookies(CookieManager {
                    selected: self.cookies.domains().into_iter().next(),
                    ..CookieManager::default()
                }));
            }
            Message::CookieDomain(domain) => {
                if let Some(manager) = self.manager() {
                    manager.selected = Some(domain);
                    manager.editor = None;
                }
            }
            Message::CookieNewDomain(text) => {
                if let Some(manager) = self.manager() {
                    manager.new_domain = text;
                }
            }
            Message::CookieAddDomain => {
                if let Some(manager) = self.manager() {
                    let domain = manager
                        .new_domain
                        .trim()
                        .trim_start_matches('.')
                        .to_ascii_lowercase();
                    manager.new_domain.clear();
                    if !domain.is_empty() {
                        if !manager.extra_domains.contains(&domain) {
                            manager.extra_domains.push(domain.clone());
                        }
                        manager.selected = Some(domain);
                        manager.editor = None;
                    }
                }
            }
            Message::CookieEdit(original) => {
                let text = original
                    .as_ref()
                    .and_then(|(name, path)| {
                        let domain = self.manager()?.selected.clone()?;
                        self.cookies
                            .for_domain(&domain)
                            .into_iter()
                            .find(|c| c.name == *name && c.path == *path)
                            .map(ferry_core::cookies::Cookie::to_set_cookie)
                    })
                    .unwrap_or_else(|| NEW_COOKIE.to_owned());
                if let Some(manager) = self.manager() {
                    manager.editor = Some(CookieEditor {
                        original,
                        text,
                        error: None,
                    });
                }
            }
            Message::CookieText(text) => {
                if let Some(editor) = self.manager().and_then(|m| m.editor.as_mut()) {
                    editor.text = text;
                    editor.error = None;
                }
            }
            Message::CookieSave => self.save_cookie(),
            Message::CookieDelete(name, path) => {
                let domain = self.manager().and_then(|m| m.selected.clone());
                if let Some(domain) = domain
                    && self.cookies.remove(&domain, &name, &path)
                {
                    self.cookies_changed = true;
                }
            }
            Message::CookieDeleteDomain(domain) => {
                self.cookies.remove_domain(&domain);
                self.cookies_changed = true;
                if let Some(manager) = self.manager() {
                    manager.extra_domains.retain(|d| *d != domain);
                    if manager.selected.as_deref() == Some(domain.as_str()) {
                        manager.selected = None;
                        manager.editor = None;
                    }
                }
            }
            Message::ToggleConsole => self.console.open = !self.console.open,
            Message::ConsoleFilter(text) => self.console.filter = text,
            Message::ConsoleErrorsOnly(on) => self.console.errors_only = on,
            Message::ConsoleClear => {
                self.console.entries.clear();
                self.console.expanded.clear();
            }
            Message::ConsoleExpand(id) => {
                if !self.console.expanded.remove(&id) {
                    self.console.expanded.insert(id);
                }
            }
            Message::SaveAsExample => return self.save_as_example(),
            _ => {}
        }
        None
    }

    fn open_history(&mut self, id: u64) -> Option<Effect> {
        let entry = self.history.get(id)?.clone();
        self.push_doc(crate::doc::Doc::Request);
        self.last_send += 1;
        let send = self.last_send;
        let tab = &mut self.tabs[self.active];
        tab.set_draft(entry.request.clone());
        tab.phase = Phase::Sending {
            id: send,
            cancel: ferry_http::CancellationToken::new(),
        };
        self.session_changed = true;
        Some(Effect::LoadHistory {
            tab: tab.id,
            id: send,
            entry: Box::new(entry),
        })
    }

    fn save_as_example(&mut self) -> Option<Effect> {
        let tab: &Tab = self.tab();
        let link = tab.link.clone()?;
        let Phase::Received(r) = &tab.phase else {
            return None;
        };
        let mut file = self.workspace.request(&link)?.clone();
        let name = unique_example_name(&file, &file.name);
        file.examples.push(Example {
            name,
            request: tab.draft.clone(),
            status: r.response.status,
            headers: r.response.headers.clone(),
            body: r.response.body.clone(),
        });
        self.expanded.insert(link.clone());
        Some(Effect::Workspace {
            action: "save the example".to_owned(),
            ops: vec![Op::Write {
                path: link,
                contents: collection::write_request(&file),
            }],
        })
    }

    // CONTRACT
    #[must_use]
    pub fn history(&self) -> &ferry_core::history::History {
        &self.history
    }

    #[must_use]
    pub fn history_search(&self) -> &str {
        &self.history_search
    }

    // CONTRACT
    /// The entries the history search keeps, grouped by local day (F10-AC3).
    #[must_use]
    pub fn history_groups(&self) -> Vec<(String, Vec<&Entry>)> {
        let entries = self.history.search(&self.history_search);
        history::groups(&entries, self.now_ms(), self.ambient.utc_offset())
    }

    #[must_use]
    pub fn cookies(&self) -> &Jar {
        &self.cookies
    }

    #[must_use]
    pub fn console(&self) -> &crate::console::Console {
        &self.console
    }

    // CONTRACT
    /// The domains the cookie manager lists: the jar's and the ones added
    /// in the manager, sorted.
    #[must_use]
    pub fn cookie_domains(&self) -> Vec<String> {
        let mut domains = self.cookies.domains();
        if let Some(Dialog::Cookies(manager)) = &self.dialog {
            domains.extend(manager.extra_domains.iter().cloned());
        }
        domains.sort();
        domains.dedup();
        domains
    }
}
