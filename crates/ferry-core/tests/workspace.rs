//! The loaded workspace and its helpers: F08-AC8, F08-AC11, F09-AC1.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use ferry_core::collection::{CollectionFile, Item, ItemKind, Node, RequestFile};
use ferry_core::environment::VariablesFile;
use ferry_core::workspace::{EnvironmentEntry, Workspace, gitignore_with_local};

#[test]
fn f09_ac1_gitignore_gets_the_local_line_once() {
    assert_eq!(
        gitignore_with_local(None).as_deref(),
        Some("*.local.toml\n")
    );
    assert_eq!(
        gitignore_with_local(Some("target/\n")).as_deref(),
        Some("target/\n*.local.toml\n")
    );
    assert_eq!(
        gitignore_with_local(Some("target/")).as_deref(),
        Some("target/\n*.local.toml\n")
    );
    assert_eq!(gitignore_with_local(Some("a\n*.local.toml\nb\n")), None);
    assert_eq!(
        gitignore_with_local(Some("a\r\n  *.local.toml  \r\n")),
        None
    );
    assert_eq!(
        gitignore_with_local(Some("# *.local.toml\n")).as_deref(),
        Some("# *.local.toml\n*.local.toml\n")
    );
    assert_eq!(
        gitignore_with_local(Some("")).as_deref(),
        Some("*.local.toml\n")
    );
}

#[test]
fn f08_ac8_environment_names_and_problems() {
    let named = EnvironmentEntry {
        path: "environments/dev.toml".into(),
        file: Ok(VariablesFile {
            name: Some("Development".into()),
            variables: vec![],
        }),
    };
    assert_eq!(named.name(), "Development");
    assert_eq!(named.problem(), None);
    let unnamed = EnvironmentEntry {
        path: "environments/qa.toml".into(),
        file: Ok(VariablesFile::default()),
    };
    assert_eq!(unnamed.name(), "qa");
    let broken = EnvironmentEntry {
        path: "environments/bad.toml".into(),
        file: Err("line 1: expected `=`".into()),
    };
    assert_eq!(broken.name(), "bad");
    assert_eq!(
        broken.problem().as_deref(),
        Some("Could not read environments/bad.toml: line 1: expected `=`")
    );
}

#[test]
fn f09_ac4_lookups() {
    let ws = Workspace {
        collections: vec![Item {
            path: "collections/A".into(),
            node: Node::Collection(CollectionFile::default()),
            items: vec![
                Item {
                    path: "collections/A/r.toml".into(),
                    node: Node::Request(RequestFile {
                        name: "r".into(),
                        ..RequestFile::default()
                    }),
                    items: vec![],
                },
                Item {
                    path: "collections/A/bad.toml".into(),
                    node: Node::Broken {
                        kind: ItemKind::Request,
                        reason: "x".into(),
                    },
                    items: vec![],
                },
            ],
        }],
        environments: vec![EnvironmentEntry {
            path: "environments/dev.toml".into(),
            file: Ok(VariablesFile::default()),
        }],
        ..Workspace::default()
    };
    assert_eq!(ws.request("collections/A/r.toml").unwrap().name, "r");
    assert!(ws.request("collections/A/bad.toml").is_none());
    assert!(ws.request("collections/A").is_none());
    assert!(ws.request("nope").is_none());
    assert!(ws.environment("environments/dev.toml").is_some());
    assert!(ws.environment("environments/x.toml").is_none());
    assert_eq!(Workspace::default().globals, Ok(VariablesFile::default()));
}
