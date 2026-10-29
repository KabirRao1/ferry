//! Collections in the workspace: file formats, item order and tree queries
//! (F09-AC2, AC3, AC11, AC12, AC13, F04-AC6, F32-AC2, ADR-0037).
//!
//! Paths are relative to the workspace with `/` separators. A collection or
//! folder is identified by its directory, a request by its `.toml` file.

use std::fmt;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

use base64::Engine as _;

use crate::auth::{AuthDraft, AuthKind};
use crate::draft::RequestDraft;
use crate::environment;
use crate::environment::to_toml;
use crate::params;
use crate::response::Header;
use crate::scripts::Scripts;
use crate::variables::Variable;

/// Settings file of a collection directory.
pub const COLLECTION_FILE: &str = "collection.toml";
/// Optional settings file of a folder directory.
pub const FOLDER_FILE: &str = "folder.toml";
/// Optional item order of a directory (ADR-0013).
pub const ORDER_FILE: &str = "_order.toml";
/// Extension of request files.
pub const REQUEST_EXT: &str = ".toml";
/// Directory of collections inside the workspace.
pub const COLLECTIONS_DIR: &str = "collections";

// CONTRACT
/// Why a file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

/// A parse error as one line: `line <n>: <message>` when the position is
/// known.
pub(crate) fn toml_error(text: &str, error: &toml::de::Error) -> FormatError {
    let message = error.message().trim();
    match error.span() {
        Some(span) => {
            let line = text
                .get(..span.start)
                .map_or(1, |before| before.matches('\n').count() + 1);
            FormatError(format!("line {line}: {message}"))
        }
        None => FormatError(message.to_owned()),
    }
}

#[derive(Serialize, Deserialize)]
struct RequestRepr {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    #[serde(flatten)]
    request: RequestDraft,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    examples: Vec<ExampleRepr>,
}

#[derive(Serialize, Deserialize)]
struct ExampleRepr {
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_encoding: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    headers: Vec<Header>,
    #[serde(default, skip_serializing_if = "crate::is_default")]
    request: RequestDraft,
}

const BASE64: &str = "base64";

impl ExampleRepr {
    fn from_example(e: &Example) -> Self {
        let (body, body_encoding) = match std::str::from_utf8(&e.body) {
            Ok(text) => (text.to_owned(), None),
            Err(_) => (
                base64::engine::general_purpose::STANDARD.encode(&e.body),
                Some(BASE64.to_owned()),
            ),
        };
        Self {
            name: e.name.clone(),
            status: e.status,
            body,
            body_encoding,
            headers: e.headers.clone(),
            request: e.request.clone(),
        }
    }

    fn into_example(self) -> Result<Example, FormatError> {
        let body = match self.body_encoding.as_deref() {
            None => Bytes::from(self.body),
            Some(BASE64) => base64::engine::general_purpose::STANDARD
                .decode(self.body.as_bytes())
                .map(Bytes::from)
                .map_err(|e| {
                    FormatError(format!(
                        "example \"{}\": invalid base64 body: {e}",
                        self.name
                    ))
                })?,
            Some(other) => {
                return Err(FormatError(format!(
                    "example \"{}\": unknown body_encoding \"{other}\"",
                    self.name
                )));
            }
        };
        let mut request = self.request;
        restore_originals(&mut request);
        Ok(Example {
            name: self.name,
            request,
            status: self.status,
            headers: self.headers,
            body,
        })
    }
}

/// Give params their text in the URL, so untouched ones keep their
/// spelling when the table is edited (F01-AC4).
fn restore_originals(request: &mut RequestDraft) {
    let mut fresh = params::rows_from_url(&request.url, &[]).into_iter();
    for param in request.params.iter_mut().filter(|p| p.row.enabled) {
        let Some(found) = fresh.next() else { break };
        if found.row.key == param.row.key && found.row.value == param.row.value {
            param.original = found.original;
        }
    }
}

// CONTRACT
/// A saved example of a request (F32).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Example {
    pub name: String,
    /// The request as edited when the example was saved.
    pub request: RequestDraft,
    pub status: u16,
    pub headers: Vec<Header>,
    pub body: Bytes,
}

// CONTRACT
/// A request file: its name, the request, and its examples.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestFile {
    pub name: String,
    /// Shown nowhere yet; kept for import and export (F11-AC3).
    pub description: String,
    pub request: RequestDraft,
    pub examples: Vec<Example>,
}

// CONTRACT
/// `collection.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CollectionFile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub auth: AuthDraft,
    /// With local values merged in (F15-AC5, ADR-0045).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<Variable>,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub scripts: Scripts,
}

// CONTRACT
/// `folder.toml`. A folder's auth defaults to Inherit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderFile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub auth: AuthDraft,
    #[serde(skip_serializing_if = "crate::is_default")]
    pub scripts: Scripts,
}

impl Default for FolderFile {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            auth: AuthDraft {
                kind: AuthKind::Inherit,
                ..AuthDraft::default()
            },
            scripts: Scripts::default(),
        }
    }
}

// CONTRACT
/// Parse a request file; a missing `name` becomes `fallback_name`.
///
/// # Errors
/// [`FormatError`] if the text is not a valid request file.
pub fn parse_request(text: &str, fallback_name: &str) -> Result<RequestFile, FormatError> {
    let repr: RequestRepr = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    let mut request = repr.request;
    restore_originals(&mut request);
    Ok(RequestFile {
        name: repr.name.unwrap_or_else(|| fallback_name.to_owned()),
        description: repr.description,
        request,
        examples: repr
            .examples
            .into_iter()
            .map(ExampleRepr::into_example)
            .collect::<Result<_, _>>()?,
    })
}

// CONTRACT
/// A request file's text. Fields at their default values are omitted.
#[must_use]
pub fn write_request(file: &RequestFile) -> String {
    to_toml(&RequestRepr {
        name: Some(file.name.clone()),
        description: file.description.clone(),
        request: file.request.clone(),
        examples: file
            .examples
            .iter()
            .map(ExampleRepr::from_example)
            .collect(),
    })
}

// CONTRACT
/// Parse `collection.toml`; a missing `name` becomes `fallback_name`.
///
/// # Errors
/// [`FormatError`] if the text is not valid.
pub fn parse_collection(text: &str, fallback_name: &str) -> Result<CollectionFile, FormatError> {
    #[derive(Deserialize)]
    struct Named {
        name: Option<String>,
    }
    let mut file: CollectionFile = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    let named: Named = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    if named.name.is_none() {
        fallback_name.clone_into(&mut file.name);
    }
    Ok(file)
}

// CONTRACT
/// `collection.toml`'s text: variables with their shared values, and none
/// that exist only in the local file (F15-AC5).
#[must_use]
pub fn write_collection(file: &CollectionFile) -> String {
    let shared = CollectionFile {
        variables: environment::shared_variables(&file.variables),
        ..file.clone()
    };
    to_toml(&shared)
}

// CONTRACT
/// `collection.local.toml`'s text, or `None` when no value belongs there
/// (F15-AC5, ADR-0045).
#[must_use]
pub fn write_collection_local(file: &CollectionFile) -> Option<String> {
    environment::local_text(&file.variables)
}

// CONTRACT
/// Merge `collection.local.toml` into a parsed collection (F15-AC5).
///
/// # Errors
/// [`FormatError`] if the local file is not valid.
pub fn merge_collection_local(file: &mut CollectionFile, local: &str) -> Result<(), FormatError> {
    environment::merge_local(&mut file.variables, local)
}

// CONTRACT
/// Parse `folder.toml`; a missing `name` becomes `fallback_name`.
///
/// # Errors
/// [`FormatError`] if the text is not valid.
pub fn parse_folder(text: &str, fallback_name: &str) -> Result<FolderFile, FormatError> {
    #[derive(Deserialize)]
    struct Named {
        name: Option<String>,
    }
    let mut file: FolderFile = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    let named: Named = toml::from_str(text).map_err(|e| toml_error(text, &e))?;
    if named.name.is_none() {
        fallback_name.clone_into(&mut file.name);
    }
    Ok(file)
}

// CONTRACT
#[must_use]
pub fn write_folder(file: &FolderFile) -> String {
    to_toml(file)
}

// CONTRACT
/// The names listed in an `_order.toml` (`order = [...]`).
///
/// # Errors
/// [`FormatError`] if the text is not valid.
pub fn parse_order(text: &str) -> Result<Vec<String>, FormatError> {
    Ok(toml::from_str::<OrderRepr>(text)
        .map_err(|e| toml_error(text, &e))?
        .order)
}

#[derive(Serialize, Deserialize)]
struct OrderRepr {
    #[serde(default)]
    order: Vec<String>,
}

// CONTRACT
#[must_use]
pub fn write_order(names: &[String]) -> String {
    to_toml(&OrderRepr {
        order: names.to_vec(),
    })
}

// CONTRACT
/// The kind of a workspace item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Collection,
    Folder,
    Request,
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Collection => "collection",
            Self::Folder => "folder",
            Self::Request => "request",
        })
    }
}

// CONTRACT
/// What an item's file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // a few hundred bytes per tree item
pub enum Node {
    Collection(CollectionFile),
    Folder(FolderFile),
    Request(RequestFile),
    /// The file could not be read or parsed (F09-AC12). Folders and
    /// collections still list their items.
    Broken {
        kind: ItemKind,
        reason: String,
    },
}

// CONTRACT
/// One item of the collections tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Directory for collections and folders, file for requests.
    pub path: String,
    pub node: Node,
    /// Children in display order; empty for requests.
    pub items: Vec<Item>,
}

impl Item {
    // CONTRACT
    #[must_use]
    pub fn kind(&self) -> ItemKind {
        match &self.node {
            Node::Collection(_) => ItemKind::Collection,
            Node::Folder(_) => ItemKind::Folder,
            Node::Request(_) => ItemKind::Request,
            Node::Broken { kind, .. } => *kind,
        }
    }

    // CONTRACT
    /// The display name: the file's `name`, or for a broken file the file
    /// or directory name without `.toml`.
    #[must_use]
    pub fn name(&self) -> &str {
        match &self.node {
            Node::Collection(c) => &c.name,
            Node::Folder(f) => &f.name,
            Node::Request(r) => &r.name,
            Node::Broken { .. } => {
                let name = file_name(&self.path);
                name.strip_suffix(REQUEST_EXT).unwrap_or(name)
            }
        }
    }

    // CONTRACT
    /// The problem text for a broken item: `Could not read <path>: <reason>`.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match &self.node {
            Node::Broken { reason, .. } => Some(format!("Could not read {}: {reason}", self.path)),
            _ => None,
        }
    }
}

// CONTRACT
/// The last segment of a path.
#[must_use]
pub fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

// CONTRACT
/// Everything before the last `/`, or `""`.
#[must_use]
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

// CONTRACT
/// `items` in display order: names listed in `order` (matched against each
/// item's file or directory name) first, in that order; the rest by display
/// name ignoring case, then by file name (F09-AC3).
#[must_use]
pub fn arrange(mut items: Vec<Item>, order: &[String]) -> Vec<Item> {
    items.sort_by_cached_key(|item| {
        let name = file_name(&item.path);
        let listed = order.iter().position(|o| o == name).unwrap_or(usize::MAX);
        (listed, item.name().to_lowercase(), name.to_owned())
    });
    items
}

// CONTRACT
/// The item at `path`, searching the whole tree.
#[must_use]
pub fn find<'a>(items: &'a [Item], path: &str) -> Option<&'a Item> {
    items.iter().find_map(|item| {
        if item.path == path {
            Some(item)
        } else {
            find(&item.items, path)
        }
    })
}

// CONTRACT
/// The items containing `path`, collection first; empty if `path` is not
/// in the tree.
#[must_use]
pub fn ancestors<'a>(items: &'a [Item], path: &str) -> Vec<&'a Item> {
    for item in items {
        if item.path == path {
            return Vec::new();
        }
        if find(&item.items, path).is_some() {
            let mut chain = vec![item];
            chain.extend(ancestors(&item.items, path));
            return chain;
        }
    }
    Vec::new()
}

// CONTRACT
/// Where an inherited auth comes from (F04-AC6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSource {
    pub kind: ItemKind,
    pub name: String,
}

// CONTRACT
/// The auth that Inherit means for the item at `path`: the nearest
/// ancestor folder or collection whose type is not Inherit, skipping broken
/// ones. A collection's Inherit means No Auth. `None` if `path` is not in
/// the tree.
#[must_use]
pub fn inherited_auth(items: &[Item], path: &str) -> Option<(AuthDraft, AuthSource)> {
    let chain = ancestors(items, path);
    for item in chain.iter().rev() {
        let auth = match &item.node {
            Node::Collection(c) if c.auth.kind == AuthKind::Inherit => AuthDraft::default(),
            Node::Collection(c) => c.auth.clone(),
            Node::Folder(f) if f.auth.kind != AuthKind::Inherit => f.auth.clone(),
            _ => continue,
        };
        return Some((
            auth,
            AuthSource {
                kind: item.kind(),
                name: item.name().to_owned(),
            },
        ));
    }
    None
}

// CONTRACT
/// The Authorization tab's note for Inherit (F04-AC6).
#[must_use]
pub fn inherit_note(source: Option<&(AuthDraft, AuthSource)>) -> String {
    match source {
        Some((auth, source)) => format!(
            "This request is using {} from {} \"{}\".",
            auth.kind, source.kind, source.name
        ),
        None => {
            "This request is not saved in a collection, so it has no parent to inherit auth from."
                .to_owned()
        }
    }
}

// CONTRACT
/// The variables of the collection containing `path` (empty if none or if
/// its `collection.toml` is broken).
#[must_use]
pub fn collection_variables<'a>(items: &'a [Item], path: &str) -> &'a [Variable] {
    let top = items
        .iter()
        .find(|c| c.path == path || find(&c.items, path).is_some());
    match top.map(|c| &c.node) {
        Some(Node::Collection(c)) => &c.variables,
        _ => &[],
    }
}

// CONTRACT
/// A saved request matching a search (F09-AC13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub method: String,
    pub url: String,
    /// Names of the containing collection and folders.
    pub location: Vec<String>,
}

// CONTRACT
/// Saved requests whose name or URL contains `query`, ignoring case, in tree
/// order. An empty query matches nothing.
#[must_use]
pub fn search(items: &[Item], query: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    if !query.is_empty() {
        collect_hits(items, &query.to_lowercase(), &mut Vec::new(), &mut hits);
    }
    hits
}

fn collect_hits(items: &[Item], query: &str, location: &mut Vec<String>, hits: &mut Vec<Hit>) {
    for item in items {
        if let Node::Request(r) = &item.node {
            if r.name.to_lowercase().contains(query) || r.request.url.to_lowercase().contains(query)
            {
                hits.push(Hit {
                    path: item.path.clone(),
                    name: r.name.clone(),
                    method: r.request.method.to_string(),
                    url: r.request.url.clone(),
                    location: location.clone(),
                });
            }
        } else {
            location.push(item.name().to_owned());
            collect_hits(&item.items, query, location, hits);
            location.pop();
        }
    }
}
