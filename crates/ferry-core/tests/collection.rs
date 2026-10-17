//! Workspace file formats and the collections tree: F09-AC2, AC3, AC8,
//! AC12, AC13, F04-AC6, F08-AC10, F32-AC2.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use bytes::Bytes;
use ferry_core::auth::{ApiKeyLocation, AuthDraft, AuthKind};
use ferry_core::body::{BodyDraft, BodyMode, FormField, RawLanguage};
use ferry_core::collection::{
    AuthSource, CollectionFile, Example, FolderFile, Item, ItemKind, Node, RequestFile, ancestors,
    arrange, collection_variables, file_name, find, inherit_note, inherited_auth, parent,
    parse_collection, parse_folder, parse_order, parse_request, search, write_collection,
    write_folder, write_order, write_request,
};
use ferry_core::draft::RequestDraft;
use ferry_core::environment::{self, VariablesFile};
use ferry_core::kv::KeyValue;
use ferry_core::names::{MAX_NAME_CHARS, item_file, sanitize, unique};
use ferry_core::params::Param;
use ferry_core::request::{Method, SendOptions};
use ferry_core::response::Header;
use ferry_core::variables::{Origin, Variable};
use proptest::prelude::*;

// --- F09-AC8 names -----------------------------------------------------------

#[test]
fn f09_ac8_forbidden_characters_become_underscores() {
    assert_eq!(sanitize(r#"a<b>c:d"e/f\g|h?i*j"#), "a_b_c_d_e_f_g_h_i_j");
    assert_eq!(sanitize("tab\there\u{7}"), "tab_here_");
    assert_eq!(sanitize("Get user"), "Get user");
    assert_eq!(sanitize("ünïcödé 名前"), "ünïcödé 名前");
}

#[test]
fn f09_ac8_trailing_dots_and_spaces_are_trimmed() {
    assert_eq!(sanitize("name. . "), "name");
    assert_eq!(sanitize("  leading kept"), "  leading kept");
    assert_eq!(sanitize("..."), "Untitled");
    assert_eq!(sanitize(""), "Untitled");
    assert_eq!(sanitize("   "), "Untitled");
}

#[test]
fn f09_ac8_windows_reserved_names_get_an_underscore() {
    for name in [
        "CON", "con", "PRN", "AUX", "NUL", "COM1", "com9", "LPT1", "LPT9",
    ] {
        assert_eq!(sanitize(name), format!("{name}_"));
    }
    assert_eq!(sanitize("NUL.txt"), "NUL_.txt");
    assert_eq!(sanitize("con.tar.gz"), "con_.tar.gz");
    for fine in ["CONSOLE", "COM10", "LPT0", "COM", "NULL", "xCON"] {
        assert_eq!(sanitize(fine), fine);
    }
}

#[test]
fn f09_ac8_reserved_names_stay_within_the_limit() {
    let name = format!("CON.{}", "x".repeat(120));
    let out = sanitize(&name);
    assert_eq!(out.chars().count(), MAX_NAME_CHARS);
    assert!(out.starts_with("CON_."));
    assert_eq!(sanitize(&out), out);
}

#[test]
fn f09_ac8_long_names_are_cut_at_100_characters() {
    let long = "é".repeat(150);
    assert_eq!(sanitize(&long).chars().count(), MAX_NAME_CHARS);
    // Cutting can expose trailing spaces, which are trimmed again.
    let spaced = format!("{} {}", "a".repeat(99), "b".repeat(10));
    assert_eq!(sanitize(&spaced), "a".repeat(99));
}

#[test]
fn f09_ac8_collisions_get_numbered_suffixes() {
    assert_eq!(unique("Get user", ".toml", &[]), "Get user.toml");
    assert_eq!(
        unique("Get user", ".toml", &["get USER.toml"]),
        "Get user (2).toml"
    );
    assert_eq!(
        unique(
            "Get user",
            ".toml",
            &["Get user.toml", "Get user (2).toml", "other"]
        ),
        "Get user (3).toml"
    );
    assert_eq!(unique("Users", "", &["users"]), "Users (2)");
    assert_eq!(unique("a/b", ".toml", &["a_b.toml"]), "a_b (2).toml");
}

#[test]
fn f09_ac8_item_files_avoid_ferrys_own_names() {
    assert_eq!(item_file("collection", ".toml", &[]), "collection (2).toml");
    assert_eq!(item_file("Folder", ".toml", &[]), "Folder (2).toml");
    assert_eq!(item_file("_order", ".toml", &[]), "_order (2).toml");
    assert_eq!(item_file("dev.local", ".toml", &[]), "dev.local_.toml");
    assert_eq!(item_file("dev.LOCAL", ".toml", &[]), "dev.LOCAL_.toml");
    assert_eq!(item_file(".git", "", &[]), "_git");
    assert_eq!(item_file(".hidden", ".toml", &[]), "_hidden.toml");
    assert_eq!(item_file("collection", "", &[]), "collection");
    assert_eq!(item_file("Users", "", &["users"]), "Users (2)");
    assert_eq!(
        item_file("Get user", ".toml", &["Get user.toml"]),
        "Get user (2).toml"
    );
}

proptest! {
    #[test]
    fn f09_ac8_prop_sanitized_names_are_safe(name in "\\PC{0,130}") {
        let out = sanitize(&name);
        prop_assert!(!out.is_empty());
        prop_assert!(out.chars().count() <= MAX_NAME_CHARS);
        let bad = out.chars().any(|c| "<>:\"/\\|?*".contains(c) || c.is_control());
        prop_assert!(!bad);
        prop_assert!(!out.ends_with(['.', ' ']));
        prop_assert_eq!(sanitize(&out), out.clone(), "idempotent");
    }

    #[test]
    fn f09_ac8_prop_unique_never_collides(names in proptest::collection::vec("[aA ]{0,3}", 0..8), name in "[aA ]{0,3}") {
        let siblings: Vec<&str> = names.iter().map(String::as_str).collect();
        let out = unique(&name, "", &siblings);
        let collides = siblings.iter().any(|s| s.eq_ignore_ascii_case(&out));
        prop_assert!(!collides);
    }
}

// --- F09-AC2 request files ---------------------------------------------------

fn rich_request() -> RequestFile {
    let mut request = RequestDraft {
        method: Method::Post,
        url: "{{base}}/users?page=2".into(),
        params: vec![
            Param::new("page", "2"),
            Param {
                row: KeyValue {
                    enabled: false,
                    description: "off".into(),
                    ..KeyValue::new("debug", "1")
                },
                original: None,
            },
        ],
        headers: vec![KeyValue::new("Accept", "application/json")],
        body: BodyDraft {
            mode: BodyMode::Raw,
            raw: "{\n  \"name\": \"Ada\"\n}".into(),
            language: RawLanguage::Json,
            ..BodyDraft::default()
        },
        auth: AuthDraft {
            kind: AuthKind::Bearer,
            token: "{{token}}".into(),
            ..AuthDraft::default()
        },
        options: SendOptions {
            follow_redirects: false,
            ..SendOptions::default()
        },
    };
    request.body.form = vec![FormField {
        row: KeyValue::new("file", ""),
        file: Some("data/a.png".into()),
    }];
    RequestFile {
        name: "Create user".into(),
        request: request.clone(),
        examples: vec![
            Example {
                name: "Created".into(),
                request: request.clone(),
                status: 201,
                headers: vec![Header {
                    name: "content-type".into(),
                    value: "application/json".into(),
                }],
                body: Bytes::from_static(b"{\"id\": 7}"),
            },
            Example {
                name: "Binary".into(),
                request: RequestDraft::default(),
                status: 200,
                headers: vec![],
                body: Bytes::from_static(&[0, 159, 146, 150, 255]),
            },
        ],
    }
}

#[test]
fn f09_ac2_request_file_format() {
    insta::assert_snapshot!("f09_ac2_request_file", write_request(&rich_request()));
}

#[test]
fn f09_ac2_minimal_request_file_omits_defaults() {
    let file = RequestFile {
        name: "Ping".into(),
        request: RequestDraft {
            url: "https://a.test/".into(),
            ..RequestDraft::default()
        },
        examples: vec![],
    };
    assert_eq!(
        write_request(&file),
        "name = \"Ping\"\nurl = \"https://a.test/\"\n"
    );
}

#[test]
fn f09_ac2_request_files_round_trip() {
    let file = rich_request();
    assert_eq!(parse_request(&write_request(&file), "x").unwrap(), file);
}

#[test]
fn f09_ac2_missing_name_falls_back() {
    let file = parse_request("url = \"https://a.test/\"\n", "From file").unwrap();
    assert_eq!(file.name, "From file");
    assert_eq!(file.request.url, "https://a.test/");
    assert_eq!(file.request.method, Method::Get);
    let empty = parse_request("", "Empty").unwrap();
    assert_eq!(empty.name, "Empty");
}

#[test]
fn f09_ac2_params_get_their_original_text_from_the_url() {
    // So an untouched param keeps its spelling when the table is edited.
    let file = parse_request(
        "url = \"https://a.test/?q=a%20b\"\n[[params]]\nkey = \"q\"\nvalue = \"a%20b\"\n",
        "x",
    )
    .unwrap();
    assert_eq!(file.request.params[0].original.as_deref(), Some("q=a%20b"));
}

#[test]
fn f09_ac12_invalid_request_files_report_a_reason() {
    for text in [
        "name = ",
        "method = \"BAD METHOD\"",
        "[body]\nmode = \"telepathy\"",
        "[[examples]]\nname = \"e\"\nbody = \"!!!\"\nbody_encoding = \"base64\"",
        "[[examples]]\nname = \"e\"\nbody = \"\"\nbody_encoding = \"rot13\"",
    ] {
        let error = parse_request(text, "x").unwrap_err();
        assert!(!error.to_string().is_empty(), "{text}");
    }
}

#[test]
fn f32_ac2_bodies_are_text_or_base64() {
    let text = write_request(&rich_request());
    // The JSON body stays readable text; the binary one is base64.
    assert!(
        text.contains(r#"{"id": 7}"#) || text.contains(r#"{\"id\": 7}"#),
        "{text}"
    );
    assert!(text.contains("body = \"AJ+Slv8=\""), "{text}");
    assert!(text.contains("body_encoding = \"base64\""), "{text}");
    assert_eq!(text.matches("body_encoding").count(), 1);
}

#[test]
fn f32_ac2_empty_example_body() {
    let file = RequestFile {
        name: "r".into(),
        request: RequestDraft::default(),
        examples: vec![Example {
            name: "e".into(),
            status: 204,
            ..Example::default()
        }],
    };
    assert_eq!(parse_request(&write_request(&file), "r").unwrap(), file);
}

fn draft_strategy() -> impl Strategy<Value = RequestDraft> {
    let kv = ("[a-z{} ]{0,6}", "\\PC{0,8}", "[a-z]{0,4}", any::<bool>()).prop_map(
        |(key, value, description, enabled)| KeyValue {
            key,
            value,
            description,
            enabled,
        },
    );
    (
        prop_oneof![
            Just(Method::Get),
            Just(Method::Post),
            Just(Method::Custom("PURGE".into()))
        ],
        "\\PC{0,20}",
        proptest::collection::vec(kv.clone(), 0..3),
        proptest::collection::vec(kv.clone(), 0..3),
        "\\PC{0,30}",
        prop_oneof![
            Just(BodyMode::None),
            Just(BodyMode::Raw),
            Just(BodyMode::FormData)
        ],
        prop_oneof![
            Just(AuthKind::Inherit),
            Just(AuthKind::Basic),
            Just(AuthKind::ApiKey)
        ],
        any::<(bool, bool)>(),
    )
        .prop_map(|(method, url, params, headers, raw, mode, kind, (a, b))| {
            let mut d = RequestDraft {
                method,
                url,
                params: params
                    .into_iter()
                    .map(|row| Param {
                        row,
                        original: None,
                    })
                    .collect(),
                headers,
                ..RequestDraft::default()
            };
            d.body.mode = mode;
            d.body.raw = raw;
            d.auth.kind = kind;
            d.auth.username = "u".into();
            d.auth.location = if a {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            };
            d.options.verify_tls = b;
            d
        })
}

proptest! {
    #[test]
    fn f09_ac2_prop_request_files_round_trip(
        name in "\\PC{0,12}",
        request in draft_strategy(),
        body in proptest::collection::vec(any::<u8>(), 0..40),
        status in 100u16..600,
    ) {
        let file = RequestFile {
            name: name.clone(),
            request: request.clone(),
            examples: vec![Example {
                name,
                request,
                status,
                headers: vec![Header { name: "x".into(), value: "y".into() }],
                body: Bytes::from(body),
            }],
        };
        let text = write_request(&file);
        let back = parse_request(&text, "fallback").unwrap();
        // Params come back with their original text; compare visible content.
        prop_assert_eq!(back, file);
    }
}

// --- collection and folder files -------------------------------------------------

#[test]
fn f09_ac2_collection_file_round_trip_and_format() {
    let file = CollectionFile {
        name: "Payments API".into(),
        description: "Charges and refunds.".into(),
        auth: AuthDraft {
            kind: AuthKind::Bearer,
            token: "{{token}}".into(),
            ..AuthDraft::default()
        },
        variables: vec![Variable::new("base", "https://pay.test")],
    };
    let text = write_collection(&file);
    insta::assert_snapshot!("f09_ac2_collection_file", text);
    assert_eq!(parse_collection(&text, "x").unwrap(), file);
    assert_eq!(
        write_collection(&CollectionFile {
            name: "C".into(),
            ..CollectionFile::default()
        }),
        "name = \"C\"\n"
    );
    assert_eq!(parse_collection("", "Dir name").unwrap().name, "Dir name");
    assert!(parse_collection("name = 3", "x").is_err());
}

#[test]
fn f09_ac2_folder_auth_defaults_to_inherit() {
    let folder = parse_folder("", "users").unwrap();
    assert_eq!(folder.name, "users");
    assert_eq!(folder.auth.kind, AuthKind::Inherit);
    assert_eq!(FolderFile::default().auth.kind, AuthKind::Inherit);
    let file = FolderFile {
        name: "Users".into(),
        description: "d".into(),
        ..FolderFile::default()
    };
    assert_eq!(parse_folder(&write_folder(&file), "x").unwrap(), file);
    let no_auth = FolderFile {
        auth: AuthDraft::default(),
        ..file
    };
    assert_eq!(parse_folder(&write_folder(&no_auth), "x").unwrap(), no_auth);
    assert!(parse_folder("[auth]\nkind = 7", "x").is_err());
}

#[test]
fn f09_ac3_order_files() {
    let names = vec!["b.toml".to_owned(), "a dir".to_owned()];
    let text = write_order(&names);
    assert_eq!(text, "order = [\"b.toml\", \"a dir\"]\n");
    assert_eq!(parse_order(&text).unwrap(), names);
    assert_eq!(parse_order("").unwrap(), Vec::<String>::new());
    assert!(parse_order("order = 1").is_err());
}

// --- F08-AC10 environment files ------------------------------------------------

fn secret(key: &str, value: &str) -> Variable {
    Variable {
        secret: true,
        ..Variable::new(key, value)
    }
}

#[test]
fn f08_ac10_environment_file_format() {
    let file = VariablesFile {
        name: Some("Staging".into()),
        variables: vec![
            Variable::new("base", "https://staging.test"),
            Variable {
                enabled: false,
                ..Variable::new("debug", "1")
            },
            secret("token", "s3cret"),
        ],
    };
    let (shared, local) = environment::write(&file);
    insta::assert_snapshot!("f08_ac10_environment_shared", shared);
    assert_eq!(
        local.as_deref(),
        Some("[[variables]]\nkey = \"token\"\nvalue = \"s3cret\"\n")
    );
    assert!(!shared.contains("s3cret"));
    let back = environment::parse(&shared, local.as_deref()).unwrap();
    assert_eq!(back.name.as_deref(), Some("Staging"));
    assert_eq!(back.variables[2].value, "s3cret");
    assert!(back.variables[2].secret);
    assert_eq!(environment::write(&back), (shared, local));
}

#[test]
fn f08_ac10_without_secrets_there_is_no_local_file() {
    let file = VariablesFile {
        name: None,
        variables: vec![Variable::new("a", "1")],
    };
    let (shared, local) = environment::write(&file);
    assert_eq!(shared, "[[variables]]\nkey = \"a\"\nvalue = \"1\"\n");
    assert_eq!(local, None);
    assert_eq!(environment::parse(&shared, None).unwrap(), file);
    assert_eq!(
        environment::write(&VariablesFile::default()),
        (String::new(), None)
    );
}

#[test]
fn f08_ac10_local_values_override_and_stay_local() {
    let shared = "name = \"Dev\"\n[[variables]]\nkey = \"base\"\nvalue = \"https://shared.test\"\n[[variables]]\nkey = \"plain\"\nvalue = \"p\"\n";
    let local = "[[variables]]\nkey = \"base\"\nvalue = \"http://localhost:8080\"\n[[variables]]\nkey = \"mine\"\nvalue = \"only here\"\n";
    let file = environment::parse(shared, Some(local)).unwrap();
    let values: Vec<(&str, &str)> = file
        .variables
        .iter()
        .map(|v| (v.key.as_str(), v.value.as_str()))
        .collect();
    assert_eq!(
        values,
        [
            ("base", "http://localhost:8080"),
            ("plain", "p"),
            ("mine", "only here")
        ]
    );
    assert_eq!(
        file.variables[0].origin,
        Origin::Overridden {
            shared: "https://shared.test".into()
        }
    );
    assert_eq!(file.variables[1].origin, Origin::Shared);
    assert_eq!(file.variables[2].origin, Origin::LocalOnly);
    // Saving puts every value back where it came from.
    let (shared_out, local_out) = environment::write(&file);
    assert_eq!(
        environment::parse(&shared_out, None)
            .unwrap()
            .variables
            .len(),
        2
    );
    assert!(shared_out.contains("https://shared.test"));
    assert!(!shared_out.contains("localhost"));
    assert!(!shared_out.contains("only here"));
    assert_eq!(
        environment::parse(&shared_out, local_out.as_deref()).unwrap(),
        file
    );
}

#[test]
fn f08_ac10_invalid_files_are_errors() {
    assert!(environment::parse("[[variables]]\nkey = 1", None).is_err());
    assert!(environment::parse("", Some("[[variables]]\nvalue = [")).is_err());
}

#[test]
fn f08_ac10_local_paths() {
    assert_eq!(
        environment::local_path("environments/dev.toml"),
        "environments/dev.local.toml"
    );
    assert_eq!(
        environment::local_path("globals.toml"),
        "globals.local.toml"
    );
    assert_eq!(environment::local_path("odd"), "odd.local.toml");
}

proptest! {
    #[test]
    fn f08_ac10_prop_variables_round_trip(vars in proptest::collection::vec(("[a-z]{1,4}", "\\PC{0,10}", any::<bool>(), any::<bool>()), 0..6)) {
        // Keys are unique per file in practice; duplicates would merge.
        let mut seen = std::collections::HashSet::new();
        let variables: Vec<Variable> = vars
            .into_iter()
            .filter(|(k, ..)| seen.insert(k.clone()))
            .map(|(key, value, enabled, secret)| Variable { key, value, enabled, secret, origin: Origin::Shared })
            .collect();
        let file = VariablesFile { name: Some("E".into()), variables };
        let (shared, local) = environment::write(&file);
        let back = environment::parse(&shared, local.as_deref()).unwrap();
        prop_assert_eq!(back.variables.len(), file.variables.len());
        for (a, b) in back.variables.iter().zip(&file.variables) {
            prop_assert_eq!((&a.key, &a.value, a.enabled, a.secret), (&b.key, &b.value, b.enabled, b.secret));
        }
        prop_assert_eq!(environment::write(&back), (shared, local));
    }
}

// --- tree --------------------------------------------------------------------

fn request(path: &str, name: &str, url: &str, auth: AuthKind) -> Item {
    Item {
        path: path.into(),
        node: Node::Request(RequestFile {
            name: name.into(),
            request: RequestDraft {
                url: url.into(),
                auth: AuthDraft {
                    kind: auth,
                    ..AuthDraft::default()
                },
                ..RequestDraft::default()
            },
            examples: vec![],
        }),
        items: vec![],
    }
}

fn folder(path: &str, name: &str, auth: AuthDraft, items: Vec<Item>) -> Item {
    Item {
        path: path.into(),
        node: Node::Folder(FolderFile {
            name: name.into(),
            description: String::new(),
            auth,
        }),
        items,
    }
}

fn inherit() -> AuthDraft {
    AuthDraft {
        kind: AuthKind::Inherit,
        ..AuthDraft::default()
    }
}

fn bearer(token: &str) -> AuthDraft {
    AuthDraft {
        kind: AuthKind::Bearer,
        token: token.into(),
        ..AuthDraft::default()
    }
}

/// collections/API (bearer col)
///   users/ (inherit)
///     admins/ (bearer admin)
///       Promote.toml
///     List.toml
///   Health.toml
///   broken/ (broken folder.toml)
///     Deep.toml
/// collections/Other (inherit = no auth)
///   Ping.toml
fn tree() -> Vec<Item> {
    vec![
        Item {
            path: "collections/API".into(),
            node: Node::Collection(CollectionFile {
                name: "API".into(),
                auth: bearer("col"),
                variables: vec![Variable::new("base", "https://api.test")],
                ..CollectionFile::default()
            }),
            items: vec![
                folder(
                    "collections/API/users",
                    "Users",
                    inherit(),
                    vec![
                        folder(
                            "collections/API/users/admins",
                            "Admins",
                            bearer("admin"),
                            vec![request(
                                "collections/API/users/admins/Promote.toml",
                                "Promote",
                                "{{base}}/promote",
                                AuthKind::Inherit,
                            )],
                        ),
                        request(
                            "collections/API/users/List.toml",
                            "List users",
                            "{{base}}/users",
                            AuthKind::Inherit,
                        ),
                    ],
                ),
                request(
                    "collections/API/Health.toml",
                    "Health",
                    "{{base}}/health",
                    AuthKind::None,
                ),
                Item {
                    path: "collections/API/broken".into(),
                    node: Node::Broken {
                        kind: ItemKind::Folder,
                        reason: "expected `=`".into(),
                    },
                    items: vec![request(
                        "collections/API/broken/Deep.toml",
                        "Deep",
                        "https://deep.test/USERS",
                        AuthKind::Inherit,
                    )],
                },
            ],
        },
        Item {
            path: "collections/Other".into(),
            node: Node::Collection(CollectionFile {
                name: "Other".into(),
                auth: inherit(),
                ..CollectionFile::default()
            }),
            items: vec![request(
                "collections/Other/Ping.toml",
                "Ping",
                "https://other.test/ping",
                AuthKind::Inherit,
            )],
        },
    ]
}

#[test]
fn f09_ac3_paths() {
    assert_eq!(file_name("collections/API/users/List.toml"), "List.toml");
    assert_eq!(file_name("top"), "top");
    assert_eq!(
        parent("collections/API/users/List.toml"),
        "collections/API/users"
    );
    assert_eq!(parent("top"), "");
}

#[test]
fn f09_ac3_items_know_kind_name_and_problem() {
    let t = tree();
    assert_eq!(t[0].kind(), ItemKind::Collection);
    assert_eq!(t[0].name(), "API");
    assert_eq!(t[0].problem(), None);
    let broken = &t[0].items[2];
    assert_eq!(broken.kind(), ItemKind::Folder);
    assert_eq!(broken.name(), "broken");
    assert_eq!(
        broken.problem().as_deref(),
        Some("Could not read collections/API/broken: expected `=`")
    );
    let broken_request = Item {
        path: "collections/API/Bad.toml".into(),
        node: Node::Broken {
            kind: ItemKind::Request,
            reason: "r".into(),
        },
        items: vec![],
    };
    assert_eq!(broken_request.name(), "Bad");
    assert_eq!(broken_request.kind(), ItemKind::Request);
    assert_eq!(t[0].items[1].kind(), ItemKind::Request);
    assert_eq!(t[0].items[0].kind(), ItemKind::Folder);
    assert_eq!(ItemKind::Folder.to_string(), "folder");
    assert_eq!(ItemKind::Collection.to_string(), "collection");
    assert_eq!(ItemKind::Request.to_string(), "request");
}

#[test]
fn f09_ac3_arrange_by_name_or_order_file() {
    let items = vec![
        request("c/b.toml", "beta", "", AuthKind::None),
        request("c/A.toml", "Alpha", "", AuthKind::None),
        folder("c/z", "zeta", inherit(), vec![]),
        request("c/d.toml", "delta", "", AuthKind::None),
        request("c/d2.toml", "delta", "", AuthKind::None),
    ];
    let names = |items: &[Item]| -> Vec<String> {
        items
            .iter()
            .map(|i| file_name(&i.path).to_owned())
            .collect()
    };
    assert_eq!(
        names(&arrange(items.clone(), &[])),
        ["A.toml", "b.toml", "d.toml", "d2.toml", "z"]
    );
    let order = vec!["z".to_owned(), "missing".to_owned(), "d.toml".to_owned()];
    assert_eq!(
        names(&arrange(items, &order)),
        ["z", "d.toml", "A.toml", "b.toml", "d2.toml"]
    );
}

#[test]
fn f09_ac3_find_and_ancestors() {
    let t = tree();
    let promote = "collections/API/users/admins/Promote.toml";
    assert_eq!(find(&t, promote).unwrap().name(), "Promote");
    assert_eq!(find(&t, "collections/Other").unwrap().name(), "Other");
    assert!(find(&t, "collections/API/nope.toml").is_none());
    let chain: Vec<&str> = ancestors(&t, promote).iter().map(|i| i.name()).collect();
    assert_eq!(chain, ["API", "Users", "Admins"]);
    assert!(ancestors(&t, "collections/API").is_empty());
    assert!(ancestors(&t, "nope").is_empty());
}

#[test]
fn f04_ac6_inherit_uses_the_nearest_parent_with_auth() {
    let t = tree();
    let (auth, source) = inherited_auth(&t, "collections/API/users/admins/Promote.toml").unwrap();
    assert_eq!(auth.token, "admin");
    assert_eq!(
        source,
        AuthSource {
            kind: ItemKind::Folder,
            name: "Admins".into()
        }
    );
    // Users is Inherit, so List gets the collection's.
    let (auth, source) = inherited_auth(&t, "collections/API/users/List.toml").unwrap();
    assert_eq!(auth.token, "col");
    assert_eq!(source.kind, ItemKind::Collection);
    // Broken folders are skipped.
    let (auth, _) = inherited_auth(&t, "collections/API/broken/Deep.toml").unwrap();
    assert_eq!(auth.token, "col");
    // A folder asks for its own parents.
    let (auth, _) = inherited_auth(&t, "collections/API/users").unwrap();
    assert_eq!(auth.token, "col");
    // A collection's Inherit means No Auth.
    let (auth, source) = inherited_auth(&t, "collections/Other/Ping.toml").unwrap();
    assert_eq!(auth.kind, AuthKind::None);
    assert_eq!(source.name, "Other");
    assert!(inherited_auth(&t, "not/in/tree.toml").is_none());
    assert!(inherited_auth(&t, "collections/API").is_none());
}

#[test]
fn f04_ac6_inherit_notes() {
    let t = tree();
    let found = inherited_auth(&t, "collections/API/users/admins/Promote.toml");
    assert_eq!(
        inherit_note(found.as_ref()),
        "This request is using Bearer Token from folder \"Admins\"."
    );
    let found = inherited_auth(&t, "collections/Other/Ping.toml");
    assert_eq!(
        inherit_note(found.as_ref()),
        "This request is using No Auth from collection \"Other\"."
    );
    assert_eq!(
        inherit_note(None),
        "This request is not saved in a collection, so it has no parent to inherit auth from."
    );
}

#[test]
fn f08_ac2_collection_variables_by_path() {
    let t = tree();
    let vars = collection_variables(&t, "collections/API/users/List.toml");
    assert_eq!(vars[0].key, "base");
    assert!(collection_variables(&t, "collections/Other/Ping.toml").is_empty());
    assert!(collection_variables(&t, "nowhere").is_empty());
    assert_eq!(collection_variables(&t, "collections/API").len(), 1);
}

#[test]
fn f09_ac13_search_by_name_or_url() {
    let t = tree();
    let hits = search(&t, "USERS");
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "collections/API/users/List.toml",
            "collections/API/broken/Deep.toml"
        ]
    );
    assert_eq!(hits[0].name, "List users");
    assert_eq!(hits[0].method, "GET");
    assert_eq!(hits[0].url, "{{base}}/users");
    assert_eq!(hits[0].location, ["API", "Users"]);
    assert_eq!(hits[1].location, ["API", "broken"]);
    assert_eq!(search(&t, "ping").len(), 1);
    assert!(search(&t, "").is_empty());
    assert!(search(&t, "zzz").is_empty());
}
