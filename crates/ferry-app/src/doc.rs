//! What a workbench tab can hold besides a request: an environment or the
//! globals (F08-AC8), a collection's or folder's settings (F09-AC11), or a
//! saved example (F32-AC3). Each keeps its saved state and a draft.

use bytes::Bytes;
use ferry_core::auth::AuthDraft;
use ferry_core::collection::{Example, ItemKind};
use ferry_core::draft::RequestDraft;
use ferry_core::kv::KeyValue;
use ferry_core::response::Header;
use ferry_core::scripts::Scripts;
use ferry_core::variables::Variable;

use crate::tab::{Edit, Table, apply_row};

// CONTRACT
/// Which variables an environment tab edits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnvTarget {
    Globals,
    /// `environments/<file>.toml`.
    Environment(String),
}

// CONTRACT
/// An edit to one row of a variables table. A row index equal to the
/// table's length addresses the trailing blank row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarEdit {
    Key(String),
    Value(String),
    Enabled(bool),
    Secret(bool),
    Remove,
}

// CONTRACT
/// Apply a row edit, creating the row when `index` is the trailing blank
/// row. Returns whether anything changed.
pub fn apply_var(rows: &mut Vec<Variable>, index: usize, edit: VarEdit) -> bool {
    if index > rows.len() {
        return false;
    }
    if edit == VarEdit::Remove {
        if index < rows.len() {
            rows.remove(index);
            return true;
        }
        return false;
    }
    let created = index == rows.len();
    if created {
        let empty = matches!(&edit, VarEdit::Key(s) | VarEdit::Value(s) if s.is_empty());
        if empty {
            return false;
        }
        rows.push(Variable::default());
    }
    let row = &mut rows[index];
    let before = row.clone();
    match edit {
        VarEdit::Key(v) => row.key = v,
        VarEdit::Value(v) => row.value = v,
        VarEdit::Enabled(v) => row.enabled = v,
        VarEdit::Secret(v) => row.secret = v,
        VarEdit::Remove => {}
    }
    created || *row != before
}

// CONTRACT
/// An environment or the globals, open in a tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvDoc {
    pub target: EnvTarget,
    pub name: String,
    pub saved: Vec<Variable>,
    pub draft: Vec<Variable>,
}

// CONTRACT
/// The editable part of a collection's or folder's settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerFields {
    pub description: String,
    pub auth: AuthDraft,
    /// Collections only.
    pub variables: Vec<Variable>,
    pub scripts: Scripts,
}

// CONTRACT
/// The sections of a collection or folder tab (F09-AC11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContainerSection {
    #[default]
    Overview,
    Authorization,
    Variables,
    /// Pre-request and post-response scripts (F15-AC1).
    Scripts,
}

// CONTRACT
/// A collection's or folder's settings, open in a tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDoc {
    /// The directory.
    pub path: String,
    pub kind: ItemKind,
    pub name: String,
    pub section: ContainerSection,
    pub saved: ContainerFields,
    pub draft: ContainerFields,
}

// CONTRACT
/// The editable part of an example (F32-AC3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExampleFields {
    pub name: String,
    /// The status code as typed.
    pub status: String,
    pub headers: Vec<KeyValue>,
    /// The body as text; a body that is not UTF-8 shows lossily until edited.
    pub body: String,
    /// The original bytes, kept while `body` is unedited so a binary body
    /// survives a save of other fields.
    pub raw_body: Option<Bytes>,
}

impl ExampleFields {
    // CONTRACT
    /// The fields of a saved example.
    #[must_use]
    pub fn from_example(example: &Example) -> Self {
        let text = std::str::from_utf8(&example.body);
        Self {
            name: example.name.clone(),
            status: example.status.to_string(),
            headers: example
                .headers
                .iter()
                .map(|h| KeyValue::new(h.name.clone(), h.value.clone()))
                .collect(),
            body: String::from_utf8_lossy(&example.body).into_owned(),
            raw_body: text.is_err().then(|| example.body.clone()),
        }
    }

    // CONTRACT
    /// The example these fields describe, with `request`.
    ///
    /// # Errors
    /// `Enter a status code from 100 to 599` for an invalid status.
    pub fn to_example(&self, request: &RequestDraft) -> Result<Example, String> {
        let status = self
            .status
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|s| (100..=599).contains(s))
            .ok_or_else(|| "Enter a status code from 100 to 599".to_owned())?;
        Ok(Example {
            name: self.name.clone(),
            request: request.clone(),
            status,
            headers: self
                .headers
                .iter()
                .filter(|h| h.is_active())
                .map(|h| Header {
                    name: h.key.trim().to_owned(),
                    value: h.value.clone(),
                })
                .collect(),
            body: self
                .raw_body
                .clone()
                .unwrap_or_else(|| Bytes::from(self.body.clone())),
        })
    }
}

// CONTRACT
/// A saved example, open in a tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExampleDoc {
    /// The request file holding the example.
    pub path: String,
    pub index: usize,
    /// The request as saved with the example (shown read-only).
    pub request: RequestDraft,
    pub saved: ExampleFields,
    pub draft: ExampleFields,
    /// Why the last save was refused.
    pub error: Option<String>,
    /// Bumped when the app replaces the body, so an editor reloads it.
    pub body_revision: u64,
}

// CONTRACT
/// What a tab holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Doc {
    /// A request; its state lives in the tab's own fields.
    #[default]
    Request,
    Environment(EnvDoc),
    Container(Box<ContainerDoc>),
    Example(Box<ExampleDoc>),
    /// The Collection Runner (F16).
    Runner(Box<crate::runner_tab::RunnerDoc>),
}

// CONTRACT
/// Apply an edit to a non-request tab. Returns whether its draft changed.
pub(crate) fn edit(doc: &mut Doc, edit: Edit) -> bool {
    match doc {
        Doc::Request => false,
        Doc::Environment(env) => match edit {
            Edit::Var(i, e) => apply_var(&mut env.draft, i, e),
            _ => false,
        },
        Doc::Container(c) => {
            let before = c.draft.clone();
            let d = &mut c.draft;
            match edit {
                Edit::Var(i, e) => {
                    apply_var(&mut d.variables, i, e);
                }
                Edit::Description(text) => d.description = text,
                Edit::ContainerSection(section) => c.section = section,
                Edit::AuthKind(kind) => d.auth.kind = kind,
                Edit::Username(v) => d.auth.username = v,
                Edit::Password(v) => d.auth.password = v,
                Edit::Token(v) => d.auth.token = v,
                Edit::ApiKey(v) => d.auth.key = v,
                Edit::ApiValue(v) => d.auth.value = v,
                Edit::ApiLocation(v) => d.auth.location = v,
                Edit::Script(phase, text) => d.scripts.set(phase, text),
                _ => {}
            }
            c.draft != before
        }
        Doc::Example(e) => {
            let before = e.draft.clone();
            let d = &mut e.draft;
            match edit {
                Edit::ExampleName(name) => d.name = name,
                Edit::ExampleStatus(status) => d.status = status,
                Edit::ExampleBody(body) => {
                    d.body = body;
                    d.raw_body = None;
                }
                Edit::Row(Table::ExampleHeaders, i, row) => {
                    apply_row(&mut d.headers, i, row, |r| r);
                }
                _ => {}
            }
            e.draft != before
        }
        Doc::Runner(_) => false,
    }
}
