//! The workspace folder: F08-AC10, AC11, F09-AC1, AC2, AC3, AC12, AC15.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::io;
use std::path::{Path, PathBuf};

use ferry_core::auth::AuthKind;
use ferry_core::collection::{
    CollectionFile, FolderFile, Item, ItemKind, Node, RequestFile, write_request,
};
use ferry_core::draft::RequestDraft;
use ferry_core::environment::VariablesFile;
use ferry_core::variables::{Origin, Variable};
use ferry_core::workspace::{Op, Workspace};
use ferry_store::{Fs, MemFs, RealFs, WorkspaceStore};

fn root() -> PathBuf {
    PathBuf::from("/ws")
}

fn store(fs: &MemFs) -> WorkspaceStore<MemFs> {
    WorkspaceStore::new(fs.clone(), &root())
}

fn put(fs: &MemFs, rel: &str, text: &str) {
    fs.put(root().join(rel), text);
}

fn request_text(name: &str, url: &str) -> String {
    write_request(&RequestFile {
        name: name.into(),
        description: String::new(),
        request: RequestDraft {
            url: url.into(),
            ..RequestDraft::default()
        },
        examples: vec![],
    })
}

fn names(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| i.name().to_owned()).collect()
}

// --- F09-AC1 init and .gitignore -------------------------------------------------

#[test]
fn f09_ac1_init_creates_the_layout_and_gitignore() {
    let fs = MemFs::new();
    store(&fs).init().unwrap();
    assert_eq!(fs.kind(&root().join("collections")).unwrap(), Some(true));
    assert_eq!(fs.kind(&root().join("environments")).unwrap(), Some(true));
    assert_eq!(
        fs.text(root().join(".gitignore")).as_deref(),
        Some("*.local.toml\n")
    );
    let writes = fs.writes();
    store(&fs).init().unwrap();
    assert_eq!(fs.writes(), writes, "a second init writes nothing");
}

#[test]
fn f09_ac1_existing_gitignore_lines_are_kept() {
    let fs = MemFs::new();
    put(&fs, ".gitignore", "target/\n");
    store(&fs).init().unwrap();
    assert_eq!(
        fs.text(root().join(".gitignore")).as_deref(),
        Some("target/\n*.local.toml\n")
    );
}

#[test]
fn f09_ac1_applying_ops_also_ensures_the_gitignore_line() {
    let fs = MemFs::new();
    put(&fs, ".gitignore", "node_modules\n");
    store(&fs)
        .apply(&[Op::CreateDir("collections/A".into())])
        .unwrap();
    assert!(
        fs.text(root().join(".gitignore"))
            .unwrap()
            .contains("*.local.toml")
    );
}

#[test]
fn f09_ac15_init_failure_names_the_path() {
    let fs = MemFs::new();
    fs.fail(root(), io::ErrorKind::PermissionDenied);
    let error = store(&fs).init().unwrap_err();
    assert_eq!(error.source.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("injected failure"), "{error}");
}

// --- F09-AC2/AC3 loading collections --------------------------------------------

fn sample(fs: &MemFs) {
    put(
        fs,
        "collections/api/collection.toml",
        "name = \"Payments API\"\n[auth]\nkind = \"bearer\"\ntoken = \"t\"\n",
    );
    put(
        fs,
        "collections/api/b.toml",
        &request_text("Beta", "https://b.test"),
    );
    put(
        fs,
        "collections/api/a.toml",
        &request_text("alpha", "https://a.test"),
    );
    put(
        fs,
        "collections/api/users/folder.toml",
        "name = \"Users\"\n",
    );
    put(
        fs,
        "collections/api/users/list.toml",
        &request_text("List", "https://u.test"),
    );
    put(
        fs,
        "collections/api/admin/x.toml",
        &request_text("X", "https://x.test"),
    );
    put(fs, "collections/api/broken.toml", "name = ");
    put(fs, "collections/api/notes.txt", "ignored");
    put(fs, "collections/api/mine.local.toml", "ignored");
    put(
        fs,
        "collections/api/.hidden/h.toml",
        &request_text("H", "https://h"),
    );
    put(fs, "collections/.git/config", "ignored");
    put(fs, "collections/stray.toml", "ignored: not in a collection");
    put(
        fs,
        "collections/plain/r.toml",
        &request_text("R", "https://r"),
    );
}

#[test]
fn f09_ac2_loads_collections_folders_and_requests() {
    let fs = MemFs::new();
    sample(&fs);
    let ws = store(&fs).load();
    assert_eq!(names(&ws.collections), ["Payments API", "plain"]);
    let api = &ws.collections[0];
    assert_eq!(api.path, "collections/api");
    match &api.node {
        Node::Collection(c) => assert_eq!(c.auth.kind, AuthKind::Bearer),
        other => panic!("{other:?}"),
    }
    // Sorted by display name, ignoring case: admin, alpha, Beta, broken, Users.
    assert_eq!(
        names(&api.items),
        ["admin", "alpha", "Beta", "broken", "Users"]
    );
    let admin = &api.items[0];
    assert_eq!(admin.kind(), ItemKind::Folder);
    assert_eq!(
        admin.node,
        Node::Folder(FolderFile {
            name: "admin".into(),
            ..FolderFile::default()
        }),
        "a folder without folder.toml inherits auth"
    );
    assert_eq!(names(&admin.items), ["X"]);
    assert_eq!(api.items[1].path, "collections/api/a.toml");
    let users = &api.items[4];
    assert_eq!(users.path, "collections/api/users");
    assert_eq!(names(&users.items), ["List"]);
    // A collection without collection.toml is named after its folder.
    assert_eq!(
        ws.collections[1].node,
        Node::Collection(CollectionFile {
            name: "plain".into(),
            ..CollectionFile::default()
        })
    );
}

#[test]
fn f09_ac12_broken_files_are_listed_with_their_reason() {
    let fs = MemFs::new();
    sample(&fs);
    put(&fs, "collections/bad/collection.toml", "name = [");
    put(&fs, "collections/bad/sub/folder.toml", "auth = 3");
    put(
        &fs,
        "collections/bad/sub/ok.toml",
        &request_text("Ok", "https://ok"),
    );
    let ws = store(&fs).load();
    let broken = ws
        .collections
        .iter()
        .flat_map(|c| &c.items)
        .find(|i| i.path == "collections/api/broken.toml")
        .unwrap();
    assert_eq!(broken.kind(), ItemKind::Request);
    let problem = broken.problem().unwrap();
    assert!(
        problem.starts_with("Could not read collections/api/broken.toml: line 1"),
        "{problem}"
    );
    let bad = ws
        .collections
        .iter()
        .find(|c| c.path == "collections/bad")
        .unwrap();
    assert!(matches!(
        bad.node,
        Node::Broken {
            kind: ItemKind::Collection,
            ..
        }
    ));
    let sub = &bad.items[0];
    assert!(matches!(
        sub.node,
        Node::Broken {
            kind: ItemKind::Folder,
            ..
        }
    ));
    assert_eq!(
        names(&sub.items),
        ["Ok"],
        "items of broken folders still load"
    );
}

#[test]
fn f09_ac12_unreadable_files_are_broken_too() {
    let fs = MemFs::new();
    sample(&fs);
    fs.fail_reads(
        root().join("collections/api/a.toml"),
        io::ErrorKind::PermissionDenied,
    );
    let ws = store(&fs).load();
    let item = ws.collections[0]
        .items
        .iter()
        .find(|i| i.path == "collections/api/a.toml")
        .unwrap();
    assert!(item.problem().unwrap().contains("injected read failure"));
    assert_eq!(item.name(), "a");
}

#[test]
fn f09_ac12_non_utf8_files_are_broken() {
    let fs = MemFs::new();
    fs.put(root().join("collections/c/r.toml"), [0xff, 0xfe]);
    let ws = store(&fs).load();
    assert!(ws.collections[0].items[0].problem().is_some());
}

#[test]
fn f09_ac3_order_files_set_the_order() {
    let fs = MemFs::new();
    sample(&fs);
    put(
        &fs,
        "collections/api/_order.toml",
        "order = [\"users\", \"b.toml\", \"gone.toml\"]\n",
    );
    let ws = store(&fs).load();
    assert_eq!(
        names(&ws.collections[0].items),
        ["Users", "Beta", "admin", "alpha", "broken"]
    );
    assert_eq!(
        ws.orders["collections/api"],
        ["users", "b.toml", "gone.toml"]
    );
    assert_eq!(ws.orders.len(), 1);
}

#[test]
fn f09_ac3_a_broken_order_file_is_ignored() {
    let fs = MemFs::new();
    sample(&fs);
    put(&fs, "collections/api/_order.toml", "order = 5");
    let ws = store(&fs).load();
    assert!(ws.orders.is_empty());
    assert_eq!(
        names(&ws.collections[0].items),
        ["admin", "alpha", "Beta", "broken", "Users"]
    );
}

#[test]
fn f09_ac2_empty_or_missing_workspace_loads_empty() {
    let fs = MemFs::new();
    assert_eq!(store(&fs).load(), Workspace::default());
    store(&fs).init().unwrap();
    assert_eq!(store(&fs).load(), Workspace::default());
}

// --- F08-AC10/AC11 environments and globals -----------------------------------------

#[test]
fn f08_ac10_loads_environments_sorted_with_local_values() {
    let fs = MemFs::new();
    put(
        &fs,
        "environments/prod.toml",
        "name = \"Production\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://prod\"\n",
    );
    put(
        &fs,
        "environments/dev.toml",
        "name = \"development\"\n[[variables]]\nkey = \"token\"\nvalue = \"\"\nsecret = true\n",
    );
    put(
        &fs,
        "environments/dev.local.toml",
        "[[variables]]\nkey = \"token\"\nvalue = \"mine\"\n",
    );
    put(
        &fs,
        "environments/orphan.local.toml",
        "[[variables]]\nkey = \"x\"\n",
    );
    put(&fs, "environments/readme.md", "ignored");
    let ws = store(&fs).load();
    let names: Vec<&str> = ws.environments.iter().map(|e| e.name()).collect();
    assert_eq!(names, ["development", "Production"]);
    let dev = ws.environment("environments/dev.toml").unwrap();
    let token = &dev.file.as_ref().unwrap().variables[0];
    assert_eq!(token.value, "mine");
    assert!(token.secret);
    assert_eq!(
        token.origin,
        Origin::Overridden {
            shared: String::new()
        }
    );
}

#[test]
fn f08_ac11_broken_environment_files_are_listed() {
    let fs = MemFs::new();
    put(&fs, "environments/bad.toml", "[[variables]]\nkey = 1\n");
    put(&fs, "environments/badlocal.toml", "name = \"B\"\n");
    put(&fs, "environments/badlocal.local.toml", "variables = 3");
    let ws = store(&fs).load();
    assert_eq!(ws.environments.len(), 2);
    for env in &ws.environments {
        let problem = env.problem().unwrap();
        assert!(
            problem.starts_with("Could not read environments/"),
            "{problem}"
        );
    }
}

#[test]
fn f08_ac10_loads_globals_with_local_values() {
    let fs = MemFs::new();
    assert_eq!(store(&fs).load().globals, Ok(VariablesFile::default()));
    put(
        &fs,
        "globals.toml",
        "[[variables]]\nkey = \"g\"\nvalue = \"shared\"\n",
    );
    put(
        &fs,
        "globals.local.toml",
        "[[variables]]\nkey = \"g\"\nvalue = \"local\"\n",
    );
    let globals = store(&fs).load().globals.unwrap();
    assert_eq!(globals.variables[0].value, "local");
    put(&fs, "globals.toml", "variables = 1");
    assert!(store(&fs).load().globals.is_err());
}

// --- applying operations ------------------------------------------------------

#[test]
fn f09_ac2_write_create_and_remove() {
    let fs = MemFs::new();
    let s = store(&fs);
    s.apply(&[
        Op::CreateDir("collections/New Collection".into()),
        Op::Write {
            path: "collections/New Collection/collection.toml".into(),
            contents: "name = \"New Collection\"\n".into(),
        },
        Op::Write {
            path: "collections/New Collection/r.toml".into(),
            contents: request_text("R", "https://r"),
        },
    ])
    .unwrap();
    let ws = s.load();
    assert_eq!(names(&ws.collections), ["New Collection"]);
    assert_eq!(names(&ws.collections[0].items), ["R"]);
    s.apply(&[
        Op::Remove("collections/New Collection/r.toml".into()),
        Op::Remove("collections/New Collection/missing.toml".into()),
    ])
    .unwrap();
    assert!(s.load().collections[0].items.is_empty());
    s.apply(&[Op::Remove("collections/New Collection".into())])
        .unwrap();
    assert!(s.load().collections.is_empty());
}

#[test]
fn f09_ac7_rename_and_copy_files_and_directories() {
    let fs = MemFs::new();
    sample(&fs);
    let s = store(&fs);
    s.apply(&[
        Op::Rename {
            from: "collections/api/users".into(),
            to: "collections/api/people".into(),
        },
        Op::Copy {
            from: "collections/api/people".into(),
            to: "collections/api/people copy".into(),
        },
        Op::Copy {
            from: "collections/api/a.toml".into(),
            to: "collections/api/a2.toml".into(),
        },
    ])
    .unwrap();
    let files: Vec<PathBuf> = fs.files();
    for expected in [
        "collections/api/people/list.toml",
        "collections/api/people/folder.toml",
        "collections/api/people copy/list.toml",
        "collections/api/a2.toml",
    ] {
        assert!(files.contains(&root().join(expected)), "{expected}");
    }
    assert!(!files.contains(&root().join("collections/api/users/list.toml")));
}

#[test]
fn f09_ac15_rename_and_copy_refuse_to_overwrite() {
    let fs = MemFs::new();
    sample(&fs);
    let s = store(&fs);
    s.init().unwrap(); // so the .gitignore line is already there
    let before = fs.files();
    for op in [
        Op::Rename {
            from: "collections/api/a.toml".into(),
            to: "collections/api/b.toml".into(),
        },
        Op::Copy {
            from: "collections/api/a.toml".into(),
            to: "collections/api/b.toml".into(),
        },
        Op::Copy {
            from: "collections/api/users".into(),
            to: "collections/api/admin".into(),
        },
    ] {
        let error = s.apply(&[op]).unwrap_err();
        assert_eq!(error.source.kind(), io::ErrorKind::AlreadyExists);
    }
    assert_eq!(fs.files(), before);
    let missing = s
        .apply(&[Op::Copy {
            from: "collections/api/nope.toml".into(),
            to: "collections/api/n.toml".into(),
        }])
        .unwrap_err();
    assert_eq!(missing.source.kind(), io::ErrorKind::NotFound);
    assert_eq!(missing.path, "collections/api/nope.toml");
}

#[test]
fn f09_ac15_apply_stops_at_the_first_failure() {
    let fs = MemFs::new();
    store(&fs).init().unwrap();
    fs.fail(
        root().join("collections/locked"),
        io::ErrorKind::PermissionDenied,
    );
    let error = store(&fs)
        .apply(&[
            Op::Write {
                path: "collections/ok/a.toml".into(),
                contents: "x".into(),
            },
            Op::Write {
                path: "collections/locked/b.toml".into(),
                contents: "y".into(),
            },
            Op::Write {
                path: "collections/ok/c.toml".into(),
                contents: "z".into(),
            },
        ])
        .unwrap_err();
    assert_eq!(error.path, "collections/locked/b.toml");
    assert_eq!(error.source.kind(), io::ErrorKind::PermissionDenied);
    assert!(fs.text(root().join("collections/ok/a.toml")).is_some());
    assert!(fs.text(root().join("collections/ok/c.toml")).is_none());
    assert!(error.to_string().starts_with("collections/locked/b.toml: "));
}

#[test]
fn f09_ac15_paths_must_stay_inside_the_workspace() {
    let fs = MemFs::new();
    let s = store(&fs);
    for bad in [
        "",
        "../x.toml",
        "collections/../../x",
        "/abs.toml",
        "./x",
        "C:\\x",
    ] {
        let op = Op::Write {
            path: bad.into(),
            contents: "x".into(),
        };
        let result = s.apply(&[op]);
        if bad == "C:\\x" && !cfg!(windows) {
            // An ordinary file name on Unix.
            continue;
        }
        let error = result.unwrap_err();
        assert_eq!(error.source.kind(), io::ErrorKind::InvalidInput, "{bad}");
    }
    for bad in ["../out", "/etc"] {
        assert!(s.apply(&[Op::Remove(bad.into())]).is_err());
        assert!(s.apply(&[Op::CreateDir(bad.into())]).is_err());
        assert!(
            s.apply(&[Op::Rename {
                from: "x".into(),
                to: bad.into()
            }])
            .is_err()
        );
        assert!(
            s.apply(&[Op::Copy {
                from: bad.into(),
                to: "x".into()
            }])
            .is_err()
        );
    }
    assert!(fs.files().iter().all(|f| f.starts_with(root())));
}

// --- the real filesystem --------------------------------------------------------

fn temp_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferry-ws-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn f09_ac2_real_filesystem_round_trip() {
    let dir = temp_root("roundtrip");
    let s = WorkspaceStore::new(RealFs, &dir);
    s.init().unwrap();
    let file = RequestFile {
        name: "Get user".into(),
        description: String::new(),
        request: RequestDraft {
            url: "{{base}}/users/1".into(),
            ..RequestDraft::default()
        },
        examples: vec![],
    };
    s.apply(&[
        Op::CreateDir("collections/API".into()),
        Op::Write {
            path: "collections/API/collection.toml".into(),
            contents: "name = \"API\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://x\"\n"
                .into(),
        },
        Op::Write {
            path: "collections/API/sub/Get user.toml".into(),
            contents: write_request(&file),
        },
        Op::Write {
            path: "environments/dev.toml".into(),
            contents: "name = \"Dev\"\n".into(),
        },
    ])
    .unwrap();
    let ws = s.load();
    assert_eq!(names(&ws.collections), ["API"]);
    let sub = &ws.collections[0].items[0];
    assert_eq!(sub.items[0].node, Node::Request(file));
    assert_eq!(ws.environments[0].name(), "Dev");
    s.apply(&[
        Op::Rename {
            from: "collections/API/sub".into(),
            to: "collections/API/moved".into(),
        },
        Op::Copy {
            from: "collections/API/moved".into(),
            to: "collections/API/copied".into(),
        },
    ])
    .unwrap();
    let ws = s.load();
    assert_eq!(names(&ws.collections[0].items), ["copied", "moved"]);
    assert!(
        s.apply(&[Op::Rename {
            from: "collections/API/moved".into(),
            to: "collections/API/copied".into(),
        }])
        .is_err()
    );
    s.apply(&[Op::Remove("collections/API".into())]).unwrap();
    assert!(s.load().collections.is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
        "*.local.toml\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn f09_ac2_saved_variables_survive_a_reload_with_their_secrets_kept_local() {
    let dir = temp_root("secrets");
    let s = WorkspaceStore::new(RealFs, &dir);
    let file = VariablesFile {
        name: Some("Dev".into()),
        variables: vec![Variable {
            secret: true,
            ..Variable::new("token", "s3cret")
        }],
    };
    let (shared, local) = ferry_core::environment::write(&file);
    s.apply(&[
        Op::Write {
            path: "environments/dev.toml".into(),
            contents: shared,
        },
        Op::Write {
            path: "environments/dev.local.toml".into(),
            contents: local.unwrap(),
        },
    ])
    .unwrap();
    let shared_text = std::fs::read_to_string(dir.join("environments/dev.toml")).unwrap();
    assert!(!shared_text.contains("s3cret"));
    let loaded = s.load();
    let token = &loaded.environments[0].file.as_ref().unwrap().variables[0];
    assert_eq!(token.value, "s3cret");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn f09_ac2_root_reports_the_folder() {
    let s = WorkspaceStore::new(MemFs::new(), Path::new("/somewhere"));
    assert_eq!(s.root(), Path::new("/somewhere"));
}

#[test]
fn f15_ac5_loads_collections_with_local_values() {
    let fs = MemFs::new();
    put(
        &fs,
        "collections/api/collection.toml",
        "name = \"API\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://shared\"\n",
    );
    put(
        &fs,
        "collections/api/collection.local.toml",
        "[[variables]]\nkey = \"base\"\nvalue = \"https://mine\"\n[[variables]]\nkey = \"token\"\nvalue = \"t\"\n",
    );
    // A collection without collection.toml can still have local values.
    put(
        &fs,
        "collections/bare/collection.local.toml",
        "[[variables]]\nkey = \"x\"\nvalue = \"1\"\n",
    );
    put(
        &fs,
        "collections/bad/collection.local.toml",
        "variables = 3",
    );
    let ws = store(&fs).load();
    let vars = |i: usize| match &ws.collections[i].node {
        Node::Collection(c) => c
            .variables
            .iter()
            .map(|v| (v.key.clone(), v.value.clone()))
            .collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(names(&ws.collections), ["API", "bad", "bare"]);
    assert_eq!(
        vars(0),
        [
            ("base".to_owned(), "https://mine".to_owned()),
            ("token".to_owned(), "t".to_owned())
        ]
    );
    assert!(ws.collections[0].items.is_empty(), "not a request");
    match &ws.collections[1].node {
        Node::Broken { reason, .. } => {
            assert!(reason.starts_with("collection.local.toml: "), "{reason}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(vars(2), [("x".to_owned(), "1".to_owned())]);
}
