//! One workbench tab: its request draft, editing state and response
//! (F01..F07).

use std::path::PathBuf;
use std::sync::Arc;

use ferry_core::auth::{ApiKeyLocation, AuthKind};
use ferry_core::body::{BodyMode, FormField, RawLanguage};
use ferry_core::cookies::Cookie;
use ferry_core::draft::RequestDraft;
use ferry_core::json::{self, JsonError};
use ferry_core::kv::{self, KeyValue};
use ferry_core::params::{self, Param};
use ferry_core::request::{Method, MethodError};
use ferry_core::response::Response;
use ferry_http::CancellationToken;

use crate::body_view::{self, BodyView};
use crate::doc::{self, ContainerSection, Doc, VarEdit};
use crate::find::{Filter, FilterState, Find};

// CONTRACT
/// Identifies a tab for its whole life, across reorders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

// CONTRACT
/// Where a tab's request is in its life cycle.
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
/// How the response body is shown (F06-AC6, F06-AC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyFormat {
    #[default]
    Pretty,
    Raw,
    Hex,
    /// Rendered by a web view (F22-AC1).
    Preview,
}

impl BodyFormat {
    pub const ALL: [BodyFormat; 4] = [
        BodyFormat::Pretty,
        BodyFormat::Raw,
        BodyFormat::Hex,
        BodyFormat::Preview,
    ];
}

impl std::fmt::Display for BodyFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Pretty => "Pretty",
            Self::Raw => "Raw",
            Self::Hex => "Hex",
            Self::Preview => "Preview",
        })
    }
}

// CONTRACT
/// A completed response plus its body views.
#[derive(Debug, Clone)]
pub struct Received {
    pub response: Response,
    pub raw: BodyView,
    /// Present when the body is JSON.
    pub pretty: Option<BodyView>,
    pub hex: BodyView,
    pub format: BodyFormat,
    /// The cookies the final response set (F17-AC4).
    pub cookies: Vec<Cookie>,
    /// A remark shown above the body, such as a history truncation.
    pub note: Option<String>,
    /// The find bar, while open (F06-AC8).
    pub find: Option<Find>,
    /// The Pretty view's `JSONPath` filter (F22-AC4).
    pub filter: Filter,
    /// The send that produced it; set when it reaches its tab.
    pub send: u64,
}

impl Received {
    // CONTRACT
    /// The view for the selected format. Pretty shows the filter's results
    /// when it has some, and falls back to Raw for bodies that are not JSON.
    #[must_use]
    pub fn view(&self) -> &BodyView {
        match self.format {
            BodyFormat::Pretty => match &self.filter.state {
                FilterState::Shown(view) => view,
                _ => self.pretty.as_ref().unwrap_or(&self.raw),
            },
            BodyFormat::Raw => &self.raw,
            BodyFormat::Hex => &self.hex,
            // The web view shows the body; the text views stay as they are.
            BodyFormat::Preview => &self.raw,
        }
    }

    pub(crate) fn view_mut(&mut self) -> &mut BodyView {
        match self.format {
            BodyFormat::Pretty => match &mut self.filter.state {
                FilterState::Shown(view) => view,
                _ => self.pretty.as_mut().unwrap_or(&mut self.raw),
            },
            BodyFormat::Raw => &mut self.raw,
            BodyFormat::Hex => &mut self.hex,
            BodyFormat::Preview => &mut self.raw,
        }
    }

    /// Every view, for messages that concern them all.
    fn views_mut(&mut self) -> impl Iterator<Item = &mut BodyView> {
        let filtered = match &mut self.filter.state {
            FilterState::Shown(view) => Some(view),
            _ => None,
        };
        [Some(&mut self.raw), Some(&mut self.hex)]
            .into_iter()
            .chain([self.pretty.as_mut(), filtered])
            .flatten()
    }

    // CONTRACT
    /// The formats offered for this body.
    #[must_use]
    pub fn formats(&self) -> Vec<BodyFormat> {
        BodyFormat::ALL
            .into_iter()
            .filter(|f| *f != BodyFormat::Pretty || self.pretty.is_some())
            .filter(|f| *f != BodyFormat::Preview || self.preview_type().is_some())
            .collect()
    }
}

// CONTRACT
/// The request builder's tabs (F14-AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestTab {
    #[default]
    Params,
    Authorization,
    Headers,
    Body,
    Scripts,
    Settings,
}

impl RequestTab {
    pub const ALL: [RequestTab; 6] = [
        RequestTab::Params,
        RequestTab::Authorization,
        RequestTab::Headers,
        RequestTab::Body,
        RequestTab::Scripts,
        RequestTab::Settings,
    ];
}

// CONTRACT
/// The response pane's tabs (F14-AC3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseTab {
    #[default]
    Body,
    Cookies,
    Headers,
    TestResults,
}

impl ResponseTab {
    pub const ALL: [ResponseTab; 4] = [
        ResponseTab::Body,
        ResponseTab::Cookies,
        ResponseTab::Headers,
        ResponseTab::TestResults,
    ];
}

// CONTRACT
/// The key-value tables a tab has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Table {
    Params,
    Headers,
    UrlEncoded,
    Form,
    /// The response headers of an example (F32-AC3).
    ExampleHeaders,
}

// CONTRACT
/// An edit to one row of a table. A row index equal to the table's length
/// addresses the trailing blank row: editing it appends a row (F01-AC8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowEdit {
    Key(String),
    Value(String),
    Description(String),
    Enabled(bool),
    Remove,
    /// Form-data only: stop sending a file and go back to text.
    ClearFile,
}

// CONTRACT
/// Which file a file picker fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTarget {
    /// A form-data row; the row index, or the table length for a new row.
    FormField(usize),
    Binary,
}

// CONTRACT
/// Edits to a tab's request.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Typed into the method field (F01-AC7).
    MethodText(String),
    MethodSelected(Method),
    Url(String),
    Row(Table, usize, RowEdit),
    /// Switch a table between rows and bulk-edit text (F02-AC4).
    ToggleBulk(Table),
    BulkText(Table, String),
    ToggleAutoHeaders,
    BodyMode(BodyMode),
    RawLanguage(RawLanguage),
    RawText(String),
    /// Re-indent raw JSON (F03-AC3).
    Beautify,
    ClearBinary,
    AuthKind(AuthKind),
    Username(String),
    Password(String),
    Token(String),
    ApiKey(String),
    ApiValue(String),
    ApiLocation(ApiKeyLocation),
    FollowRedirects(bool),
    VerifyTls(bool),
    /// `Send and store cookies` (F17-AC3).
    SendCookies(bool),
    /// A row of an environment's or a collection's variables (F08-AC8,
    /// F09-AC11).
    Var(usize, VarEdit),
    /// A collection's or folder's description (F09-AC11).
    Description(String),
    ContainerSection(ContainerSection),
    /// An example's name, status code and body (F32-AC3).
    ExampleName(String),
    ExampleStatus(String),
    ExampleBody(String),
}

// CONTRACT
/// One open request.
#[derive(Debug)]
pub struct Tab {
    pub id: TabId,
    pub draft: RequestDraft,
    /// The state the dirty dot compares against (F07-AC4).
    pub saved: RequestDraft,
    pub phase: Phase,
    pub request_tab: RequestTab,
    pub response_tab: ResponseTab,
    /// Text of the method field, which may be a half-typed method.
    pub method_text: String,
    pub method_error: Option<MethodError>,
    /// Bulk-edit text per table, while in bulk mode.
    pub bulk: Vec<(Table, String)>,
    pub show_auto_headers: bool,
    /// Problem with the raw JSON body, refreshed on every change.
    pub json_error: Option<JsonError>,
    /// Bumped whenever the app itself rewrites the raw body (Beautify,
    /// restore), so an editor widget knows to reload it.
    pub raw_revision: u64,
    /// Workspace path of the saved request this tab edits (F07-AC7).
    pub link: Option<String>,
    /// The saved request's name, while linked.
    pub name: Option<String>,
    /// What the tab holds; request state lives in the fields above.
    pub doc: Doc,
}

impl Tab {
    // CONTRACT
    /// A tab for `draft`, whose saved state is a new request.
    #[must_use]
    pub fn new(id: TabId, draft: RequestDraft) -> Self {
        let mut tab = Self {
            id,
            method_text: draft.method.to_string(),
            draft,
            saved: RequestDraft::default(),
            phase: Phase::Idle,
            request_tab: RequestTab::default(),
            response_tab: ResponseTab::default(),
            method_error: None,
            bulk: Vec::new(),
            show_auto_headers: false,
            json_error: None,
            raw_revision: 0,
            link: None,
            name: None,
            doc: Doc::Request,
        };
        tab.refresh_json_error();
        tab
    }

    // CONTRACT
    /// Replace the request, as when a saved request is opened or reloaded:
    /// editors reload, bulk edit closes, and the method field follows.
    pub fn set_draft(&mut self, draft: RequestDraft) {
        self.method_text = draft.method.to_string();
        self.method_error = None;
        self.draft = draft;
        self.bulk.clear();
        self.raw_revision += 1;
        self.refresh_json_error();
    }

    // CONTRACT
    /// The tab's title: a saved request's name, otherwise its URL or
    /// `Untitled Request` (F07-AC1, F07-AC7); for other tabs, the name of
    /// what they hold.
    #[must_use]
    pub fn title(&self) -> &str {
        match &self.doc {
            Doc::Request => {
                if let Some(name) = &self.name {
                    return name;
                }
                let url = self.draft.url.trim();
                if url.is_empty() {
                    "Untitled Request"
                } else {
                    url
                }
            }
            Doc::Environment(env) => &env.name,
            Doc::Container(c) => &c.name,
            Doc::Example(e) => &e.draft.name,
        }
    }

    // CONTRACT
    /// Whether the tab differs from its saved state (F07-AC4).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match &self.doc {
            Doc::Request => self.draft != self.saved,
            Doc::Environment(env) => env.draft != env.saved,
            Doc::Container(c) => c.draft != c.saved,
            Doc::Example(e) => e.draft != e.saved,
        }
    }

    #[must_use]
    pub fn is_sending(&self) -> bool {
        matches!(self.phase, Phase::Sending { .. })
    }

    // CONTRACT
    /// The bulk-edit text of `table`, if it is in bulk mode.
    #[must_use]
    pub fn bulk_text(&self, table: Table) -> Option<&str> {
        self.bulk
            .iter()
            .find(|(t, _)| *t == table)
            .map(|(_, text)| text.as_str())
    }

    // CONTRACT
    /// Apply `edit`. Returns `true` if the request changed.
    pub fn edit(&mut self, edit: Edit) -> bool {
        if self.doc != Doc::Request {
            return doc::edit(&mut self.doc, edit);
        }
        let before = self.draft.clone();
        let d = &mut self.draft;
        match edit {
            Edit::MethodText(text) => {
                match Method::parse(&text) {
                    Ok(method) => {
                        d.method = method;
                        self.method_error = None;
                    }
                    Err(e) => self.method_error = Some(e),
                }
                self.method_text = text;
            }
            Edit::MethodSelected(method) => {
                self.method_text = method.to_string();
                self.method_error = None;
                d.method = method;
            }
            Edit::Url(url) => {
                d.params = params::rows_from_url(&url, &d.params);
                d.url = url;
            }
            Edit::Row(table, index, row_edit) => self.edit_row(table, index, row_edit),
            Edit::ToggleBulk(table) => self.toggle_bulk(table),
            Edit::BulkText(table, text) => {
                if let Some((_, current)) = self.bulk.iter_mut().find(|(t, _)| *t == table) {
                    current.clone_from(&text);
                    self.apply_bulk(table, &text);
                }
            }
            Edit::ToggleAutoHeaders => self.show_auto_headers = !self.show_auto_headers,
            Edit::BodyMode(mode) => d.body.mode = mode,
            Edit::RawLanguage(language) => d.body.language = language,
            Edit::RawText(text) => d.body.raw = text,
            Edit::Beautify => {
                if let Some(pretty) = json::pretty(d.body.raw.as_bytes())
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                {
                    d.body.raw = pretty;
                    self.raw_revision += 1;
                }
            }
            Edit::ClearBinary => d.body.binary = None,
            Edit::AuthKind(kind) => d.auth.kind = kind,
            Edit::Username(v) => d.auth.username = v,
            Edit::Password(v) => d.auth.password = v,
            Edit::Token(v) => d.auth.token = v,
            Edit::ApiKey(v) => d.auth.key = v,
            Edit::ApiValue(v) => d.auth.value = v,
            Edit::ApiLocation(v) => d.auth.location = v,
            Edit::FollowRedirects(v) => d.options.follow_redirects = v,
            Edit::VerifyTls(v) => d.options.verify_tls = v,
            Edit::SendCookies(v) => d.options.cookies = v,
            // Edits for other kinds of tab.
            Edit::Var(..)
            | Edit::Description(_)
            | Edit::ContainerSection(_)
            | Edit::ExampleName(_)
            | Edit::ExampleStatus(_)
            | Edit::ExampleBody(_) => {}
        }
        self.refresh_json_error();
        self.draft != before
    }

    // CONTRACT
    /// Put a picked file into its place (F03-AC5, F03-AC6).
    pub fn file_picked(&mut self, target: FileTarget, path: PathBuf) {
        match target {
            FileTarget::Binary => self.draft.body.binary = Some(path),
            FileTarget::FormField(index) => {
                let form = &mut self.draft.body.form;
                if index >= form.len() {
                    form.push(FormField::default());
                }
                let last = form.len() - 1;
                form[index.min(last)].file = Some(path);
            }
        }
    }

    fn refresh_json_error(&mut self) {
        let body = &self.draft.body;
        self.json_error = (body.mode == BodyMode::Raw
            && body.language == RawLanguage::Json
            && !body.raw.trim().is_empty())
        .then(|| json::validate(&body.raw).err())
        .flatten();
    }

    fn edit_row(&mut self, table: Table, index: usize, edit: RowEdit) {
        let d = &mut self.draft;
        match table {
            Table::Params => {
                if apply_row(&mut d.params, index, edit, |p| &mut p.row) {
                    d.url = params::url_with_rows(&d.url, &d.params);
                }
            }
            Table::Headers => {
                apply_row(&mut d.headers, index, edit, |r| r);
            }
            Table::UrlEncoded => {
                apply_row(&mut d.body.urlencoded, index, edit, |r| r);
            }
            Table::ExampleHeaders => {}
            Table::Form => {
                if edit == RowEdit::ClearFile {
                    if let Some(field) = d.body.form.get_mut(index) {
                        field.file = None;
                    }
                } else {
                    apply_row(&mut d.body.form, index, edit, |f| &mut f.row);
                }
            }
        }
    }

    fn rows(&self, table: Table) -> Vec<KeyValue> {
        let d = &self.draft;
        match table {
            Table::Params => d.params.iter().map(|p| p.row.clone()).collect(),
            Table::Headers => d.headers.clone(),
            Table::UrlEncoded => d.body.urlencoded.clone(),
            Table::Form => d.body.form.iter().map(|f| f.row.clone()).collect(),
            Table::ExampleHeaders => Vec::new(),
        }
    }

    fn toggle_bulk(&mut self, table: Table) {
        if let Some(pos) = self.bulk.iter().position(|(t, _)| *t == table) {
            self.bulk.remove(pos);
        } else {
            let text = kv::to_bulk(&self.rows(table));
            self.bulk.push((table, text));
        }
    }

    fn apply_bulk(&mut self, table: Table, text: &str) {
        let rows = kv::from_bulk(text, &self.rows(table));
        let d = &mut self.draft;
        match table {
            Table::Params => {
                d.params = rows
                    .into_iter()
                    .map(|row| Param {
                        row,
                        original: None,
                    })
                    .collect();
                d.url = params::url_with_rows(&d.url, &d.params);
            }
            Table::Headers => d.headers = rows,
            Table::UrlEncoded => d.body.urlencoded = rows,
            Table::ExampleHeaders => {}
            Table::Form => {
                // Files stay with their rows by position.
                let files: Vec<Option<PathBuf>> =
                    d.body.form.iter().map(|f| f.file.clone()).collect();
                d.body.form = rows
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| FormField {
                        row,
                        file: files.get(i).cloned().flatten(),
                    })
                    .collect();
            }
        }
    }

    // CONTRACT
    /// Record a finished send, if it is this tab's current one.
    pub fn finished(&mut self, id: u64, result: Arc<crate::Finished>) {
        if !matches!(self.phase, Phase::Sending { id: current, .. } if current == id) {
            return;
        }
        let result = Arc::try_unwrap(result).unwrap_or_else(|shared| (*shared).clone());
        self.phase = match result {
            crate::Finished::Received(mut received) => {
                received.send = id;
                Phase::Received(received)
            }
            crate::Finished::Failed(text) => Phase::Failed(text),
            crate::Finished::Cancelled => Phase::Cancelled,
        };
    }

    // CONTRACT
    /// Parse the cookies the response just received set, for the Cookies
    /// tab (F17-AC4).
    pub fn received_cookies(&mut self, now: i64) {
        let Phase::Received(received) = &mut self.phase else {
            return;
        };
        let response = &received.response;
        let Ok(url) = url::Url::parse(&response.url) else {
            return;
        };
        let host = url.host_str().unwrap_or_default();
        received.cookies = response
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("set-cookie"))
            .filter_map(|h| {
                ferry_core::cookies::parse_set_cookie(&h.value, host, url.path(), now).ok()
            })
            .collect();
    }

    // CONTRACT
    /// Cancel the send in flight, if any.
    pub fn cancel(&mut self) {
        if let Phase::Sending { cancel, .. } = &self.phase {
            cancel.cancel();
            self.phase = Phase::Cancelled;
        }
    }

    pub(crate) fn body_message(&mut self, message: body_view::Message) {
        if let Phase::Received(received) = &mut self.phase {
            if let body_view::Message::Resized(_) = message {
                // The viewport is shared by every format.
                received.views_mut().for_each(|view| view.update(message));
            } else {
                received.view_mut().update(message);
            }
        }
    }

    pub(crate) fn select_format(&mut self, format: BodyFormat) {
        if let Phase::Received(received) = &mut self.phase
            && received.formats().contains(&format)
        {
            received.format = format;
        }
    }
}

/// Apply a row edit to `rows`, creating the row if `index` is the trailing
/// blank row. Returns whether anything changed.
pub(crate) fn apply_row<T: Default>(
    rows: &mut Vec<T>,
    index: usize,
    edit: RowEdit,
    row: impl Fn(&mut T) -> &mut KeyValue,
) -> bool {
    if index > rows.len() {
        return false;
    }
    if edit == RowEdit::Remove {
        if index < rows.len() {
            rows.remove(index);
            return true;
        }
        return false;
    }
    if index == rows.len() {
        if matches!(&edit, RowEdit::Key(s) | RowEdit::Value(s) | RowEdit::Description(s) if s.is_empty())
        {
            return false;
        }
        rows.push(T::default());
    }
    let target = row(&mut rows[index]);
    match edit {
        RowEdit::Key(v) => target.key = v,
        RowEdit::Value(v) => target.value = v,
        RowEdit::Description(v) => target.description = v,
        RowEdit::Enabled(v) => target.enabled = v,
        RowEdit::Remove | RowEdit::ClearFile => return false,
    }
    true
}
