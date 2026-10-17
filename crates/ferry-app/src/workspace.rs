//! Workspace actions on [`App`]: the sidebar tree, saving, environments,
//! dialogs, drag and drop, and reconciling tabs with a freshly loaded
//! workspace (F04-AC6, F08, F09, ADR-0037).
//!
//! `update()` never touches files: every change becomes an
//! [`Effect::Workspace`] with the operations to apply, and the reloaded
//! workspace comes back as [`Message::WorkspaceApplied`].

use std::path::PathBuf;
use std::sync::Arc;

use ferry_core::auth::{AuthDraft, AuthKind};
use ferry_core::collection::{
    self, COLLECTION_FILE, COLLECTIONS_DIR, CollectionFile, FOLDER_FILE, FolderFile, Hit, Item,
    ItemKind, Node, ORDER_FILE, REQUEST_EXT, RequestFile,
};
use ferry_core::draft::RequestDraft;
use ferry_core::environment::{self, VariablesFile};
use ferry_core::names;
use ferry_core::variables::{self, ScopeKind, Variable};
use ferry_core::workspace::{ENVIRONMENTS_DIR, GLOBALS_FILE, Op, Workspace};

use crate::app::{App, Effect, Message, vars_for};
use crate::doc::{
    ContainerDoc, ContainerFields, ContainerSection, Doc, EnvDoc, EnvTarget, ExampleDoc,
    ExampleFields,
};
use crate::tab::Tab;

/// Name of the default new items (F09-AC6, F08-AC8).
const NEW_COLLECTION: &str = "New Collection";
const NEW_FOLDER: &str = "New Folder";
const NEW_REQUEST: &str = "New Request";
const NEW_ENVIRONMENT: &str = "New Environment";
/// Title of the globals tab.
pub const GLOBALS: &str = "Globals";
/// Error for an empty name in the Save and Rename dialogs.
pub const ENTER_A_NAME: &str = "Enter a name";

// CONTRACT
/// Something in the sidebar that can be renamed, duplicated or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// A collection, folder or request, by path.
    Item(String),
    /// An environment file.
    Environment(String),
    /// An example: its request file and index.
    Example(String, usize),
}

// CONTRACT
/// A modal dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// Save an unlinked request (F09-AC5).
    SaveAs {
        name: String,
        /// The collection or folder to save into.
        target: Option<String>,
        /// Name of the collection to create when there is none.
        new_collection: String,
        error: Option<String>,
    },
    Rename {
        target: Target,
        name: String,
        error: Option<String>,
    },
    /// `Delete "<name>"?` or `Delete environment "<name>"?`.
    ConfirmDelete { target: Target, name: String },
}

// CONTRACT
/// How a variable the request uses resolves (F08-AC6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarState {
    Defined {
        value: String,
        scope: ScopeKind,
        secret: bool,
    },
    /// `$guid` and friends.
    Dynamic,
    Missing,
}

// CONTRACT
/// One entry of the variables strip under the URL bar (F08-AC6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarUse {
    pub name: String,
    pub state: VarState,
}

/// `parent/name`.
fn join(parent: &str, name: &str) -> String {
    format!("{parent}/{name}")
}

/// Whether `path` is `root` or inside it.
fn within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `path` with its `from` prefix replaced by `to`, if it is within `from`.
fn moved(path: &str, from: &str, to: &str) -> Option<String> {
    within(path, from).then(|| format!("{to}{}", &path[from.len()..]))
}

fn workspace_effect(action: impl Into<String>, ops: Vec<Op>) -> Option<Effect> {
    Some(Effect::Workspace {
        action: action.into(),
        ops,
    })
}

/// The settings file of a container directory.
fn settings_path(path: &str, kind: ItemKind) -> String {
    join(
        path,
        if kind == ItemKind::Collection {
            COLLECTION_FILE
        } else {
            FOLDER_FILE
        },
    )
}

/// A container's settings file text with a new name.
fn settings_with_name(node: &Node, name: &str) -> Option<String> {
    match node {
        Node::Collection(c) => Some(collection::write_collection(&CollectionFile {
            name: name.to_owned(),
            ..c.clone()
        })),
        Node::Folder(f) => Some(collection::write_folder(&FolderFile {
            name: name.to_owned(),
            ..f.clone()
        })),
        _ => None,
    }
}

fn container_fields(node: &Node) -> Option<ContainerFields> {
    match node {
        Node::Collection(c) => Some(ContainerFields {
            description: c.description.clone(),
            auth: c.auth.clone(),
            variables: c.variables.clone(),
        }),
        Node::Folder(f) => Some(ContainerFields {
            description: f.description.clone(),
            auth: f.auth.clone(),
            variables: Vec::new(),
        }),
        _ => None,
    }
}

/// The name and variables an environment tab shows, if readable.
fn env_vars(workspace: &Workspace, target: &EnvTarget) -> Option<(String, Vec<Variable>)> {
    match target {
        EnvTarget::Globals => workspace
            .globals
            .as_ref()
            .ok()
            .map(|g| (GLOBALS.to_owned(), g.variables.clone())),
        EnvTarget::Environment(path) => {
            let entry = workspace.environment(path)?;
            let file = entry.file.as_ref().ok()?;
            Some((entry.name().to_owned(), file.variables.clone()))
        }
    }
}

/// The file names of a container's children, in display order. The
/// `collections` directory lists the collections.
fn children(workspace: &Workspace, container: &str) -> Vec<String> {
    let items: &[Item] = if container == COLLECTIONS_DIR {
        &workspace.collections
    } else {
        match collection::find(&workspace.collections, container) {
            Some(item) => &item.items,
            None => &[],
        }
    };
    items
        .iter()
        .map(|i| collection::file_name(&i.path).to_owned())
        .collect()
}

fn env_file_names(workspace: &Workspace, except: Option<&str>) -> Vec<String> {
    workspace
        .environments
        .iter()
        .filter(|e| Some(e.path.as_str()) != except)
        .map(|e| collection::file_name(&e.path).to_owned())
        .collect()
}

fn as_strs(names: &[String]) -> Vec<&str> {
    names.iter().map(String::as_str).collect()
}

fn kind_noun(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Collection => "collection",
        ItemKind::Folder => "folder",
        ItemKind::Request => "request",
    }
}

impl App {
    // CONTRACT
    /// Replace the workspace and reconcile tabs with it (F09-AC10).
    pub(crate) fn workspace_loaded(&mut self, workspace: Arc<Workspace>) {
        self.workspace = workspace;
        self.reconcile();
    }

    /// Bring every tab in line with the loaded workspace: linked requests
    /// take the file's contents unless they have unsaved changes, and tabs
    /// whose file is gone are unlinked (requests) or closed (others).
    fn reconcile(&mut self) {
        let ws = Arc::clone(&self.workspace);
        let mut gone = Vec::new();
        for tab in &mut self.tabs {
            match &mut tab.doc {
                Doc::Request => {
                    let Some(link) = tab.link.clone() else {
                        continue;
                    };
                    match ws.request(&link) {
                        Some(file) => {
                            tab.name = Some(file.name.clone());
                            let dirty = tab.draft != tab.saved;
                            if !dirty && tab.draft != file.request {
                                tab.set_draft(file.request.clone());
                            }
                            tab.saved = file.request.clone();
                        }
                        None => {
                            tab.link = None;
                            tab.name = None;
                            tab.saved = RequestDraft::default();
                        }
                    }
                }
                Doc::Environment(env) => match env_vars(&ws, &env.target) {
                    Some((name, vars)) => {
                        env.name = name;
                        // Take the loaded rows when nothing visible differs,
                        // so where each value is stored stays current.
                        if env.draft == env.saved || same_rows(&env.draft, &vars) {
                            env.draft.clone_from(&vars);
                        }
                        env.saved = vars;
                    }
                    None => gone.push(tab.id),
                },
                Doc::Container(c) => match collection::find(&ws.collections, &c.path) {
                    Some(item) => match container_fields(&item.node) {
                        Some(fields) => {
                            item.name().clone_into(&mut c.name);
                            let same = c.draft.description == fields.description
                                && c.draft.auth == fields.auth
                                && same_rows(&c.draft.variables, &fields.variables);
                            if c.draft == c.saved || same {
                                c.draft = fields.clone();
                            }
                            c.saved = fields;
                        }
                        None => gone.push(tab.id),
                    },
                    None => gone.push(tab.id),
                },
                Doc::Example(e) => {
                    match ws.request(&e.path).and_then(|f| f.examples.get(e.index)) {
                        Some(example) => {
                            let fields = ExampleFields::from_example(example);
                            e.request = example.request.clone();
                            if e.draft == e.saved && e.draft != fields {
                                e.draft = fields.clone();
                                e.body_revision += 1;
                            }
                            e.saved = fields;
                        }
                        None => gone.push(tab.id),
                    }
                }
            }
        }
        for id in gone {
            self.discard_tab(id);
        }
        if let Some(env) = &self.environment
            && !Self::selectable(&ws, env)
        {
            self.environment = None;
            self.session_changed = true;
        }
        if let Some(menu) = &self.menu
            && collection::find(&ws.collections, menu).is_none()
            && ws.environment(menu).is_none()
        {
            self.menu = None;
        }
        self.session_changed = true;
    }

    fn selectable(workspace: &Workspace, path: &str) -> bool {
        workspace.environment(path).is_some_and(|e| e.file.is_ok())
    }

    /// Close a tab without asking, keeping at least one open.
    fn discard_tab(&mut self, id: crate::tab::TabId) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        let mut tab = self.tabs.remove(i);
        tab.cancel();
        if self.tabs.is_empty() {
            self.push_doc(Doc::Request);
        }
        if i < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    // CONTRACT
    /// Saved requests matching the header search (F09-AC13).
    #[must_use]
    pub fn search_hits(&self) -> Vec<Hit> {
        collection::search(&self.workspace.collections, &self.search)
    }

    // CONTRACT
    /// The variables the active request uses and how they resolve (F08-AC6).
    #[must_use]
    pub fn variable_strip(&self) -> Vec<VarUse> {
        let tab = self.tab();
        if tab.doc != Doc::Request {
            return Vec::new();
        }
        let mut draft = tab.draft.clone();
        if draft.auth.kind == AuthKind::Inherit {
            draft.auth = self
                .inherited_auth(tab)
                .map(|(auth, _)| auth)
                .unwrap_or_default();
        }
        let vars = vars_for(
            &self.workspace,
            self.environment.as_deref(),
            tab.link.as_deref(),
        );
        draft
            .variable_names()
            .into_iter()
            .map(|name| {
                let state = match vars.lookup(&name) {
                    Some((variable, scope)) => VarState::Defined {
                        value: variable.value.clone(),
                        scope,
                        secret: variable.secret,
                    },
                    None if variables::is_dynamic(&name) => VarState::Dynamic,
                    None => VarState::Missing,
                };
                VarUse { name, state }
            })
            .collect()
    }

    // CONTRACT
    /// The location of a request tab: collection and folder names, then
    /// the request name; empty for unlinked tabs (F14-AC6).
    #[must_use]
    pub fn location(&self, tab: &Tab) -> Vec<String> {
        let Some(link) = tab.link.as_deref() else {
            return Vec::new();
        };
        let mut names: Vec<String> = collection::ancestors(&self.workspace.collections, link)
            .iter()
            .map(|i| i.name().to_owned())
            .collect();
        names.push(tab.title().to_owned());
        names
    }

    pub(crate) fn workspace_message(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::WorkspaceLoaded(workspace) => self.workspace_loaded(workspace),
            Message::WorkspaceApplied {
                action,
                error,
                workspace,
            } => {
                if let Some(error) = error {
                    self.status = Some(format!("Could not {action}: {error}"));
                }
                self.workspace_loaded(workspace);
            }
            Message::OpenWorkspace => return Some(Effect::PickWorkspace),
            Message::WorkspacePicked(Some(path)) => return self.switch_workspace(path),
            Message::Save => return self.save(),
            Message::Search(text) => self.search = text,
            Message::ToggleExpanded(path) => {
                if !self.expanded.remove(&path) {
                    self.expanded.insert(path);
                }
            }
            Message::OpenItem(path) => self.open_item(&path),
            Message::Menu(path) => self.menu = path,
            Message::NewCollection => return self.new_collection(),
            Message::NewFolder(parent) => return self.new_folder(&parent),
            Message::NewRequest(parent) => return self.new_request(&parent),
            Message::Rename(target) => {
                self.menu = None;
                self.start_rename(target);
            }
            Message::Duplicate(target) => {
                self.menu = None;
                return self.duplicate(&target);
            }
            Message::Delete(target) => {
                self.menu = None;
                self.start_delete(target);
            }
            Message::ItemPressed(path) => {
                self.tree_drag = Some(path);
                self.tree_target = None;
            }
            Message::ItemEntered(path) => {
                if self.tree_drag.is_some() {
                    self.tree_target = Some(path);
                }
            }
            Message::ItemReleased => return self.drop_item(),
            Message::OpenExample(path, index) => self.open_example(&path, index),
            Message::SelectEnvironment(path) => self.select_environment(path),
            Message::NewEnvironment => return self.new_environment(),
            Message::OpenEnvironment(target) => self.open_environment(target),
            Message::DialogInput(text) => {
                if let Some(
                    Dialog::SaveAs { name, error, .. } | Dialog::Rename { name, error, .. },
                ) = &mut self.dialog
                {
                    *name = text;
                    *error = None;
                }
            }
            Message::DialogPick(path) => {
                if let Some(Dialog::SaveAs { target, .. }) = &mut self.dialog {
                    *target = Some(path);
                }
            }
            Message::DialogCollectionName(text) => {
                if let Some(Dialog::SaveAs { new_collection, .. }) = &mut self.dialog {
                    *new_collection = text;
                }
            }
            Message::DialogConfirm => return self.confirm_dialog(),
            Message::DialogCancel => self.dialog = None,
            Message::DismissStatus => self.status = None,
            _ => {}
        }
        None
    }

    // --- opening ---------------------------------------------------------------------

    fn open_item(&mut self, path: &str) {
        let ws = Arc::clone(&self.workspace);
        let Some(item) = collection::find(&ws.collections, path) else {
            return;
        };
        match &item.node {
            Node::Request(file) => {
                let existing = self
                    .tabs
                    .iter()
                    .position(|t| t.doc == Doc::Request && t.link.as_deref() == Some(path));
                if let Some(i) = existing {
                    self.active = i;
                } else {
                    self.push_doc(Doc::Request);
                    let tab = &mut self.tabs[self.active];
                    tab.set_draft(file.request.clone());
                    tab.saved = file.request.clone();
                    tab.link = Some(path.to_owned());
                    tab.name = Some(file.name.clone());
                }
                self.session_changed = true;
            }
            Node::Collection(_) | Node::Folder(_) => {
                self.expanded.insert(path.to_owned());
                let existing = self
                    .tabs
                    .iter()
                    .position(|t| matches!(&t.doc, Doc::Container(c) if c.path == path));
                if let Some(i) = existing {
                    self.active = i;
                } else if let Some(fields) = container_fields(&item.node) {
                    self.push_doc(Doc::Container(Box::new(ContainerDoc {
                        path: path.to_owned(),
                        kind: item.kind(),
                        name: item.name().to_owned(),
                        section: ContainerSection::default(),
                        saved: fields.clone(),
                        draft: fields,
                    })));
                }
            }
            Node::Broken { .. } => {}
        }
    }

    fn open_example(&mut self, path: &str, index: usize) {
        let existing = self
            .tabs
            .iter()
            .position(|t| matches!(&t.doc, Doc::Example(e) if e.path == path && e.index == index));
        if let Some(i) = existing {
            self.active = i;
            return;
        }
        let Some(example) = self
            .workspace
            .request(path)
            .and_then(|f| f.examples.get(index))
        else {
            return;
        };
        let fields = ExampleFields::from_example(example);
        let doc = ExampleDoc {
            path: path.to_owned(),
            index,
            request: example.request.clone(),
            saved: fields.clone(),
            draft: fields,
            error: None,
            body_revision: 0,
        };
        self.push_doc(Doc::Example(Box::new(doc)));
    }

    fn select_environment(&mut self, path: Option<String>) {
        match path {
            Some(path) if !Self::selectable(&self.workspace, &path) => {}
            path => {
                self.environment = path;
                self.session_changed = true;
            }
        }
    }

    fn open_environment(&mut self, target: EnvTarget) {
        let existing = self
            .tabs
            .iter()
            .position(|t| matches!(&t.doc, Doc::Environment(e) if e.target == target));
        if let Some(i) = existing {
            self.active = i;
            return;
        }
        if let Some((name, vars)) = env_vars(&self.workspace, &target) {
            self.push_doc(Doc::Environment(EnvDoc {
                target,
                name,
                saved: vars.clone(),
                draft: vars,
            }));
        }
    }

    fn switch_workspace(&mut self, path: PathBuf) -> Option<Effect> {
        self.workspace_dir = Some(path.clone());
        self.workspace_root = Some(path.clone());
        let docs: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.doc != Doc::Request)
            .map(|t| t.id)
            .collect();
        for id in docs {
            self.discard_tab(id);
        }
        for tab in &mut self.tabs {
            tab.link = None;
            tab.name = None;
            tab.saved = RequestDraft::default();
        }
        self.workspace = Arc::new(Workspace::default());
        self.environment = None;
        self.expanded.clear();
        self.search.clear();
        self.menu = None;
        self.session_changed = true;
        Some(Effect::SwitchWorkspace(path))
    }

    // --- saving -------------------------------------------------------------------------

    fn save(&mut self) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        let tab = &mut self.tabs[self.active];
        match &mut tab.doc {
            Doc::Request => {
                let Some(link) = tab.link.clone() else {
                    let target = ws.collections.first().map(|c| c.path.clone());
                    self.dialog = Some(Dialog::SaveAs {
                        name: tab.title().to_owned(),
                        target,
                        new_collection: NEW_COLLECTION.to_owned(),
                        error: None,
                    });
                    return None;
                };
                let examples = ws
                    .request(&link)
                    .map(|f| f.examples.clone())
                    .unwrap_or_default();
                let contents = collection::write_request(&RequestFile {
                    name: tab.title().to_owned(),
                    request: tab.draft.clone(),
                    examples,
                });
                workspace_effect(
                    "save the request",
                    vec![Op::Write {
                        path: link,
                        contents,
                    }],
                )
            }
            Doc::Environment(env) => {
                let (path, name, action) = match &env.target {
                    EnvTarget::Globals => (GLOBALS_FILE.to_owned(), None, "save the globals"),
                    EnvTarget::Environment(path) => {
                        (path.clone(), Some(env.name.clone()), "save the environment")
                    }
                };
                let file = VariablesFile {
                    name,
                    variables: env.draft.clone(),
                };
                workspace_effect(action, variables_ops(&path, &file))
            }
            Doc::Container(c) => {
                let contents = if c.kind == ItemKind::Collection {
                    collection::write_collection(&CollectionFile {
                        name: c.name.clone(),
                        description: c.draft.description.clone(),
                        auth: c.draft.auth.clone(),
                        variables: c.draft.variables.clone(),
                    })
                } else {
                    collection::write_folder(&FolderFile {
                        name: c.name.clone(),
                        description: c.draft.description.clone(),
                        auth: c.draft.auth.clone(),
                    })
                };
                workspace_effect(
                    format!("save the {} settings", kind_noun(c.kind)),
                    vec![Op::Write {
                        path: settings_path(&c.path, c.kind),
                        contents,
                    }],
                )
            }
            Doc::Example(e) => {
                let example = match e.draft.to_example(&e.request) {
                    Ok(example) => example,
                    Err(message) => {
                        e.error = Some(message);
                        return None;
                    }
                };
                e.error = None;
                let mut file = ws.request(&e.path)?.clone();
                *file.examples.get_mut(e.index)? = example;
                workspace_effect(
                    "save the example",
                    vec![Op::Write {
                        path: e.path.clone(),
                        contents: collection::write_request(&file),
                    }],
                )
            }
        }
    }

    // --- creating -------------------------------------------------------------------------

    fn new_collection(&mut self) -> Option<Effect> {
        let (dir, ops) = self.collection_ops(NEW_COLLECTION);
        self.expanded.insert(dir);
        workspace_effect("create the collection", ops)
    }

    /// Operations creating a collection named after `name`, and its path.
    fn collection_ops(&self, name: &str) -> (String, Vec<Op>) {
        let siblings = children(&self.workspace, COLLECTIONS_DIR);
        let dir_name = names::item_file(name, "", &as_strs(&siblings));
        let dir = join(COLLECTIONS_DIR, &dir_name);
        let ops = vec![
            Op::CreateDir(dir.clone()),
            Op::Write {
                path: join(&dir, COLLECTION_FILE),
                contents: collection::write_collection(&CollectionFile {
                    name: dir_name,
                    ..CollectionFile::default()
                }),
            },
        ];
        (dir, ops)
    }

    fn new_folder(&mut self, parent: &str) -> Option<Effect> {
        let siblings = children(&self.workspace, parent);
        let dir_name = names::item_file(NEW_FOLDER, "", &as_strs(&siblings));
        let dir = join(parent, &dir_name);
        self.expanded.insert(parent.to_owned());
        workspace_effect(
            "create the folder",
            vec![
                Op::CreateDir(dir.clone()),
                Op::Write {
                    path: join(&dir, FOLDER_FILE),
                    contents: collection::write_folder(&FolderFile {
                        name: dir_name,
                        ..FolderFile::default()
                    }),
                },
            ],
        )
    }

    fn new_request(&mut self, parent: &str) -> Option<Effect> {
        let siblings = children(&self.workspace, parent);
        let file_name = names::item_file(NEW_REQUEST, REQUEST_EXT, &as_strs(&siblings));
        let path = join(parent, &file_name);
        let name = file_name
            .strip_suffix(REQUEST_EXT)
            .unwrap_or(&file_name)
            .to_owned();
        let request = RequestDraft {
            auth: AuthDraft {
                kind: AuthKind::Inherit,
                ..AuthDraft::default()
            },
            ..RequestDraft::default()
        };
        let contents = collection::write_request(&RequestFile {
            name: name.clone(),
            request: request.clone(),
            examples: Vec::new(),
        });
        self.expanded.insert(parent.to_owned());
        self.push_doc(Doc::Request);
        let tab = &mut self.tabs[self.active];
        tab.set_draft(request.clone());
        tab.saved = request;
        tab.link = Some(path.clone());
        tab.name = Some(name);
        self.session_changed = true;
        workspace_effect("create the request", vec![Op::Write { path, contents }])
    }

    fn new_environment(&mut self) -> Option<Effect> {
        let siblings = env_file_names(&self.workspace, None);
        let file_name = names::item_file(NEW_ENVIRONMENT, ".toml", &as_strs(&siblings));
        let path = join(ENVIRONMENTS_DIR, &file_name);
        let name = file_name
            .strip_suffix(".toml")
            .unwrap_or(&file_name)
            .to_owned();
        let file = VariablesFile {
            name: Some(name.clone()),
            variables: Vec::new(),
        };
        self.push_doc(Doc::Environment(EnvDoc {
            target: EnvTarget::Environment(path.clone()),
            name,
            saved: Vec::new(),
            draft: Vec::new(),
        }));
        workspace_effect("create the environment", variables_ops(&path, &file))
    }

    // --- dialogs -----------------------------------------------------------------------------

    fn start_rename(&mut self, target: Target) {
        let name = match &target {
            Target::Item(path) => match collection::find(&self.workspace.collections, path) {
                Some(item) if !matches!(item.node, Node::Broken { .. }) => item.name().to_owned(),
                _ => return,
            },
            Target::Environment(path) => match self.workspace.environment(path) {
                Some(entry) if entry.file.is_ok() => entry.name().to_owned(),
                _ => return,
            },
            Target::Example(path, index) => {
                match self
                    .workspace
                    .request(path)
                    .and_then(|f| f.examples.get(*index))
                {
                    Some(example) => example.name.clone(),
                    None => return,
                }
            }
        };
        self.dialog = Some(Dialog::Rename {
            target,
            name,
            error: None,
        });
    }

    fn start_delete(&mut self, target: Target) {
        let name = match &target {
            Target::Item(path) => match collection::find(&self.workspace.collections, path) {
                Some(item) => item.name().to_owned(),
                None => return,
            },
            Target::Environment(path) => match self.workspace.environment(path) {
                Some(entry) => entry.name().to_owned(),
                None => return,
            },
            Target::Example(path, index) => {
                match self
                    .workspace
                    .request(path)
                    .and_then(|f| f.examples.get(*index))
                {
                    Some(example) => example.name.clone(),
                    None => return,
                }
            }
        };
        self.dialog = Some(Dialog::ConfirmDelete { target, name });
    }

    fn confirm_dialog(&mut self) -> Option<Effect> {
        match self.dialog.clone()? {
            Dialog::SaveAs {
                name,
                target,
                new_collection,
                ..
            } => self.save_as(name.trim(), target, &new_collection),
            Dialog::Rename { target, name, .. } => {
                let name = name.trim();
                if name.is_empty() {
                    self.dialog_error(ENTER_A_NAME);
                    return None;
                }
                self.dialog = None;
                self.rename(&target, name)
            }
            Dialog::ConfirmDelete { target, .. } => {
                self.dialog = None;
                self.delete(&target)
            }
        }
    }

    fn dialog_error(&mut self, message: &str) {
        if let Some(Dialog::SaveAs { error, .. } | Dialog::Rename { error, .. }) = &mut self.dialog
        {
            *error = Some(message.to_owned());
        }
    }

    fn save_as(
        &mut self,
        name: &str,
        target: Option<String>,
        new_collection: &str,
    ) -> Option<Effect> {
        if name.is_empty() {
            self.dialog_error(ENTER_A_NAME);
            return None;
        }
        let (dir, mut ops) = match target {
            Some(dir) => (dir, Vec::new()),
            None => {
                let collection_name = if new_collection.trim().is_empty() {
                    NEW_COLLECTION
                } else {
                    new_collection.trim()
                };
                self.collection_ops(collection_name)
            }
        };
        let siblings = children(&self.workspace, &dir);
        let file_name = names::item_file(name, REQUEST_EXT, &as_strs(&siblings));
        let path = join(&dir, &file_name);
        let tab = &mut self.tabs[self.active];
        ops.push(Op::Write {
            path: path.clone(),
            contents: collection::write_request(&RequestFile {
                name: name.to_owned(),
                request: tab.draft.clone(),
                examples: Vec::new(),
            }),
        });
        tab.link = Some(path);
        tab.name = Some(name.to_owned());
        self.dialog = None;
        for ancestor in collection::ancestors(&self.workspace.collections, &dir) {
            self.expanded.insert(ancestor.path.clone());
        }
        self.expanded.insert(dir);
        self.session_changed = true;
        workspace_effect("save the request", ops)
    }

    // --- rename, duplicate, delete, move ----------------------------------------------------------

    /// Point tabs, the selection and expanded folders at `to` instead of `from`.
    fn relink(&mut self, from: &str, to: &str) {
        for tab in &mut self.tabs {
            match &mut tab.doc {
                Doc::Request => {
                    if let Some(new) = tab.link.as_deref().and_then(|l| moved(l, from, to)) {
                        tab.link = Some(new);
                    }
                }
                Doc::Container(c) => {
                    if let Some(new) = moved(&c.path, from, to) {
                        c.path = new;
                    }
                }
                Doc::Example(e) => {
                    if let Some(new) = moved(&e.path, from, to) {
                        e.path = new;
                    }
                }
                Doc::Environment(env) => {
                    if env.target == EnvTarget::Environment(from.to_owned()) {
                        env.target = EnvTarget::Environment(to.to_owned());
                    }
                }
            }
        }
        if self.environment.as_deref() == Some(from) {
            self.environment = Some(to.to_owned());
        }
        self.expanded = std::mem::take(&mut self.expanded)
            .into_iter()
            .map(|p| moved(&p, from, to).unwrap_or(p))
            .collect();
        self.session_changed = true;
    }

    /// The `_order.toml` write for `dir` after its entry `old` became
    /// `new`, if the directory has an order file listing it.
    fn order_rename_op(&self, dir: &str, old: &str, new: &str) -> Option<Op> {
        let order = self.workspace.orders.get(dir)?;
        order.iter().any(|n| n == old).then(|| Op::Write {
            path: join(dir, ORDER_FILE),
            contents: collection::write_order(
                &order
                    .iter()
                    .map(|n| if n == old { new.to_owned() } else { n.clone() })
                    .collect::<Vec<_>>(),
            ),
        })
    }

    fn rename(&mut self, target: &Target, name: &str) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        match target {
            Target::Item(path) => {
                let item = collection::find(&ws.collections, path)?;
                let dir = collection::parent(path);
                let old_file = collection::file_name(path);
                let siblings: Vec<String> = children(&ws, dir)
                    .into_iter()
                    .filter(|n| n != old_file)
                    .collect();
                let (extension, contents, written) = match &item.node {
                    Node::Request(file) => (
                        REQUEST_EXT,
                        collection::write_request(&RequestFile {
                            name: name.to_owned(),
                            ..file.clone()
                        }),
                        None,
                    ),
                    node => ("", settings_with_name(node, name)?, Some(item.kind())),
                };
                let new_file = names::item_file(name, extension, &as_strs(&siblings));
                let new_path = join(dir, &new_file);
                let mut ops = Vec::new();
                if new_path != *path {
                    ops.push(Op::Rename {
                        from: path.clone(),
                        to: new_path.clone(),
                    });
                }
                ops.push(Op::Write {
                    path: match written {
                        Some(kind) => settings_path(&new_path, kind),
                        None => new_path.clone(),
                    },
                    contents,
                });
                ops.extend(self.order_rename_op(dir, old_file, &new_file));
                self.relink(path, &new_path);
                workspace_effect(format!("rename the {}", kind_noun(item.kind())), ops)
            }
            Target::Environment(path) => {
                let entry = ws.environment(path)?;
                let file = entry.file.as_ref().ok()?;
                let siblings = env_file_names(&ws, Some(path));
                let new_file = names::item_file(name, ".toml", &as_strs(&siblings));
                let new_path = join(ENVIRONMENTS_DIR, &new_file);
                let renamed = VariablesFile {
                    name: Some(name.to_owned()),
                    variables: file.variables.clone(),
                };
                let mut ops = Vec::new();
                let (old_local, new_local) = (
                    environment::local_path(path),
                    environment::local_path(&new_path),
                );
                if new_path != *path {
                    ops.push(Op::Rename {
                        from: path.clone(),
                        to: new_path.clone(),
                    });
                    if !old_local.eq_ignore_ascii_case(&new_local) {
                        ops.push(Op::Remove(old_local));
                    }
                }
                ops.extend(variables_ops(&new_path, &renamed));
                self.relink(path, &new_path);
                workspace_effect("rename the environment", ops)
            }
            Target::Example(path, index) => {
                let mut file = ws.request(path)?.clone();
                name.clone_into(&mut file.examples.get_mut(*index)?.name);
                workspace_effect(
                    "rename the example",
                    vec![Op::Write {
                        path: path.clone(),
                        contents: collection::write_request(&file),
                    }],
                )
            }
        }
    }

    fn duplicate(&mut self, target: &Target) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        match target {
            Target::Item(path) => {
                let item = collection::find(&ws.collections, path)?;
                let dir = collection::parent(path);
                let siblings = children(&ws, dir);
                let name = format!("{} Copy", item.name());
                match &item.node {
                    Node::Request(file) => {
                        let new_file = names::item_file(&name, REQUEST_EXT, &as_strs(&siblings));
                        workspace_effect(
                            "duplicate the request",
                            vec![Op::Write {
                                path: join(dir, &new_file),
                                contents: collection::write_request(&RequestFile {
                                    name,
                                    ..file.clone()
                                }),
                            }],
                        )
                    }
                    Node::Broken { .. } => None,
                    node => {
                        let new_dir = join(dir, &names::item_file(&name, "", &as_strs(&siblings)));
                        let contents = settings_with_name(node, &name)?;
                        workspace_effect(
                            format!("duplicate the {}", kind_noun(item.kind())),
                            vec![
                                Op::Copy {
                                    from: path.clone(),
                                    to: new_dir.clone(),
                                },
                                Op::Write {
                                    path: settings_path(&new_dir, item.kind()),
                                    contents,
                                },
                            ],
                        )
                    }
                }
            }
            Target::Environment(path) => {
                let entry = ws.environment(path)?;
                let file = entry.file.as_ref().ok()?;
                let name = format!("{} Copy", entry.name());
                let siblings = env_file_names(&ws, None);
                let new_path = join(
                    ENVIRONMENTS_DIR,
                    &names::item_file(&name, ".toml", &as_strs(&siblings)),
                );
                let copy = VariablesFile {
                    name: Some(name),
                    variables: file.variables.clone(),
                };
                workspace_effect("duplicate the environment", variables_ops(&new_path, &copy))
            }
            Target::Example(path, index) => {
                let mut file = ws.request(path)?.clone();
                let mut copy = file.examples.get(*index)?.clone();
                copy.name = unique_example_name(&file, &format!("{} Copy", copy.name));
                file.examples.push(copy);
                workspace_effect(
                    "duplicate the example",
                    vec![Op::Write {
                        path: path.clone(),
                        contents: collection::write_request(&file),
                    }],
                )
            }
        }
    }

    fn delete(&mut self, target: &Target) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        match target {
            Target::Item(path) => {
                let item = collection::find(&ws.collections, path)?;
                workspace_effect(
                    format!("delete the {}", kind_noun(item.kind())),
                    vec![Op::Remove(path.clone())],
                )
            }
            Target::Environment(path) => workspace_effect(
                "delete the environment",
                vec![
                    Op::Remove(path.clone()),
                    Op::Remove(environment::local_path(path)),
                ],
            ),
            Target::Example(path, index) => {
                let mut file = ws.request(path)?.clone();
                if *index >= file.examples.len() {
                    return None;
                }
                file.examples.remove(*index);
                // Example tabs of this request follow the shift.
                let mut closing = Vec::new();
                for tab in &mut self.tabs {
                    if let Doc::Example(e) = &mut tab.doc
                        && e.path == *path
                    {
                        if e.index == *index {
                            closing.push(tab.id);
                        } else if e.index > *index {
                            e.index -= 1;
                        }
                    }
                }
                for id in closing {
                    self.discard_tab(id);
                }
                workspace_effect(
                    "delete the example",
                    vec![Op::Write {
                        path: path.clone(),
                        contents: collection::write_request(&file),
                    }],
                )
            }
        }
    }

    fn drop_item(&mut self) -> Option<Effect> {
        let from = self.tree_drag.take()?;
        match self.tree_target.take() {
            Some(target) if target != from => self.move_item(&from, &target),
            _ => {
                self.open_item(&from);
                None
            }
        }
    }

    fn move_item(&mut self, from: &str, target: &str) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        let source = collection::find(&ws.collections, from)?;
        if source.kind() == ItemKind::Collection {
            return None;
        }
        let target_item = collection::find(&ws.collections, target)?;
        let (dest, before) = if target_item.kind() == ItemKind::Request {
            (
                collection::parent(target).to_owned(),
                Some(collection::file_name(target)),
            )
        } else {
            (target.to_owned(), None)
        };
        if within(&dest, from) {
            return None;
        }
        let file = collection::file_name(from);
        let same_dir = collection::parent(from) == dest;
        let mut order: Vec<String> = children(&ws, &dest)
            .into_iter()
            .filter(|n| !(same_dir && n == file))
            .collect();
        let new_file = if same_dir {
            file.to_owned()
        } else {
            let (stem, extension) = match file.strip_suffix(REQUEST_EXT) {
                Some(stem) if source.kind() == ItemKind::Request => (stem, REQUEST_EXT),
                _ => (file, ""),
            };
            names::unique(stem, extension, &as_strs(&order))
        };
        let position = before
            .and_then(|b| order.iter().position(|n| n == b))
            .unwrap_or(order.len());
        order.insert(position, new_file.clone());
        let new_path = join(&dest, &new_file);
        let mut ops = Vec::new();
        if !same_dir {
            ops.push(Op::Rename {
                from: from.to_owned(),
                to: new_path.clone(),
            });
        }
        ops.push(Op::Write {
            path: join(&dest, ORDER_FILE),
            contents: collection::write_order(&order),
        });
        self.relink(from, &new_path);
        self.expanded.insert(dest);
        workspace_effect(format!("move the {}", kind_noun(source.kind())), ops)
    }
}

/// Whether two variable tables show the same rows, ignoring where values
/// are stored.
fn same_rows(a: &[Variable], b: &[Variable]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            (&x.key, &x.value, x.enabled, x.secret) == (&y.key, &y.value, y.enabled, y.secret)
        })
}

/// Writes for a variables file and its local file, removing a local file
/// that is no longer needed.
fn variables_ops(path: &str, file: &VariablesFile) -> Vec<Op> {
    let (shared, local) = environment::write(file);
    let local_path = environment::local_path(path);
    vec![
        Op::Write {
            path: path.to_owned(),
            contents: shared,
        },
        match local {
            Some(contents) => Op::Write {
                path: local_path,
                contents,
            },
            None => Op::Remove(local_path),
        },
    ]
}

/// `name`, or `name (2)`, `name (3)`, … if an example already has it.
pub(crate) fn unique_example_name(file: &RequestFile, name: &str) -> String {
    let taken = |candidate: &str| file.examples.iter().any(|e| e.name == candidate);
    let mut candidate = name.to_owned();
    let mut n = 2u64;
    while taken(&candidate) {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    candidate
}
