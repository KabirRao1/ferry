//! Workspace flows as message sequences against an in-memory filesystem:
//! F04-AC6, F07-AC7, F08-AC2..AC11, F09-AC4..AC15, F14-AC6.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use ferry_app::{
    App, AuthKind, ContainerSection, Dialog, Doc, Edit, Effect, EnvTarget, FixedAmbient, Item,
    ItemKind, KeyValue, MemFs, Message, Method, Node, Phase, RequestDraft, RowEdit, ScopeKind,
    ScriptPhase, Session, SessionTab, Table, Target, VarEdit, VarState, Variable, Workspace,
    WorkspaceStore, collection,
};

fn root() -> PathBuf {
    PathBuf::from("/ws")
}

/// The app wired to an in-memory workspace: workspace effects are applied
/// and reloaded as the UI would.
struct H {
    app: App,
    fs: MemFs,
    store: WorkspaceStore<MemFs>,
}

impl H {
    fn new() -> Self {
        Self::with(|_| {})
    }

    fn with(setup: impl FnOnce(&MemFs)) -> Self {
        Self::restore(Session::default(), setup)
    }

    fn restore(session: Session, setup: impl FnOnce(&MemFs)) -> Self {
        let fs = MemFs::new();
        let store = WorkspaceStore::new(fs.clone(), &root());
        store.init().unwrap();
        setup(&fs);
        let app = App::restore(session)
            .with_ambient(Box::new(FixedAmbient::new(
                UNIX_EPOCH + Duration::from_secs(1_792_264_040),
                0,
            )))
            .with_workspace(store.load());
        Self { app, fs, store }
    }

    /// Send a message; run workspace effects; return any other effect.
    fn update(&mut self, message: Message) -> Option<Effect> {
        match self.app.update(message)? {
            Effect::Workspace { action, ops } => {
                let error = self.store.apply(&ops).err().map(|e| e.to_string());
                let workspace = Arc::new(self.store.load());
                self.update(Message::WorkspaceApplied {
                    action,
                    error,
                    workspace,
                })
            }
            other => Some(other),
        }
    }

    fn edit(&mut self, e: Edit) {
        assert!(self.update(Message::Edit(e)).is_none());
    }

    /// Reload after another program changed the files.
    fn reload(&mut self) {
        let workspace = Arc::new(self.store.load());
        assert!(self.update(Message::WorkspaceLoaded(workspace)).is_none());
    }

    fn text(&self, rel: &str) -> Option<String> {
        self.fs.text(root().join(rel))
    }

    fn put(&self, rel: &str, text: &str) {
        self.fs.put(root().join(rel), text);
    }

    fn exists(&self, rel: &str) -> bool {
        self.text(rel).is_some()
    }

    fn item(&self, path: &str) -> Item {
        collection::find(&self.app.workspace().collections, path)
            .cloned()
            .unwrap_or_else(|| panic!("{path} not in the tree"))
    }

    fn names(&self, path: &str) -> Vec<String> {
        let items = if path.is_empty() {
            self.app.workspace().collections.clone()
        } else {
            self.item(path).items
        };
        items.iter().map(|i| i.name().to_owned()).collect()
    }

    /// The request in the next send effect.
    fn sent(&mut self) -> ferry_app::Request {
        match self.update(Message::SendPressed) {
            Some(Effect::Send { request, .. }) => request,
            other => panic!(
                "expected a send effect, got {other:?}, phase {:?}",
                self.app.tab().phase
            ),
        }
    }

    fn failed(&mut self) -> String {
        assert!(self.update(Message::SendPressed).is_none());
        match &self.app.tab().phase {
            Phase::Failed(text) => text.clone(),
            other => panic!("expected failure, got {other:?}"),
        }
    }

    fn header(request: &ferry_app::Request, name: &str) -> Option<String> {
        request
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.clone())
    }
}

fn request_file(name: &str, url: &str) -> String {
    collection::write_request(&collection::RequestFile {
        name: name.into(),
        request: RequestDraft {
            url: url.into(),
            ..RequestDraft::default()
        },
        examples: vec![],
    })
}

/// collections/api: "API" with bearer auth and variable base.
///   users/ (folder.toml "Users")
///     list.toml "List users" GET {{base}}/users, inherit
///   health.toml "Health"
fn sample(fs: &MemFs) {
    let put = |rel: &str, text: &str| fs.put(root().join(rel), text);
    put(
        "collections/api/collection.toml",
        "name = \"API\"\n[auth]\nkind = \"bearer\"\ntoken = \"{{token}}\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://api.test\"\n",
    );
    put("collections/api/users/folder.toml", "name = \"Users\"\n");
    put(
        "collections/api/users/list.toml",
        "name = \"List users\"\nurl = \"{{base}}/users\"\n[auth]\nkind = \"inherit\"\n",
    );
    put(
        "collections/api/health.toml",
        &request_file("Health", "{{base}}/health"),
    );
    put(
        "environments/dev.toml",
        "name = \"Dev\"\n[[variables]]\nkey = \"token\"\nvalue = \"dev-token\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://dev.test\"\n",
    );
    put(
        "globals.toml",
        "[[variables]]\nkey = \"token\"\nvalue = \"global-token\"\n[[variables]]\nkey = \"g\"\nvalue = \"global\"\n",
    );
}

const LIST: &str = "collections/api/users/list.toml";
const HEALTH: &str = "collections/api/health.toml";

// --- F09-AC4 opening ------------------------------------------------------------

#[test]
fn f09_ac4_clicking_a_request_opens_a_linked_tab_once() {
    let mut h = H::with(sample);
    assert!(h.update(Message::OpenItem(LIST.into())).is_none());
    assert_eq!(h.app.tabs().len(), 2);
    let tab = h.app.tab();
    assert_eq!(tab.link.as_deref(), Some(LIST));
    assert_eq!(tab.title(), "List users");
    assert_eq!(tab.draft.url, "{{base}}/users");
    assert!(!tab.is_dirty());
    let id = tab.id;
    h.update(Message::NewTab);
    h.update(Message::OpenItem(LIST.into()));
    assert_eq!(h.app.tab().id, id, "the linked tab is activated");
    assert_eq!(h.app.tabs().len(), 3);
}

#[test]
fn f09_ac12_broken_requests_do_not_open() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(root().join("collections/api/bad.toml"), "name = ");
    });
    h.update(Message::OpenItem("collections/api/bad.toml".into()));
    assert_eq!(h.app.tabs().len(), 1);
    assert!(h.app.tab().link.is_none());
}

#[test]
fn f09_ac3_toggling_folders() {
    let mut h = H::with(sample);
    assert!(!h.app.is_expanded("collections/api"));
    h.update(Message::ToggleExpanded("collections/api".into()));
    assert!(h.app.is_expanded("collections/api"));
    h.update(Message::ToggleExpanded("collections/api".into()));
    assert!(!h.app.is_expanded("collections/api"));
}

// --- F09-AC5 saving ----------------------------------------------------------------

#[test]
fn f09_ac5_saving_a_linked_tab_rewrites_its_file() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::MethodSelected(Method::Head));
    assert!(h.app.tab().is_dirty());
    assert!(h.update(Message::Save).is_none());
    let text = h.text(HEALTH).unwrap();
    assert!(text.contains("method = \"HEAD\""), "{text}");
    assert!(text.contains("name = \"Health\""), "{text}");
    assert!(!h.app.tab().is_dirty());
    assert!(h.app.status().is_none());
}

#[test]
fn f09_ac5_saving_keeps_the_files_examples() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join(HEALTH),
            "name = \"Health\"\n[[examples]]\nname = \"Up\"\nstatus = 200\nbody = \"ok\"\n",
        );
    });
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::Url("https://x.test".into()));
    h.update(Message::Save);
    let text = h.text(HEALTH).unwrap();
    assert!(text.contains("name = \"Up\""), "{text}");
}

#[test]
fn f09_ac5_unlinked_tabs_ask_where_to_save() {
    let mut h = H::with(sample);
    h.edit(Edit::Url("https://new.test/thing".into()));
    h.update(Message::Save);
    match h.app.dialog() {
        Some(Dialog::SaveAs { name, target, .. }) => {
            assert_eq!(name, "https://new.test/thing");
            assert_eq!(target.as_deref(), Some("collections/api"));
        }
        other => panic!("{other:?}"),
    }
    h.update(Message::DialogInput("Thing".into()));
    h.update(Message::DialogPick("collections/api/users".into()));
    h.update(Message::DialogConfirm);
    assert!(h.app.dialog().is_none());
    let path = "collections/api/users/Thing.toml";
    let text = h.text(path).unwrap();
    assert!(text.contains("url = \"https://new.test/thing\""));
    assert_eq!(h.app.tab().link.as_deref(), Some(path));
    assert_eq!(h.app.tab().title(), "Thing");
    assert!(!h.app.tab().is_dirty());
}

#[test]
fn f09_ac5_empty_names_are_refused_and_cancel_closes() {
    let mut h = H::with(sample);
    h.update(Message::Save);
    h.update(Message::DialogInput("   ".into()));
    h.update(Message::DialogConfirm);
    match h.app.dialog() {
        Some(Dialog::SaveAs { error, .. }) => assert_eq!(error.as_deref(), Some("Enter a name")),
        other => panic!("{other:?}"),
    }
    h.update(Message::DialogCancel);
    assert!(h.app.dialog().is_none());
    assert!(h.app.tab().link.is_none());
}

#[test]
fn f09_ac5_without_collections_the_dialog_creates_one() {
    let mut h = H::new();
    h.edit(Edit::Url("https://a.test".into()));
    h.update(Message::Save);
    match h.app.dialog() {
        Some(Dialog::SaveAs {
            target,
            new_collection,
            ..
        }) => {
            assert_eq!(*target, None);
            assert_eq!(new_collection, "New Collection");
        }
        other => panic!("{other:?}"),
    }
    h.update(Message::DialogInput("First".into()));
    h.update(Message::DialogConfirm);
    assert_eq!(h.names(""), ["New Collection"]);
    assert!(h.exists("collections/New Collection/collection.toml"));
    assert_eq!(
        h.app.tab().link.as_deref(),
        Some("collections/New Collection/First.toml")
    );
}

#[test]
fn f09_ac5_save_names_avoid_collisions() {
    let mut h = H::with(sample);
    h.update(Message::Save);
    h.update(Message::DialogInput("health".into()));
    h.update(Message::DialogConfirm);
    assert_eq!(
        h.app.tab().link.as_deref(),
        Some("collections/api/health (2).toml")
    );
}

// --- F09-AC6 creating -----------------------------------------------------------------

#[test]
fn f09_ac6_new_collections_folders_and_requests() {
    let mut h = H::new();
    h.update(Message::NewCollection);
    h.update(Message::NewCollection);
    assert_eq!(h.names(""), ["New Collection", "New Collection (2)"]);
    let col = "collections/New Collection";
    h.update(Message::NewFolder(col.into()));
    assert_eq!(h.names(col), ["New Folder"]);
    assert!(h.app.is_expanded(col));
    h.update(Message::NewRequest(format!("{col}/New Folder")));
    let path = "collections/New Collection/New Folder/New Request.toml";
    assert!(h.exists(path));
    let tab = h.app.tab();
    assert_eq!(tab.link.as_deref(), Some(path));
    assert_eq!(tab.title(), "New Request");
    assert_eq!(tab.draft.auth.kind, AuthKind::Inherit);
    assert!(!tab.is_dirty());
}

// --- F09-AC7 rename, duplicate, delete --------------------------------------------------------

fn rename(h: &mut H, target: Target, name: &str) {
    h.update(Message::Rename(target));
    h.update(Message::DialogInput(name.into()));
    h.update(Message::DialogConfirm);
}

#[test]
fn f09_ac7_renaming_a_request_moves_its_file_and_tab() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.update(Message::Rename(Target::Item(HEALTH.into())));
    match h.app.dialog() {
        Some(Dialog::Rename { name, .. }) => assert_eq!(name, "Health"),
        other => panic!("{other:?}"),
    }
    h.update(Message::DialogInput("Status check".into()));
    h.update(Message::DialogConfirm);
    let new = "collections/api/Status check.toml";
    assert!(!h.exists(HEALTH));
    assert!(h.text(new).unwrap().contains("name = \"Status check\""));
    assert_eq!(h.app.tab().link.as_deref(), Some(new));
    assert_eq!(h.app.tab().title(), "Status check");
    assert!(!h.app.tab().is_dirty());
}

#[test]
fn f09_ac7_renaming_a_folder_moves_tabs_inside_it() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(LIST.into()));
    h.edit(Edit::Url("{{base}}/people".into()));
    rename(
        &mut h,
        Target::Item("collections/api/users".into()),
        "People",
    );
    let new = "collections/api/People/list.toml";
    assert!(h.exists(new));
    assert!(
        h.text("collections/api/People/folder.toml")
            .unwrap()
            .contains("name = \"People\"")
    );
    assert_eq!(h.app.tab().link.as_deref(), Some(new));
    assert!(h.app.tab().is_dirty(), "unsaved edits survive the rename");
    assert_eq!(h.app.tab().draft.url, "{{base}}/people");
}

#[test]
fn f09_ac7_renaming_refuses_empty_names() {
    let mut h = H::with(sample);
    rename(&mut h, Target::Item(HEALTH.into()), "");
    match h.app.dialog() {
        Some(Dialog::Rename { error, .. }) => assert_eq!(error.as_deref(), Some("Enter a name")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f09_ac7_renaming_keeps_a_custom_order() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("collections/api/_order.toml"),
            "order = [\"health.toml\", \"users\"]\n",
        );
    });
    assert_eq!(h.names("collections/api"), ["Health", "Users"]);
    rename(&mut h, Target::Item(HEALTH.into()), "Zulu");
    assert_eq!(h.names("collections/api"), ["Zulu", "Users"]);
}

#[test]
fn f09_ac7_duplicating_requests_and_folders() {
    let mut h = H::with(sample);
    h.update(Message::Duplicate(Target::Item(HEALTH.into())));
    let copy = h.text("collections/api/Health Copy.toml").unwrap();
    assert!(copy.contains("name = \"Health Copy\""));
    assert!(copy.contains("{{base}}/health"));
    h.update(Message::Duplicate(Target::Item(
        "collections/api/users".into(),
    )));
    assert!(h.exists("collections/api/Users Copy/list.toml"));
    assert!(
        h.text("collections/api/Users Copy/folder.toml")
            .unwrap()
            .contains("name = \"Users Copy\"")
    );
    h.update(Message::Duplicate(Target::Item("collections/api".into())));
    assert_eq!(h.names(""), ["API", "API Copy"]);
}

#[test]
fn f09_ac7_deleting_asks_first_and_unlinks_tabs() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.update(Message::Delete(Target::Item(HEALTH.into())));
    match h.app.dialog() {
        Some(Dialog::ConfirmDelete { name, .. }) => assert_eq!(name, "Health"),
        other => panic!("{other:?}"),
    }
    assert!(h.exists(HEALTH), "nothing happens before confirming");
    h.update(Message::DialogConfirm);
    assert!(!h.exists(HEALTH));
    let tab = h.app.tab();
    assert_eq!(tab.link, None);
    assert_eq!(tab.title(), "{{base}}/health");
    assert!(tab.is_dirty(), "the request is now unsaved");
    // Cancelling keeps things.
    h.update(Message::Delete(Target::Item("collections/api".into())));
    h.update(Message::DialogCancel);
    assert_eq!(h.names(""), ["API"]);
}

// --- F09-AC9 drag and drop -----------------------------------------------------------

fn drag(h: &mut H, from: &str, to: &str) {
    h.update(Message::ItemPressed(from.into()));
    h.update(Message::ItemEntered(to.into()));
    h.update(Message::ItemReleased);
}

#[test]
fn f09_ac9_dropping_on_a_request_moves_before_it() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    drag(&mut h, HEALTH, LIST);
    let moved = "collections/api/users/health.toml";
    assert!(h.exists(moved));
    assert!(!h.exists(HEALTH));
    assert_eq!(h.names("collections/api/users"), ["Health", "List users"]);
    assert_eq!(h.app.tab().link.as_deref(), Some(moved));
    assert!(
        h.text("collections/api/users/_order.toml")
            .unwrap()
            .contains("\"health.toml\", \"list.toml\"")
    );
}

#[test]
fn f09_ac9_dropping_on_a_folder_appends() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("collections/api/users/a.toml"),
            request_file("A", "x"),
        );
    });
    drag(&mut h, HEALTH, "collections/api/users");
    assert_eq!(
        h.names("collections/api/users"),
        ["A", "List users", "Health"]
    );
}

#[test]
fn f09_ac9_reordering_within_a_folder() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("collections/api/users/a.toml"),
            request_file("A", "x"),
        );
    });
    drag(&mut h, LIST, "collections/api/users/a.toml");
    assert_eq!(h.names("collections/api/users"), ["List users", "A"]);
    assert!(h.exists(LIST), "same folder: the file stays put");
}

#[test]
fn f09_ac9_folders_cannot_move_into_themselves_and_collections_do_not_move() {
    let mut h = H::with(sample);
    let before = h.fs.files();
    drag(&mut h, "collections/api/users", LIST);
    drag(&mut h, "collections/api", HEALTH);
    assert_eq!(h.fs.files(), before);
    assert_eq!(h.app.tree_drag(), (None, None));
}

#[test]
fn f09_ac9_releasing_on_the_pressed_item_opens_it() {
    let mut h = H::with(sample);
    h.update(Message::ItemPressed(HEALTH.into()));
    h.update(Message::ItemReleased);
    assert_eq!(h.app.tab().link.as_deref(), Some(HEALTH));
}

// --- F09-AC10 outside changes ---------------------------------------------------------

#[test]
fn f09_ac10_clean_tabs_reload_and_dirty_tabs_keep_edits() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    let clean = h.app.tab().id;
    h.update(Message::OpenItem(LIST.into()));
    h.edit(Edit::Url("mine".into()));
    h.put(HEALTH, &request_file("Health v2", "https://v2.test"));
    h.put(LIST, &request_file("List v2", "https://v2.test/users"));
    h.reload();
    let tab = h.app.tabs().iter().find(|t| t.id == clean).unwrap();
    assert_eq!(tab.draft.url, "https://v2.test");
    assert_eq!(tab.title(), "Health v2");
    assert!(!tab.is_dirty());
    let dirty = h.app.tab();
    assert_eq!(dirty.draft.url, "mine");
    assert_eq!(dirty.title(), "List v2");
    assert!(dirty.is_dirty());
}

#[test]
fn f09_ac10_deleted_or_broken_files_unlink_tabs() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.update(Message::OpenItem(LIST.into()));
    // Another program deletes one file and breaks the other.
    h.store
        .apply(&[ferry_app::Op::Remove(HEALTH.into())])
        .unwrap();
    h.put(LIST, "name = ");
    h.reload();
    assert!(h.app.tabs().iter().all(|t| t.link.is_none()));
    assert_eq!(
        h.app.tab().draft.url,
        "{{base}}/users",
        "the request stays open"
    );
}

#[test]
fn f09_ac10_own_saves_do_not_disturb_other_edits() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::Url("one".into()));
    h.update(Message::Save);
    h.edit(Edit::Url("two".into()));
    h.reload();
    assert_eq!(h.app.tab().draft.url, "two");
    assert!(h.app.tab().is_dirty());
}

// --- F09-AC11 collection and folder settings ---------------------------------------------

#[test]
fn f09_ac11_collection_settings_tab() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem("collections/api".into()));
    assert!(h.app.is_expanded("collections/api"));
    match &h.app.tab().doc {
        Doc::Container(c) => {
            assert_eq!(c.kind, ItemKind::Collection);
            assert_eq!(c.draft.auth.kind, AuthKind::Bearer);
            assert_eq!(c.draft.variables[0].key, "base");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.app.tab().title(), "API");
    h.edit(Edit::ContainerSection(ContainerSection::Variables));
    h.edit(Edit::Description("All the endpoints.".into()));
    h.edit(Edit::Var(0, VarEdit::Value("https://prod.test".into())));
    h.edit(Edit::Var(1, VarEdit::Key("extra".into())));
    h.edit(Edit::AuthKind(AuthKind::Basic));
    h.edit(Edit::Username("u".into()));
    assert!(h.app.tab().is_dirty());
    h.update(Message::Save);
    let text = h.text("collections/api/collection.toml").unwrap();
    assert!(
        text.contains("description = \"All the endpoints.\""),
        "{text}"
    );
    assert!(text.contains("https://prod.test"));
    assert!(text.contains("key = \"extra\""));
    assert!(text.contains("kind = \"basic\""));
    assert!(!h.app.tab().is_dirty());
    // Opening it again activates the same tab.
    let tabs = h.app.tabs().len();
    h.update(Message::OpenItem("collections/api".into()));
    assert_eq!(h.app.tabs().len(), tabs);
}

#[test]
fn f09_ac11_folder_settings_create_folder_toml() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("collections/api/plain/r.toml"),
            request_file("R", "x"),
        );
    });
    h.update(Message::OpenItem("collections/api/plain".into()));
    match &h.app.tab().doc {
        Doc::Container(c) => assert_eq!(c.draft.auth.kind, AuthKind::Inherit),
        other => panic!("{other:?}"),
    }
    h.edit(Edit::Description("d".into()));
    h.update(Message::Save);
    let text = h.text("collections/api/plain/folder.toml").unwrap();
    assert!(text.contains("name = \"plain\""));
    assert!(text.contains("description = \"d\""));
}

#[test]
fn f09_ac11_collection_variables_resolve_in_requests() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    let request = h.sent();
    assert_eq!(request.url.as_str(), "https://api.test/health");
}

// --- F09-AC13 search ------------------------------------------------------------------

#[test]
fn f09_ac13_search_lists_matching_saved_requests() {
    let mut h = H::with(sample);
    assert!(h.app.search_hits().is_empty());
    h.update(Message::Search("USERS".into()));
    assert_eq!(h.app.search(), "USERS");
    let hits = h.app.search_hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, LIST);
    assert_eq!(hits[0].location, ["API", "Users"]);
    h.update(Message::Search(String::new()));
    assert!(h.app.search_hits().is_empty());
}

// --- F09-AC14 switching workspaces ------------------------------------------------------

#[test]
fn f09_ac14_switching_workspaces() {
    let mut h = H::with(sample);
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    h.update(Message::OpenItem(HEALTH.into()));
    h.update(Message::OpenItem("collections/api".into()));
    assert!(matches!(
        h.update(Message::OpenWorkspace),
        Some(Effect::PickWorkspace)
    ));
    assert!(h.update(Message::WorkspacePicked(None)).is_none());
    assert_eq!(h.app.tab().title(), "API", "cancelling changes nothing");
    let effect = h.update(Message::WorkspacePicked(Some(PathBuf::from("/other"))));
    assert!(matches!(effect, Some(Effect::SwitchWorkspace(p)) if p == Path::new("/other")));
    assert_eq!(h.app.workspace_dir(), Some(Path::new("/other")));
    assert_eq!(h.app.workspace_root(), Some(Path::new("/other")));
    assert!(h.app.tabs().iter().all(|t| t.link.is_none()));
    assert!(h.app.tabs().iter().all(|t| t.doc == Doc::Request));
    assert_eq!(h.app.environment(), None);
    assert_eq!(h.app.workspace(), &Workspace::default());
    assert_eq!(
        h.app.session().workspace.as_deref(),
        Some(Path::new("/other"))
    );
}

// --- F09-AC15 failures ----------------------------------------------------------------

#[test]
fn f09_ac15_failed_operations_show_why() {
    let mut h = H::with(sample);
    h.fs.fail(
        root().join("collections/api"),
        io::ErrorKind::PermissionDenied,
    );
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::Url("changed".into()));
    h.update(Message::Save);
    let status = h.app.status().unwrap();
    assert!(
        status.starts_with("Could not save the request: collections/api/health.toml: "),
        "{status}"
    );
    assert!(h.app.tab().is_dirty(), "still unsaved");
    h.update(Message::DismissStatus);
    assert!(h.app.status().is_none());
    h.update(Message::NewFolder("collections/api".into()));
    assert!(
        h.app
            .status()
            .unwrap()
            .starts_with("Could not create the folder: ")
    );
}

// --- F04-AC6 inherited auth ----------------------------------------------------------------

#[test]
fn f04_ac6_inherit_uses_the_collections_auth_with_variables() {
    let mut h = H::with(sample);
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    h.update(Message::OpenItem(LIST.into()));
    let request = h.sent();
    assert_eq!(
        H::header(&request, "authorization").as_deref(),
        Some("Bearer dev-token")
    );
    let (auth, source) = h.app.inherited_auth(h.app.tab()).unwrap();
    assert_eq!(auth.kind, AuthKind::Bearer);
    assert_eq!(source.name, "API");
}

#[test]
fn f04_ac6_unsaved_requests_inherit_nothing() {
    let mut h = H::with(sample);
    h.edit(Edit::Url("https://a.test".into()));
    h.edit(Edit::AuthKind(AuthKind::Inherit));
    let request = h.sent();
    assert_eq!(H::header(&request, "authorization"), None);
    assert!(h.app.inherited_auth(h.app.tab()).is_none());
}

// --- F08 variables ---------------------------------------------------------------------

#[test]
fn f08_ac2_environment_beats_collection_beats_globals() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::Url("{{base}}/{{g}}?t={{token}}".into()));
    // No environment: collection base, global token.
    let request = h.sent();
    assert_eq!(
        request.url.as_str(),
        "https://api.test/global?t=global-token"
    );
    h.update(Message::CancelPressed);
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    let request = h.sent();
    assert_eq!(request.url.as_str(), "https://dev.test/global?t=dev-token");
}

#[test]
fn f08_ac5_unresolved_variables_block_sending() {
    let mut h = H::with(sample);
    h.edit(Edit::Url("{{nope}}/x/{{also}}/{{nope}}".into()));
    assert_eq!(h.failed(), "Unresolved variables {{nope}}, {{also}}");
    h.edit(Edit::Url("https://a.test/{{nope}}".into()));
    assert_eq!(h.failed(), "Unresolved variable {{nope}}");
}

#[test]
fn f08_ac4_cycles_block_sending() {
    let mut h = H::with(|fs| {
        fs.put(
            root().join("globals.toml"),
            "[[variables]]\nkey = \"a\"\nvalue = \"{{b}}\"\n[[variables]]\nkey = \"b\"\nvalue = \"{{a}}\"\n",
        );
    });
    h.edit(Edit::Url("https://x/{{a}}".into()));
    assert_eq!(h.failed(), "Variable {{a}} refers to itself");
}

#[test]
fn f08_ac7_dynamic_variables_use_the_injected_clock() {
    let mut h = H::new();
    h.edit(Edit::Url("https://a.test/{{$timestamp}}".into()));
    assert_eq!(h.sent().url.as_str(), "https://a.test/1792264040");
}

#[test]
fn f08_ac6_variable_strip() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("environments/dev.toml"),
            "name = \"Dev\"\n[[variables]]\nkey = \"token\"\nvalue = \"\"\nsecret = true\n",
        );
        fs.put(
            root().join("environments/dev.local.toml"),
            "[[variables]]\nkey = \"token\"\nvalue = \"hush\"\n",
        );
    });
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    h.update(Message::OpenItem(LIST.into()));
    h.edit(Edit::Url("{{base}}/{{missing}}/{{$guid}}".into()));
    h.edit(Edit::AuthKind(AuthKind::Bearer));
    h.edit(Edit::Token("{{token}}".into()));
    let strip = h.app.variable_strip();
    let names: Vec<&str> = strip.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["base", "missing", "$guid", "token"]);
    assert_eq!(
        strip[0].state,
        VarState::Defined {
            value: "https://api.test".into(),
            scope: ScopeKind::Collection,
            secret: false
        }
    );
    assert_eq!(strip[1].state, VarState::Missing);
    assert_eq!(strip[2].state, VarState::Dynamic);
    assert_eq!(
        strip[3].state,
        VarState::Defined {
            value: "hush".into(),
            scope: ScopeKind::Environment,
            secret: true
        }
    );
    h.edit(Edit::Url("https://plain.test".into()));
    h.edit(Edit::AuthKind(AuthKind::None));
    assert!(h.app.variable_strip().is_empty());
}

// --- F08-AC8 environments -----------------------------------------------------------------

fn env_doc(h: &H) -> ferry_app::EnvDoc {
    match &h.app.tab().doc {
        Doc::Environment(env) => env.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f08_ac8_new_environment_edit_and_save() {
    let mut h = H::new();
    h.update(Message::NewEnvironment);
    let path = "environments/New Environment.toml";
    assert!(h.exists(path));
    assert_eq!(h.app.tab().title(), "New Environment");
    assert_eq!(env_doc(&h).target, EnvTarget::Environment(path.into()));
    h.edit(Edit::Var(0, VarEdit::Key("base".into())));
    h.edit(Edit::Var(0, VarEdit::Value("https://x.test".into())));
    h.edit(Edit::Var(1, VarEdit::Key("token".into())));
    h.edit(Edit::Var(1, VarEdit::Value("s3cret".into())));
    h.edit(Edit::Var(1, VarEdit::Secret(true)));
    h.edit(Edit::Var(2, VarEdit::Value("".into())));
    assert_eq!(
        env_doc(&h).draft.len(),
        2,
        "empty edits of the blank row add nothing"
    );
    assert!(h.app.tab().is_dirty());
    h.update(Message::Save);
    assert!(!h.app.tab().is_dirty());
    let shared = h.text(path).unwrap();
    assert!(shared.contains("https://x.test"));
    assert!(!shared.contains("s3cret"), "{shared}");
    assert!(
        h.text("environments/New Environment.local.toml")
            .unwrap()
            .contains("s3cret")
    );
    h.edit(Edit::Var(0, VarEdit::Enabled(false)));
    h.edit(Edit::Var(1, VarEdit::Remove));
    h.update(Message::Save);
    assert!(!h.exists("environments/New Environment.local.toml"));
    assert!(h.text(path).unwrap().contains("enabled = false"));
}

#[test]
fn f08_ac8_globals_open_in_a_tab() {
    let mut h = H::with(sample);
    h.update(Message::OpenEnvironment(EnvTarget::Globals));
    let doc = env_doc(&h);
    assert_eq!(doc.name, "Globals");
    assert_eq!(doc.draft[1].key, "g");
    h.edit(Edit::Var(1, VarEdit::Value("changed".into())));
    h.update(Message::Save);
    assert!(h.text("globals.toml").unwrap().contains("changed"));
    let tabs = h.app.tabs().len();
    h.update(Message::OpenEnvironment(EnvTarget::Globals));
    assert_eq!(h.app.tabs().len(), tabs);
}

#[test]
fn f08_ac8_rename_duplicate_delete_environments() {
    let mut h = H::with(sample);
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    h.update(Message::OpenEnvironment(EnvTarget::Environment(
        "environments/dev.toml".into(),
    )));
    rename(
        &mut h,
        Target::Environment("environments/dev.toml".into()),
        "Development",
    );
    let new = "environments/Development.toml";
    assert!(h.exists(new));
    assert!(!h.exists("environments/dev.toml"));
    assert_eq!(h.app.environment(), Some(new), "the selection follows");
    assert_eq!(h.app.tab().title(), "Development");
    h.update(Message::Duplicate(Target::Environment(new.into())));
    let names: Vec<&str> = h
        .app
        .workspace()
        .environments
        .iter()
        .map(|e| e.name())
        .collect();
    assert_eq!(names, ["Development", "Development Copy"]);
    h.update(Message::Delete(Target::Environment(new.into())));
    match h.app.dialog() {
        Some(Dialog::ConfirmDelete { name, .. }) => assert_eq!(name, "Development"),
        other => panic!("{other:?}"),
    }
    h.update(Message::DialogConfirm);
    assert!(!h.exists(new));
    assert_eq!(h.app.environment(), None);
    assert!(
        h.app
            .tabs()
            .iter()
            .all(|t| !matches!(t.doc, Doc::Environment(_))),
        "its tab closes"
    );
}

// --- F08-AC9 the selector --------------------------------------------------------------

#[test]
fn f08_ac9_selection_is_saved_and_cleared_when_gone() {
    let mut h = H::with(sample);
    h.update(Message::SelectEnvironment(Some(
        "environments/dev.toml".into(),
    )));
    assert_eq!(
        h.app.session().environment.as_deref(),
        Some("environments/dev.toml")
    );
    assert!(matches!(
        h.update(Message::Autosave),
        Some(Effect::SaveSession(_))
    ));
    h.store
        .apply(&[ferry_app::Op::Remove("environments/dev.toml".into())])
        .unwrap();
    h.reload();
    assert_eq!(h.app.environment(), None);
    h.update(Message::SelectEnvironment(Some(
        "environments/nope.toml".into(),
    )));
    assert_eq!(
        h.app.environment(),
        None,
        "unknown environments are ignored"
    );
}

#[test]
fn f08_ac11_broken_environments_cannot_be_selected_or_opened() {
    let mut h = H::with(|fs| {
        fs.put(
            root().join("environments/bad.toml"),
            "[[variables]]\nkey = 1\n",
        );
    });
    h.update(Message::SelectEnvironment(Some(
        "environments/bad.toml".into(),
    )));
    assert_eq!(h.app.environment(), None);
    h.update(Message::OpenEnvironment(EnvTarget::Environment(
        "environments/bad.toml".into(),
    )));
    assert_eq!(h.app.tabs().len(), 1);
    let writes = h.fs.writes();
    h.update(Message::Rename(Target::Environment(
        "environments/bad.toml".into(),
    )));
    assert!(h.app.dialog().is_none(), "broken files cannot be renamed");
    assert_eq!(h.fs.writes(), writes);
}

// --- F07-AC7 linked tabs in the session ---------------------------------------------------

#[test]
fn f07_ac7_links_and_edits_are_restored() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    h.edit(Edit::Url("edited".into()));
    h.update(Message::OpenItem(LIST.into()));
    h.update(Message::OpenEnvironment(EnvTarget::Globals));
    let session = h.app.session();
    assert_eq!(session.tabs.len(), 3, "environment tabs are not saved");
    assert_eq!(session.tabs[1].link.as_deref(), Some(HEALTH));
    assert_eq!(session.tabs[1].draft.url, "edited");
    assert_eq!(
        session.active, 0,
        "the active tab is not a request: the first one is"
    );
    let restored = H::restore(session, sample);
    let tab = &restored.app.tabs()[1];
    assert_eq!(tab.link.as_deref(), Some(HEALTH));
    assert_eq!(tab.title(), "Health");
    assert!(tab.is_dirty());
    assert!(!restored.app.tabs()[2].is_dirty());
}

#[test]
fn f07_ac7_tabs_whose_files_are_gone_restore_unlinked() {
    let session = Session {
        tabs: vec![SessionTab {
            link: Some("collections/api/gone.toml".into()),
            draft: RequestDraft {
                url: "https://gone.test".into(),
                ..RequestDraft::default()
            },
        }],
        ..Session::default()
    };
    let h = H::restore(session, sample);
    assert_eq!(h.app.tab().link, None);
    assert_eq!(h.app.tab().title(), "https://gone.test");
}

// --- F14-AC6 location -------------------------------------------------------------------

#[test]
fn f09_ac1_the_root_in_use_is_shown_but_not_saved() {
    let app = App::default().with_workspace_root(PathBuf::from("/from/env"));
    assert_eq!(app.workspace_root(), Some(Path::new("/from/env")));
    assert_eq!(app.session().workspace, None);
}

#[test]
fn f14_ac6_location_of_linked_requests() {
    let mut h = H::with(sample);
    assert!(h.app.location(h.app.tab()).is_empty());
    h.update(Message::OpenItem(LIST.into()));
    assert_eq!(h.app.location(h.app.tab()), ["API", "Users", "List users"]);
}

// --- misc ------------------------------------------------------------------------------

#[test]
fn f09_ac3_item_menu_opens_and_closes() {
    let mut h = H::with(sample);
    h.update(Message::Menu(Some(HEALTH.into())));
    assert_eq!(h.app.menu(), Some(HEALTH));
    h.update(Message::Rename(Target::Item(HEALTH.into())));
    assert_eq!(h.app.menu(), None, "choosing an action closes the menu");
    h.update(Message::DialogCancel);
    h.update(Message::Menu(Some(HEALTH.into())));
    h.update(Message::Menu(None));
    assert_eq!(h.app.menu(), None);
}

#[test]
fn f09_ac2_variables_rows_keep_the_shared_file_format() {
    // Editing through the app produces the same file format as core.
    let mut h = H::new();
    h.update(Message::NewEnvironment);
    h.edit(Edit::Var(0, VarEdit::Key("k".into())));
    h.update(Message::Save);
    assert_eq!(
        h.text("environments/New Environment.toml").unwrap(),
        "name = \"New Environment\"\n\n[[variables]]\nkey = \"k\"\nvalue = \"\"\n"
    );
    let _ = (
        KeyValue::default(),
        RowEdit::Remove,
        Table::Headers,
        Variable::default(),
        Node::Broken {
            kind: ItemKind::Request,
            reason: String::new(),
        },
    );
}

#[test]
fn f08_ac10_local_overrides_stay_local_across_saves() {
    let mut h = H::with(|fs| {
        fs.put(
            root().join("environments/dev.toml"),
            "name = \"Dev\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://shared.test\"\n",
        );
        fs.put(
            root().join("environments/dev.local.toml"),
            "[[variables]]\nkey = \"base\"\nvalue = \"http://localhost:1\"\n",
        );
    });
    h.update(Message::OpenEnvironment(EnvTarget::Environment(
        "environments/dev.toml".into(),
    )));
    for round in 0..2 {
        h.edit(Edit::Var(1, VarEdit::Key(format!("k{round}"))));
        h.update(Message::Save);
        assert!(!h.app.tab().is_dirty(), "round {round}");
        let shared = h.text("environments/dev.toml").unwrap();
        assert!(shared.contains("https://shared.test"), "{shared}");
        assert!(!shared.contains("localhost"), "{shared}");
        assert!(
            h.text("environments/dev.local.toml")
                .unwrap()
                .contains("http://localhost:1")
        );
        h.edit(Edit::Var(1, VarEdit::Remove));
        h.update(Message::Save);
    }
}

// --- F32-AC3 example tabs --------------------------------------------------------------

/// `HEALTH` with a JSON example and a binary one.
fn with_examples(fs: &MemFs) {
    sample(fs);
    let example = |name: &str, status: u16, body: &[u8]| ferry_app::Example {
        name: name.into(),
        request: RequestDraft {
            url: "{{base}}/health".into(),
            ..RequestDraft::default()
        },
        status,
        headers: vec![ferry_app::Header {
            name: "Content-Type".into(),
            value: "application/json".into(),
        }],
        body: ferry_app::Bytes::copy_from_slice(body),
    };
    let file = collection::RequestFile {
        name: "Health".into(),
        request: RequestDraft {
            url: "{{base}}/health".into(),
            ..RequestDraft::default()
        },
        examples: vec![
            example("OK", 200, br#"{"ok": true}"#),
            example("Binary", 200, &[0, 159, 146, 150, 255]),
        ],
    };
    fs.put(root().join(HEALTH), collection::write_request(&file));
}

fn example_doc(h: &H) -> ferry_app::ExampleDoc {
    match &h.app.tab().doc {
        Doc::Example(e) => (**e).clone(),
        other => panic!("{other:?}"),
    }
}

fn saved_examples(h: &H) -> Vec<ferry_app::Example> {
    collection::parse_request(&h.text(HEALTH).unwrap(), "x")
        .unwrap()
        .examples
}

#[test]
fn f32_ac3_examples_open_in_a_tab_once() {
    let mut h = H::with(with_examples);
    let tabs = h.app.tabs().len();
    h.update(Message::OpenExample(HEALTH.into(), 0));
    let e = example_doc(&h);
    assert_eq!((e.path.as_str(), e.index), (HEALTH, 0));
    assert_eq!(e.draft.name, "OK");
    assert_eq!(e.draft.status, "200");
    assert_eq!(e.draft.headers[0].key, "Content-Type");
    assert_eq!(e.draft.body, r#"{"ok": true}"#);
    assert_eq!(e.draft.raw_body, None, "text bodies are edited as text");
    assert_eq!(e.request.url, "{{base}}/health");
    assert_eq!(h.app.tab().title(), "OK");
    assert!(!h.app.tab().is_dirty());
    assert_eq!(h.app.tabs().len(), tabs + 1);
    // Again: the same tab. A missing example opens nothing.
    h.update(Message::OpenExample(HEALTH.into(), 0));
    h.update(Message::OpenExample(HEALTH.into(), 9));
    h.update(Message::OpenExample("collections/api/none.toml".into(), 0));
    assert_eq!(h.app.tabs().len(), tabs + 1);
}

#[test]
fn f32_ac3_editing_and_saving_an_example() {
    let mut h = H::with(with_examples);
    h.update(Message::OpenExample(HEALTH.into(), 0));
    h.edit(Edit::ExampleName("Healthy".into()));
    h.edit(Edit::ExampleStatus(" 201 ".into()));
    h.edit(Edit::ExampleBody("{}".into()));
    h.edit(Edit::Row(
        Table::ExampleHeaders,
        1,
        RowEdit::Key(" X-Id ".into()),
    ));
    h.edit(Edit::Row(
        Table::ExampleHeaders,
        1,
        RowEdit::Value("7".into()),
    ));
    h.edit(Edit::Row(Table::ExampleHeaders, 0, RowEdit::Enabled(false)));
    // Edits that do not belong to examples change nothing.
    h.edit(Edit::Url("ignored".into()));
    h.edit(Edit::Description("ignored".into()));
    assert!(h.app.tab().is_dirty());
    assert_eq!(h.app.tab().title(), "Healthy");
    h.update(Message::Save);
    let examples = saved_examples(&h);
    assert_eq!(examples[0].name, "Healthy");
    assert_eq!(examples[0].status, 201);
    assert_eq!(&examples[0].body[..], b"{}");
    assert_eq!(
        examples[0]
            .headers
            .iter()
            .map(|x| (x.name.as_str(), x.value.as_str()))
            .collect::<Vec<_>>(),
        [("X-Id", "7")],
        "disabled rows are left out and names trimmed"
    );
    assert_eq!(examples[0].request.url, "{{base}}/health");
    assert_eq!(examples[1].name, "Binary", "the other example is untouched");
    // The tab now shows what was saved (FND-019).
    assert!(!h.app.tab().is_dirty());
    let e = example_doc(&h);
    assert_eq!(e.draft.headers.len(), 1);
    assert_eq!(e.draft.headers[0].key, "X-Id");
    assert_eq!(e.body_revision, 0, "the body editor keeps its text");
}

#[test]
fn f32_ac3_invalid_status_codes_are_refused() {
    let mut h = H::with(with_examples);
    h.update(Message::OpenExample(HEALTH.into(), 0));
    let before = h.text(HEALTH);
    for status in ["", "abc", "99", "600", "70000"] {
        h.edit(Edit::ExampleStatus(status.into()));
        h.update(Message::Save);
        assert_eq!(
            example_doc(&h).error.as_deref(),
            Some("Enter a status code from 100 to 599"),
            "{status:?}"
        );
        assert_eq!(h.text(HEALTH), before, "{status:?}");
        assert!(h.app.tab().is_dirty());
    }
    for status in ["100", "599"] {
        h.edit(Edit::ExampleStatus(status.into()));
        h.update(Message::Save);
        assert_eq!(example_doc(&h).error, None, "{status}");
        assert_eq!(saved_examples(&h)[0].status.to_string(), status);
    }
}

#[test]
fn f32_ac3_binary_bodies_survive_saving_other_fields() {
    let mut h = H::with(with_examples);
    h.update(Message::OpenExample(HEALTH.into(), 1));
    let e = example_doc(&h);
    assert_eq!(
        e.draft.body,
        String::from_utf8_lossy(&[0, 159, 146, 150, 255])
    );
    assert!(e.draft.raw_body.is_some());
    h.edit(Edit::ExampleName("Bytes".into()));
    h.update(Message::Save);
    assert_eq!(&saved_examples(&h)[1].body[..], [0, 159, 146, 150, 255]);
    // Editing the body replaces the bytes with the text.
    h.edit(Edit::ExampleBody("text".into()));
    assert_eq!(example_doc(&h).draft.raw_body, None);
    h.update(Message::Save);
    assert_eq!(&saved_examples(&h)[1].body[..], b"text");
}

#[test]
fn f32_ac3_example_tabs_follow_their_file() {
    let mut h = H::with(with_examples);
    h.update(Message::OpenExample(HEALTH.into(), 0));
    let clean = h.app.tab().id;
    h.update(Message::OpenExample(HEALTH.into(), 1));
    h.edit(Edit::ExampleName("Mine".into()));
    // Another program renames both examples.
    let mut file = collection::parse_request(&h.text(HEALTH).unwrap(), "x").unwrap();
    file.examples[0].name = "OK v2".into();
    file.examples[0].body = "{}".into();
    file.examples[1].name = "Binary v2".into();
    h.put(HEALTH, &collection::write_request(&file));
    h.reload();
    let tab = h.app.tabs().iter().find(|t| t.id == clean).unwrap();
    assert_eq!(tab.title(), "OK v2", "a clean tab reloads");
    assert!(!tab.is_dirty());
    match &tab.doc {
        Doc::Example(e) => assert_eq!(e.body_revision, 1, "its editor reloads"),
        other => panic!("{other:?}"),
    }
    assert_eq!(h.app.tab().title(), "Mine", "a dirty tab keeps its edits");
    assert!(h.app.tab().is_dirty());
    // When the example goes away, its tab closes.
    file.examples.truncate(1);
    h.put(HEALTH, &collection::write_request(&file));
    h.reload();
    assert!(
        h.app
            .tabs()
            .iter()
            .all(|t| !matches!(&t.doc, Doc::Example(e) if e.index == 1))
    );
}

#[test]
fn f32_ac3_deleting_an_example_closes_its_tab_and_shifts_later_ones() {
    let mut h = H::with(with_examples);
    h.update(Message::OpenExample(HEALTH.into(), 0));
    h.update(Message::OpenExample(HEALTH.into(), 1));
    h.update(Message::Delete(Target::Example(HEALTH.into(), 0)));
    assert!(h.app.dialog().is_some(), "deleting asks first");
    h.update(Message::DialogConfirm);
    let examples = saved_examples(&h);
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].name, "Binary");
    let open: Vec<_> = h
        .app
        .tabs()
        .iter()
        .filter_map(|t| match &t.doc {
            Doc::Example(e) => Some((e.index, e.draft.name.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(open, [(0, "Binary".to_owned())]);
}

#[test]
fn f09_ac11_every_auth_field_edits_the_settings() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem("collections/api".into()));
    h.edit(Edit::AuthKind(AuthKind::ApiKey));
    h.edit(Edit::Password("p".into()));
    h.edit(Edit::Token("t".into()));
    h.edit(Edit::ApiKey("k".into()));
    h.edit(Edit::ApiValue("v".into()));
    // Not a settings edit.
    h.edit(Edit::ExampleName("ignored".into()));
    match &h.app.tab().doc {
        Doc::Container(c) => {
            let a = &c.draft.auth;
            assert_eq!(
                (
                    a.password.as_str(),
                    a.token.as_str(),
                    a.key.as_str(),
                    a.value.as_str()
                ),
                ("p", "t", "k", "v")
            );
        }
        other => panic!("{other:?}"),
    }
}

// --- F15-AC1 editing scripts --------------------------------------------------------

#[test]
fn f15_ac1_request_scripts_are_edited_and_saved() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem(HEALTH.into()));
    assert_eq!(h.app.tab().script_phase, ScriptPhase::PreRequest);
    h.update(Message::ScriptPhaseSelected(ScriptPhase::PostResponse));
    assert_eq!(h.app.tab().script_phase, ScriptPhase::PostResponse);
    h.edit(Edit::Script(ScriptPhase::PreRequest, "let a = 1;".into()));
    h.edit(Edit::Script(
        ScriptPhase::PostResponse,
        "pm.test('t', () => {});".into(),
    ));
    assert!(h.app.tab().is_dirty());
    h.update(Message::Save);
    let file = collection::parse_request(&h.text(HEALTH).unwrap(), "x").unwrap();
    assert_eq!(file.request.scripts.pre_request, "let a = 1;");
    assert_eq!(
        file.request.scripts.post_response,
        "pm.test('t', () => {});"
    );
    assert!(!h.app.tab().is_dirty());
}

#[test]
fn f15_ac1_collection_and_folder_scripts_are_saved() {
    let mut h = H::with(sample);
    h.update(Message::OpenItem("collections/api".into()));
    h.edit(Edit::ContainerSection(ContainerSection::Scripts));
    h.edit(Edit::Script(ScriptPhase::PreRequest, "c();".into()));
    assert!(h.app.tab().is_dirty());
    h.update(Message::Save);
    let text = h.text("collections/api/collection.toml").unwrap();
    assert!(text.contains("pre_request = \"c();\""), "{text}");
    assert!(!h.app.tab().is_dirty());
    h.update(Message::OpenItem("collections/api/users".into()));
    h.edit(Edit::Script(ScriptPhase::PostResponse, "f();".into()));
    h.update(Message::Save);
    let text = h.text("collections/api/users/folder.toml").unwrap();
    assert!(text.contains("post_response = \"f();\""), "{text}");
    // Another program changes the scripts: a clean tab follows.
    h.put(
        "collections/api/users/folder.toml",
        "name = \"Users\"\n[scripts]\npost_response = \"g();\"\n",
    );
    h.reload();
    match &h.app.tab().doc {
        Doc::Container(c) => assert_eq!(c.draft.scripts.post_response, "g();"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn f15_ac5_collection_settings_keep_local_values_local() {
    let mut h = H::with(|fs| {
        sample(fs);
        fs.put(
            root().join("collections/api/collection.local.toml"),
            "[[variables]]\nkey = \"base\"\nvalue = \"https://mine.test\"\n",
        );
    });
    h.update(Message::OpenItem("collections/api".into()));
    match &h.app.tab().doc {
        Doc::Container(c) => assert_eq!(c.draft.variables[0].value, "https://mine.test"),
        other => panic!("{other:?}"),
    }
    h.edit(Edit::Description("d".into()));
    h.update(Message::Save);
    let shared = h.text("collections/api/collection.toml").unwrap();
    assert!(shared.contains("https://api.test"), "{shared}");
    assert!(!shared.contains("mine"), "{shared}");
    let local = h.text("collections/api/collection.local.toml").unwrap();
    assert!(local.contains("https://mine.test"), "{local}");
    // Without local values, saving removes the local file.
    h.edit(Edit::Var(0, VarEdit::Remove));
    h.update(Message::Save);
    assert!(!h.exists("collections/api/collection.local.toml"));
}
