//! Variables: F08-AC1..AC7, and the calendar helpers behind them.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_core::auth::{AuthDraft, AuthKind};
use ferry_core::body::{BodyDraft, BodyMode, FormField};
use ferry_core::draft::RequestDraft;
use ferry_core::kv::KeyValue;
use ferry_core::time::{self, Date};
use ferry_core::variables::{
    Ambient, MAX_DEPTH, MAX_GROWTH, Reference, Resolver, ScopeKind, VarError, Variable, Vars,
    dynamic, is_dynamic, references, uuid_v4,
};
use proptest::prelude::*;

/// A fixed clock and a counter for randomness.
struct Fixed {
    now: SystemTime,
    next: u64,
}

impl Fixed {
    fn new() -> Self {
        Self {
            // 2026-10-17T19:07:20.123Z
            now: UNIX_EPOCH + Duration::from_millis(1_792_264_040_123),
            next: 0,
        }
    }
}

impl Ambient for Fixed {
    fn now(&self) -> SystemTime {
        self.now
    }
    fn random(&mut self) -> u64 {
        self.next += 1;
        self.next.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }
}

fn names(text: &str) -> Vec<String> {
    references(text).into_iter().map(|r| r.name).collect()
}

fn resolve(vars: &Vars<'_>, text: &str) -> (String, Result<(), VarError>) {
    let mut ambient = Fixed::new();
    let mut r = Resolver::new(vars, &mut ambient);
    let out = r.resolve(text);
    (out, r.finish())
}

fn off(key: &str, value: &str) -> Variable {
    Variable {
        enabled: false,
        ..Variable::new(key, value)
    }
}

// --- F08-AC1 syntax --------------------------------------------------------

#[test]
fn f08_ac1_finds_references_with_ranges() {
    let text = "{{base}}/users/{{id}}?q={{ spaced name }}";
    assert_eq!(
        references(text),
        [
            Reference {
                name: "base".into(),
                range: 0..8
            },
            Reference {
                name: "id".into(),
                range: 15..21
            },
            Reference {
                name: " spaced name ".into(),
                range: 24..41
            },
        ]
    );
}

#[test]
fn f08_ac1_incomplete_or_empty_references_are_literal() {
    for text in [
        "{{}}", "{x}", "{{a", "a}}", "{ {a}}", "{{a}", "{{a\nb}}", "{{a\rb}}", "",
    ] {
        assert_eq!(names(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn f08_ac1_braces_inside_names_are_not_allowed() {
    // The innermost complete reference wins.
    assert_eq!(names("{{{a}}}"), ["a"]);
    assert_eq!(names("{{a{{b}}"), ["b"]);
    assert_eq!(names("{{a}}{{b}}"), ["a", "b"]);
    assert_eq!(names("{{é ü}}"), ["é ü"]);
}

#[test]
fn f08_ac1_substituted_values_are_not_rescanned_as_new_syntax() {
    // `{{` + `x}}` assembled by two values is not a reference.
    let vars_list = [Variable::new("open", "{{"), Variable::new("rest", "x}}")];
    let vars = Vars::new().with(ScopeKind::Global, &vars_list);
    let (out, result) = resolve(&vars, "{{open}}{{rest}}");
    assert_eq!(out, "{{x}}");
    assert_eq!(result, Ok(()));
}

// --- F08-AC2 precedence ------------------------------------------------------

#[test]
fn f08_ac2_environment_beats_collection_beats_global() {
    let env = [Variable::new("a", "env")];
    let col = [Variable::new("a", "col"), Variable::new("b", "col")];
    let glob = [
        Variable::new("a", "glob"),
        Variable::new("b", "glob"),
        Variable::new("c", "glob"),
    ];
    let vars = Vars::new()
        .with(ScopeKind::Environment, &env)
        .with(ScopeKind::Collection, &col)
        .with(ScopeKind::Global, &glob);
    assert_eq!(resolve(&vars, "{{a}} {{b}} {{c}}").0, "env col glob");
    let (var, scope) = vars.lookup("b").unwrap();
    assert_eq!((var.value.as_str(), scope), ("col", ScopeKind::Collection));
    assert!(vars.lookup("zzz").is_none());
}

#[test]
fn f08_ac2_disabled_variables_are_ignored() {
    let env = [off("a", "disabled env")];
    let glob = [Variable::new("a", "glob"), off("b", "x")];
    let vars = Vars::new()
        .with(ScopeKind::Environment, &env)
        .with(ScopeKind::Global, &glob);
    assert_eq!(resolve(&vars, "{{a}}").0, "glob");
    assert_eq!(
        resolve(&vars, "{{b}}").1,
        Err(VarError::Unresolved(vec!["b".into()]))
    );
}

#[test]
fn f08_ac2_first_enabled_definition_within_a_scope_wins() {
    let env = [
        off("a", "1"),
        Variable::new("a", "2"),
        Variable::new("a", "3"),
    ];
    let vars = Vars::new().with(ScopeKind::Environment, &env);
    assert_eq!(resolve(&vars, "{{a}}").0, "2");
}

#[test]
fn f08_ac2_names_match_exactly() {
    let env = [Variable::new("Base", "x")];
    let vars = Vars::new().with(ScopeKind::Environment, &env);
    assert!(vars.lookup("base").is_none());
    assert!(vars.lookup(" Base").is_none());
}

#[test]
fn f08_ac2_scope_names() {
    assert_eq!(ScopeKind::Environment.to_string(), "Environment");
    assert_eq!(ScopeKind::Collection.to_string(), "Collection");
    assert_eq!(ScopeKind::Global.to_string(), "Global");
}

// --- F08-AC3 where variables apply --------------------------------------------

fn full_vars() -> [Variable; 4] {
    [
        Variable::new("base", "https://api.test"),
        Variable::new("token", "s3cret"),
        Variable::new("name", "X-Trace"),
        Variable::new("v", "42"),
    ]
}

#[test]
fn f08_ac3_resolves_every_sent_field_and_keeps_the_draft() {
    let list = full_vars();
    let vars = Vars::new().with(ScopeKind::Environment, &list);
    let mut d = RequestDraft {
        url: "{{base}}/users?id={{v}}".into(),
        headers: vec![KeyValue::new("{{name}}", "v={{v}}")],
        ..RequestDraft::default()
    };
    d.body = BodyDraft {
        mode: BodyMode::Raw,
        raw: r#"{"id": {{v}}}"#.into(),
        ..BodyDraft::default()
    };
    d.auth = AuthDraft {
        kind: AuthKind::Bearer,
        token: "{{token}}".into(),
        ..AuthDraft::default()
    };
    let original = d.clone();
    let mut ambient = Fixed::new();
    let mut r = Resolver::new(&vars, &mut ambient);
    let resolved = d.resolve(&mut r);
    assert_eq!(r.finish(), Ok(()));
    assert_eq!(d, original, "the draft keeps its placeholders");
    let request = resolved.build().unwrap();
    assert_eq!(request.url.as_str(), "https://api.test/users?id=42");
    let headers: Vec<(String, String)> = request
        .headers
        .into_iter()
        .map(|h| (h.name, h.value))
        .collect();
    assert!(headers.contains(&("X-Trace".into(), "v=42".into())));
    assert!(headers.contains(&("Authorization".into(), "Bearer s3cret".into())));
    assert_eq!(resolved.body.raw, r#"{"id": 42}"#);
}

#[test]
fn f08_ac3_resolves_form_bodies_and_all_auth_fields() {
    let list = full_vars();
    let vars = Vars::new().with(ScopeKind::Environment, &list);
    let mut d = RequestDraft {
        url: "{{base}}".into(),
        ..RequestDraft::default()
    };
    d.body.urlencoded = vec![KeyValue::new("{{name}}", "{{v}}")];
    d.body.form = vec![FormField {
        row: KeyValue::new("f{{v}}", "{{token}}"),
        file: None,
    }];
    d.auth = AuthDraft {
        kind: AuthKind::Basic,
        username: "{{name}}".into(),
        password: "{{token}}".into(),
        token: "{{v}}".into(),
        key: "{{name}}".into(),
        value: "{{v}}".into(),
        ..AuthDraft::default()
    };
    for mode in [BodyMode::UrlEncoded, BodyMode::FormData] {
        d.body.mode = mode;
        let mut ambient = Fixed::new();
        let mut r = Resolver::new(&vars, &mut ambient);
        let out = d.resolve(&mut r);
        assert_eq!(r.finish(), Ok(()));
        match mode {
            BodyMode::UrlEncoded => {
                assert_eq!(out.body.urlencoded[0].key, "X-Trace");
                assert_eq!(out.body.urlencoded[0].value, "42");
            }
            _ => {
                assert_eq!(out.body.form[0].row.key, "f42");
                assert_eq!(out.body.form[0].row.value, "s3cret");
            }
        }
        assert_eq!(out.auth.username, "X-Trace");
        assert_eq!(out.auth.password, "s3cret");
    }
    for kind in [AuthKind::Bearer, AuthKind::ApiKey] {
        d.auth.kind = kind;
        let mut ambient = Fixed::new();
        let mut r = Resolver::new(&vars, &mut ambient);
        let out = d.resolve(&mut r);
        assert_eq!(r.finish(), Ok(()));
        assert_eq!(
            out.auth.token,
            if kind == AuthKind::Bearer {
                "42"
            } else {
                "{{v}}"
            }
        );
        if kind == AuthKind::ApiKey {
            assert_eq!(
                (out.auth.key.as_str(), out.auth.value.as_str()),
                ("X-Trace", "42")
            );
        }
    }
}

#[test]
fn f08_ac3_parts_that_are_not_sent_do_not_need_their_variables() {
    let vars = Vars::new();
    let mut d = RequestDraft {
        url: "http://a.test/".into(),
        headers: vec![KeyValue {
            enabled: false,
            ..KeyValue::new("X-{{off}}", "{{off}}")
        }],
        ..RequestDraft::default()
    };
    d.headers.push(KeyValue {
        description: "{{only in a description}}".into(),
        ..KeyValue::new("X-Ok", "1")
    });
    d.body.raw = "{{not sent: mode is none}}".into();
    d.auth.token = "{{not sent: no auth}}".into();
    let mut ambient = Fixed::new();
    let mut r = Resolver::new(&vars, &mut ambient);
    let _ = d.resolve(&mut r);
    assert_eq!(r.finish(), Ok(()));
    assert!(d.variable_names().is_empty());
}

#[test]
fn f08_ac3_variable_names_lists_sent_references_once_in_order() {
    let mut d = RequestDraft {
        url: "{{base}}/{{id}}/{{base}}".into(),
        headers: vec![KeyValue::new("{{h}}", "{{id}} {{token}}")],
        ..RequestDraft::default()
    };
    d.body.mode = BodyMode::Raw;
    d.body.raw = "{{b}}".into();
    d.auth.kind = AuthKind::Bearer;
    d.auth.token = "{{token}} {{z}}".into();
    assert_eq!(d.variable_names(), ["base", "id", "h", "token", "b", "z"]);
}

// --- F08-AC4 nesting -------------------------------------------------------------

#[test]
fn f08_ac4_values_resolve_recursively() {
    let list = [
        Variable::new("url", "{{scheme}}://{{host}}"),
        Variable::new("scheme", "https"),
        Variable::new("host", "{{sub}}.test"),
        Variable::new("sub", "api"),
    ];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(
        resolve(&vars, "{{url}}/x"),
        ("https://api.test/x".into(), Ok(()))
    );
}

#[test]
fn f08_ac4_self_reference_is_a_cycle() {
    let list = [Variable::new("a", "x{{a}}")];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(resolve(&vars, "{{a}}").1, Err(VarError::Cycle("a".into())));
}

#[test]
fn f08_ac4_indirect_cycle_names_the_repeated_variable() {
    let list = [
        Variable::new("a", "{{b}}"),
        Variable::new("b", "{{c}}"),
        Variable::new("c", "{{b}}"),
    ];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(resolve(&vars, "{{a}}").1, Err(VarError::Cycle("b".into())));
}

/// Ten levels of ten references each: `{{l9}}` would expand to 10^10
/// copies of `x` (FND-017).
fn laughs() -> Vec<Variable> {
    let mut list = vec![Variable::new("l0", "xxxxxxxxxx")];
    for level in 1..10 {
        list.push(Variable::new(
            format!("l{level}"),
            format!("{{{{l{}}}}}", level - 1).repeat(10),
        ));
    }
    list
}

#[test]
fn f08_ac4_exponential_expansion_stops_at_10_mb() {
    let list = laughs();
    let vars = Vars::new().with(ScopeKind::Global, &list);
    let started = std::time::Instant::now();
    let (out, result) = resolve(&vars, "{{l9}}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    assert!(
        out.len() <= MAX_GROWTH + 64,
        "output stays bounded: {} bytes",
        out.len()
    );
    let Err(VarError::TooLarge(name)) = &result else {
        panic!("{result:?}");
    };
    assert!(name.starts_with('l'), "{name}");
    assert_eq!(
        VarError::TooLarge("l3".into()).to_string(),
        "Variables expand to more than 10 MB at {{l3}}"
    );
}

#[test]
fn f08_ac4_resolving_stops_at_the_reference_that_grew_too_much() {
    let mut list = laughs();
    list.push(Variable::new("tail", "TAIL"));
    let vars = Vars::new().with(ScopeKind::Global, &list);
    let mut ambient = Fixed::new();
    let mut r = Resolver::new(&vars, &mut ambient);
    // Nothing after the overflow is expanded, at any level.
    let out = r.resolve("head {{l9}} {{tail}}");
    assert_eq!(out, "head ");
    // The next text, such as the next header, starts afresh.
    assert_eq!(r.resolve("{{l0}} {{tail}}"), "xxxxxxxxxx TAIL");
    assert!(matches!(r.finish(), Err(VarError::TooLarge(_))));
}

#[test]
fn f08_ac4_growth_is_measured_against_the_text() {
    // A large text with a small variable is fine.
    let list = [Variable::new("v", "value")];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    let big = "a".repeat(MAX_GROWTH + 1000) + "{{v}}";
    let (out, result) = resolve(&vars, &big);
    assert_eq!(result, Ok(()));
    assert!(out.ends_with("avalue"));
    // Exactly 10 MB of growth is allowed; one byte more is not.
    let exact = [Variable::new("v", "x".repeat(MAX_GROWTH + 5))];
    let vars = Vars::new().with(ScopeKind::Global, &exact);
    assert_eq!(
        resolve(&vars, "{{v}}").1,
        Ok(()),
        "growth of exactly MAX_GROWTH"
    );
    let over = [Variable::new("v", "x".repeat(MAX_GROWTH + 6))];
    let vars = Vars::new().with(ScopeKind::Global, &over);
    assert_eq!(
        resolve(&vars, "{{v}}").1,
        Err(VarError::TooLarge("v".into()))
    );
}

#[test]
fn f08_ac4_ten_levels_resolve_and_eleven_do_not() {
    // v0 -> v1 -> ... -> v{n-1} -> "end": n variables deep.
    let chain = |n: usize| -> Vec<Variable> {
        (0..n)
            .map(|i| {
                let value = if i + 1 == n {
                    "end".to_owned()
                } else {
                    format!("{{{{v{}}}}}", i + 1)
                };
                Variable::new(format!("v{i}"), value)
            })
            .collect()
    };
    let ok = chain(MAX_DEPTH);
    let vars = Vars::new().with(ScopeKind::Global, &ok);
    assert_eq!(resolve(&vars, "{{v0}}"), ("end".into(), Ok(())));
    let deep = chain(MAX_DEPTH + 1);
    let vars = Vars::new().with(ScopeKind::Global, &deep);
    assert_eq!(
        resolve(&vars, "{{v0}}").1,
        Err(VarError::TooDeep(format!("v{MAX_DEPTH}")))
    );
}

#[test]
fn f08_ac4_the_same_variable_twice_side_by_side_is_not_a_cycle() {
    let list = [Variable::new("a", "{{b}}{{b}}"), Variable::new("b", "x")];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(resolve(&vars, "{{a}}{{a}}"), ("xxxx".into(), Ok(())));
}

#[test]
fn f08_ac4_cycle_is_reported_before_unresolved_names() {
    let list = [Variable::new("a", "{{a}}")];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(
        resolve(&vars, "{{missing}} {{a}}").1,
        Err(VarError::Cycle("a".into()))
    );
}

// --- F08-AC5 unresolved ----------------------------------------------------------

#[test]
fn f08_ac5_unresolved_names_once_in_order_and_kept_verbatim() {
    let list = [
        Variable::new("known", "k"),
        Variable::new("nested", "{{inner}}"),
    ];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    let mut ambient = Fixed::new();
    let mut r = Resolver::new(&vars, &mut ambient);
    assert_eq!(r.resolve("{{b}}-{{known}}-{{a}}"), "{{b}}-k-{{a}}");
    assert_eq!(r.resolve("{{a}}{{nested}}{{b}}"), "{{a}}{{inner}}{{b}}");
    assert_eq!(
        r.finish(),
        Err(VarError::Unresolved(vec![
            "b".into(),
            "a".into(),
            "inner".into()
        ]))
    );
}

#[test]
fn f08_ac5_error_messages() {
    assert_eq!(
        VarError::Unresolved(vec!["a".into()]).to_string(),
        "Unresolved variable {{a}}"
    );
    assert_eq!(
        VarError::Unresolved(vec!["a".into(), "b c".into()]).to_string(),
        "Unresolved variables {{a}}, {{b c}}"
    );
    assert_eq!(
        VarError::Cycle("a".into()).to_string(),
        "Variable {{a}} refers to itself"
    );
    assert_eq!(
        VarError::TooDeep("v".into()).to_string(),
        "Variables are nested more than 10 levels deep at {{v}}"
    );
}

// --- F08-AC7 dynamic variables ---------------------------------------------------

#[test]
fn f08_ac7_timestamps_use_the_injected_clock() {
    let mut a = Fixed::new();
    assert_eq!(dynamic("$timestamp", &mut a).unwrap(), "1792264040");
    assert_eq!(
        dynamic("$isoTimestamp", &mut a).unwrap(),
        "2026-10-17T19:07:20.123Z"
    );
}

#[test]
fn f08_ac7_guids_are_v4_and_fresh_per_occurrence() {
    let vars = Vars::new();
    let (out, result) = resolve(&vars, "{{$guid}} {{$randomUUID}} {{$guid}}");
    assert_eq!(result, Ok(()));
    let ids: Vec<&str> = out.split(' ').collect();
    assert_eq!(ids.len(), 3);
    for id in &ids {
        assert_eq!(id.len(), 36, "{id}");
        assert_eq!(&id[14..15], "4", "{id}");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"), "{id}");
        assert!(
            id.chars()
                .all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }
    assert_ne!(ids[0], ids[2]);
}

#[test]
fn f08_ac7_random_int_is_between_0_and_1000() {
    struct Max;
    impl Ambient for Max {
        fn now(&self) -> SystemTime {
            UNIX_EPOCH
        }
        fn random(&mut self) -> u64 {
            u64::MAX
        }
    }
    let mut a = Fixed::new();
    for _ in 0..200 {
        let n: u32 = dynamic("$randomInt", &mut a).unwrap().parse().unwrap();
        assert!(n <= 1000);
    }
    let n: u32 = dynamic("$randomInt", &mut Max).unwrap().parse().unwrap();
    assert!(n <= 1000);
    struct Zero;
    impl Ambient for Zero {
        fn now(&self) -> SystemTime {
            UNIX_EPOCH
        }
        fn random(&mut self) -> u64 {
            1000
        }
    }
    assert_eq!(dynamic("$randomInt", &mut Zero).unwrap(), "1000");
}

#[test]
fn f08_ac7_unknown_dynamic_names_are_unresolved() {
    let mut a = Fixed::new();
    assert_eq!(dynamic("$nope", &mut a), None);
    assert_eq!(dynamic("guid", &mut a), None);
    let vars = Vars::new();
    assert_eq!(
        resolve(&vars, "{{$nope}}").1,
        Err(VarError::Unresolved(vec!["$nope".into()]))
    );
}

#[test]
fn f08_ac7_is_dynamic_matches_what_dynamic_resolves() {
    let mut a = Fixed::new();
    for name in [
        "$guid",
        "$randomUUID",
        "$timestamp",
        "$isoTimestamp",
        "$randomInt",
    ] {
        assert!(is_dynamic(name), "{name}");
        assert!(dynamic(name, &mut a).is_some(), "{name}");
    }
    for name in ["$nope", "guid", "", "$GUID"] {
        assert!(!is_dynamic(name), "{name}");
        assert!(dynamic(name, &mut a).is_none(), "{name}");
    }
}

#[test]
fn f08_ac7_defined_variable_overrides_dynamic() {
    let list = [Variable::new("$guid", "fixed")];
    let vars = Vars::new().with(ScopeKind::Global, &list);
    assert_eq!(resolve(&vars, "{{$guid}}").0, "fixed");
}

#[test]
fn f08_ac7_uuid_v4_layout() {
    assert_eq!(uuid_v4([0, 0]), "00000000-0000-4000-8000-000000000000");
    assert_eq!(
        uuid_v4([u64::MAX, u64::MAX]),
        "ffffffff-ffff-4fff-bfff-ffffffffffff"
    );
    assert_eq!(
        uuid_v4([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]),
        "01234567-89ab-4def-bedc-ba9876543210"
    );
}

// --- calendar ------------------------------------------------------------------

#[test]
fn f08_ac7_iso_timestamps() {
    assert_eq!(time::iso8601_utc(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
    let leap = UNIX_EPOCH + Duration::from_millis(951_782_400_999); // 2000-02-29
    assert_eq!(time::iso8601_utc(leap), "2000-02-29T00:00:00.999Z");
    let before = UNIX_EPOCH - Duration::from_millis(1);
    assert_eq!(time::iso8601_utc(before), "1969-12-31T23:59:59.999Z");
    assert_eq!(time::unix_millis(before), -1);
    assert_eq!(time::unix_millis(leap), 951_782_400_999);
}

#[test]
fn f10_ac3_dates_at_an_offset() {
    // 2026-10-17T19:07:20Z
    let t = 1_792_264_040_123;
    assert_eq!(
        time::date_at(t, 0),
        Date {
            year: 2026,
            month: 10,
            day: 17
        }
    );
    assert_eq!(
        time::date_at(t, 5 * 3600),
        Date {
            year: 2026,
            month: 10,
            day: 18
        }
    );
    assert_eq!(
        time::date_at(t, -20 * 3600),
        Date {
            year: 2026,
            month: 10,
            day: 16
        }
    );
    assert_eq!(
        time::date_at(-1, 0),
        Date {
            year: 1969,
            month: 12,
            day: 31
        }
    );
    assert_eq!(
        time::date_at(0, 0),
        Date {
            year: 1970,
            month: 1,
            day: 1
        }
    );
    assert_eq!(
        time::date_at(951_782_400_000 + 86_400_000, 0),
        Date {
            year: 2000,
            month: 3,
            day: 1
        }
    );
}

#[test]
fn f10_ac3_month_names() {
    assert_eq!(time::month_name(1), "January");
    assert_eq!(time::month_name(10), "October");
    assert_eq!(time::month_name(12), "December");
    assert_eq!(time::month_name(0), "");
    assert_eq!(time::month_name(13), "");
}

proptest! {
    #[test]
    fn f10_ac3_prop_dates_match_a_day_count(days in -1_000_000i64..1_000_000, ms in 0i64..86_400_000) {
        let date = time::date_at(days * 86_400_000 + ms, 0);
        let next = time::date_at((days + 1) * 86_400_000 + ms, 0);
        prop_assert!(next > date);
        prop_assert!((1..=12).contains(&date.month));
        prop_assert!((1..=31).contains(&date.day));
        if next.month == date.month {
            prop_assert_eq!(next.day, date.day + 1);
        } else {
            prop_assert_eq!(next.day, 1);
        }
    }

    /// Resolving text with no references returns it unchanged.
    #[test]
    fn f08_ac1_prop_text_without_references_is_unchanged(text in "[^{}]*|[a-z{} ]{0,20}") {
        prop_assume!(references(&text).is_empty());
        let vars = Vars::new();
        let (out, result) = resolve(&vars, &text);
        prop_assert_eq!(out, text);
        prop_assert_eq!(result, Ok(()));
    }

    /// Resolution is idempotent when values have no braces and name no
    /// defined variable.
    #[test]
    fn f08_ac1_prop_resolution_is_idempotent(
        text in "[a-c{} ]{0,30}",
        values in proptest::collection::vec("[d-z/:. ]{0,8}", 3),
    ) {
        let list: Vec<Variable> = ["a", "b", "c"].iter().zip(&values).map(|(k, v)| Variable::new(*k, v.clone())).collect();
        let vars = Vars::new().with(ScopeKind::Global, &list);
        let (once, _) = resolve(&vars, &text);
        let (twice, _) = resolve(&vars, &once);
        prop_assert_eq!(once, twice);
    }

    /// Every reference's range slices exactly `{{name}}`.
    #[test]
    fn f08_ac1_prop_ranges_slice_the_reference(text in "[a{}\n ]{0,40}") {
        for r in references(&text) {
            prop_assert_eq!(&text[r.range.clone()], format!("{{{{{}}}}}", r.name));
            prop_assert!(!r.name.is_empty());
            let forbidden = ['{', '}', '\n', '\r'];
            let clean = !r.name.contains(forbidden);
            prop_assert!(clean);
        }
    }
}
