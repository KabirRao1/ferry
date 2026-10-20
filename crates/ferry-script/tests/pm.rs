//! The `pm` API: F15-AC3..AC13 at the script level, with a fake host.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ferry_core::scripts::Phase;
use ferry_core::variables::{Ambient, Origin, Variable};
use ferry_script::{
    Host, Info, Input, Level, Location, Outcome, Script, ScriptBody, ScriptRequest, ScriptResponse,
    TestStatus, Variables, run,
};

struct Fake {
    lines: Vec<(Level, String)>,
    sent: Vec<ScriptRequest>,
    reply: Result<ScriptResponse, String>,
    delay: Duration,
}

impl Fake {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            sent: Vec::new(),
            reply: Ok(response(200, "{\"id\": 7}")),
            delay: Duration::ZERO,
        }
    }
}

impl Ambient for Fake {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_792_264_040)
    }
    fn random(&mut self) -> u64 {
        42
    }
}

impl Host for Fake {
    fn send(&mut self, request: ScriptRequest) -> Result<ScriptResponse, String> {
        self.sent.push(request);
        std::thread::sleep(self.delay);
        self.reply.clone()
    }
    fn console(&mut self, level: Level, text: String) {
        self.lines.push((level, text));
    }
    fn ambient(&mut self) -> &mut dyn Ambient {
        self
    }
}

fn response(code: u16, body: &str) -> ScriptResponse {
    ScriptResponse {
        code,
        status: if code == 200 { "OK" } else { "Not Found" }.into(),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("X-Id".into(), "7".into()),
        ],
        body: body.into(),
        response_time: 85,
        response_size: body.len() as u64,
    }
}

fn request() -> ScriptRequest {
    ScriptRequest {
        method: "POST".into(),
        url: "{{base}}/users".into(),
        headers: vec![("Accept".into(), "*/*".into())],
        body: ScriptBody {
            mode: "raw".into(),
            raw: "{}".into(),
        },
    }
}

fn variables() -> Variables {
    Variables {
        environment: Some(vec![
            Variable::new("base", "https://env.test"),
            Variable::new("token", "env-token"),
        ]),
        environment_name: Some("Dev".into()),
        globals: vec![Variable::new("token", "global"), Variable::new("g", "1")],
        collection: Some(vec![Variable::new("base", "https://col.test")]),
        ..Variables::default()
    }
}

/// The world a test's scripts run against.
struct World {
    request: ScriptRequest,
    response: Option<ScriptResponse>,
    variables: Variables,
    info: Info,
    host: Rc<RefCell<Fake>>,
    cancel: Arc<AtomicBool>,
}

impl World {
    fn new() -> Self {
        Self {
            request: request(),
            response: Some(response(200, "{\"id\": 7, \"tags\": [\"a\", \"b\"]}")),
            variables: variables(),
            info: Info {
                request_name: "Create user".into(),
                iteration: 0,
                iteration_count: 1,
            },
            host: Rc::new(RefCell::new(Fake::new())),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn run(&mut self, phase: Phase, scripts: &[(Location, &str)]) -> Outcome {
        let scripts: Vec<Script> = scripts
            .iter()
            .map(|(location, source)| Script {
                location: location.clone(),
                source: (*source).to_owned(),
            })
            .collect();
        let host: Rc<RefCell<dyn Host>> = self.host.clone();
        let response = match phase {
            Phase::PreRequest => None,
            Phase::PostResponse => self.response.as_ref(),
        };
        run(
            &scripts,
            Input {
                phase,
                request: &mut self.request,
                response,
                variables: &mut self.variables,
                info: &self.info,
                cancel: &self.cancel,
            },
            &host,
        )
    }

    fn pre(&mut self, source: &str) -> Outcome {
        self.run(Phase::PreRequest, &[(Location::Request, source)])
    }

    fn post(&mut self, source: &str) -> Outcome {
        self.run(Phase::PostResponse, &[(Location::Request, source)])
    }

    /// The single test a post-response script records.
    fn verdict(&mut self, body: &str) -> (TestStatus, Option<String>) {
        let outcome = self.post(&format!("pm.test('t', () => {{ {body} }});"));
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.tests.len(), 1, "{outcome:?}");
        (outcome.tests[0].status, outcome.tests[0].error.clone())
    }

    fn passes(&mut self, body: &str) {
        let (status, error) = self.verdict(body);
        assert_eq!(status, TestStatus::Passed, "{body}: {error:?}");
    }

    fn fails(&mut self, body: &str, message: &str) {
        let (status, error) = self.verdict(body);
        assert_eq!(status, TestStatus::Failed, "{body}");
        assert_eq!(error.as_deref(), Some(message), "{body}");
    }

    fn logged(&self) -> Vec<String> {
        self.host
            .borrow()
            .lines
            .iter()
            .map(|(_, t)| t.clone())
            .collect()
    }
}

// --- F15-AC2 and AC3: order and failures ----------------------------------------------

#[test]
fn f15_ac2_scripts_run_in_order_and_share_variables() {
    let mut w = World::new();
    let outcome = w.run(
        Phase::PreRequest,
        &[
            (Location::Collection, "pm.variables.set('trail', 'c');"),
            (
                Location::Folder("Users".into()),
                "pm.variables.set('trail', pm.variables.get('trail') + 'f');",
            ),
            (Location::Request, "   "),
            (
                Location::Request,
                "pm.variables.set('trail', pm.variables.get('trail') + 'r');",
            ),
        ],
    );
    assert_eq!(outcome, Outcome::default());
    assert_eq!(w.variables.local, [Variable::new("trail", "cfr")]);
}

#[test]
fn f15_ac3_a_throw_stops_the_phase_and_names_the_location() {
    let mut w = World::new();
    let outcome = w.run(
        Phase::PreRequest,
        &[
            (Location::Collection, "pm.variables.set('a', '1');"),
            (Location::Folder("Users".into()), "throw new Error('boom');"),
            (Location::Request, "pm.variables.set('b', '2');"),
        ],
    );
    assert_eq!(
        outcome.error.as_deref(),
        Some("Pre-request script failed in folder Users: Error: boom")
    );
    assert_eq!(w.variables.local, [Variable::new("a", "1")]);
    let outcome = w.run(
        Phase::PostResponse,
        &[(Location::Collection, "notDefined();")],
    );
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "Post-response script failed in the collection: ReferenceError: notDefined is not defined"
        )
    );
    assert_eq!(
        w.post("throw 'plain text';").error.as_deref(),
        Some("Post-response script failed in the request: plain text")
    );
}

#[test]
fn f15_ac3_syntax_errors_name_their_line() {
    let mut w = World::new();
    let outcome = w.pre("let a = 1;\nlet b = ;\n");
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "Pre-request script failed in the request: SyntaxError: unexpected token in expression: ';' (line 2)"
        )
    );
}

#[test]
fn f15_ac3_tests_before_a_throw_still_count() {
    let mut w = World::new();
    let outcome = w.post("pm.test('first', () => {});\nthrow new Error('later');");
    assert_eq!(outcome.tests.len(), 1);
    assert_eq!(outcome.tests[0].status, TestStatus::Passed);
    assert!(outcome.error.is_some());
}

#[test]
fn f15_ac3_a_failed_pre_request_script_leaves_the_request_alone() {
    let mut w = World::new();
    w.pre("pm.request.method = 'PUT'; throw new Error('no');");
    assert_eq!(w.request, request());
}

// --- F15-AC4 the sandbox -------------------------------------------------------------

#[test]
fn f15_ac4_no_modules_files_or_processes() {
    let mut w = World::new();
    assert_eq!(
        w.pre("require('fs');").error.as_deref(),
        Some(
            "Pre-request script failed in the request: Error: require is not available in Ferry scripts"
        )
    );
    let outcome = w.pre(
        "console.log(typeof process, typeof std, typeof os, typeof fetch, typeof XMLHttpRequest, typeof __ferry, typeof __ferry_input);",
    );
    assert_eq!(outcome.error, None);
    assert_eq!(
        w.logged(),
        ["undefined undefined undefined undefined undefined undefined undefined"]
    );
}

#[test]
fn f15_ac4_long_scripts_time_out() {
    let mut w = World::new();
    let started = Instant::now();
    let outcome = w.pre("pm.variables.set('a', '1'); while (true) {}");
    let took = started.elapsed();
    assert_eq!(
        outcome.error.as_deref(),
        Some("Pre-request script failed in the request: Script timed out after 5 s")
    );
    assert!(
        took >= Duration::from_secs(5) && took < Duration::from_secs(8),
        "{took:?}"
    );
    assert_eq!(
        w.variables.local[0].key, "a",
        "changes before the timeout stay"
    );
}

#[test]
fn f15_ac4_waiting_for_send_request_is_not_counted() {
    let mut w = World::new();
    w.host.borrow_mut().delay = Duration::from_millis(1200);
    // Five sends of 1.2 s each: 6 s of waiting, well under 5 s of work.
    let outcome = w.pre("for (let i = 0; i < 5; i++) pm.sendRequest('https://x.test', () => {});");
    assert_eq!(outcome.error, None);
    assert_eq!(w.host.borrow().sent.len(), 5);
}

#[test]
fn f15_ac4_memory_is_limited() {
    let mut w = World::new();
    let outcome = w.pre("let a = []; while (true) a.push('x'.repeat(100000));");
    assert_eq!(
        outcome.error.as_deref(),
        Some("Pre-request script failed in the request: Script ran out of memory (64 MB)")
    );
    // Deep recursion is an ordinary error, not a crash.
    let outcome = w.pre("function f() { f(); } f();");
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "Pre-request script failed in the request: RangeError: Maximum call stack size exceeded"
        )
    );
}

#[test]
fn f15_ac4_cancel_stops_a_script() {
    let mut w = World::new();
    let cancel = Arc::clone(&w.cancel);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = Instant::now();
    let outcome = w.pre("while (true) {}");
    stopper.join().unwrap();
    assert!(outcome.cancelled);
    assert_eq!(outcome.error.as_deref(), Some("Request cancelled"));
    assert!(started.elapsed() < Duration::from_secs(2));
}

// --- F15-AC5 saved scopes --------------------------------------------------------------

#[test]
fn f15_ac5_saved_scopes_get_set_unset() {
    let mut w = World::new();
    let outcome = w.pre(
        "pm.environment.set('token', 'fresh');
         pm.environment.set('n', 42);
         pm.environment.set('o', {a: [1]});
         pm.globals.set('g', 'two');
         pm.collectionVariables.set('base', 'https://mine.test');
         console.log(pm.environment.get('token'), pm.environment.has('n'), pm.environment.get('missing'));
         console.log(JSON.stringify(pm.environment.toObject()));
         console.log(pm.environment.name);
         pm.globals.unset('g');
         console.log(pm.globals.get('g'));",
    );
    assert_eq!(outcome.error, None);
    assert_eq!(
        w.logged(),
        [
            "fresh true undefined",
            r#"{"base":"https://env.test","token":"fresh","n":"42","o":"{\"a\":[1]}"}"#,
            "Dev",
            "1",
        ]
    );
    let env = w.variables.environment.as_ref().unwrap();
    assert_eq!(env[1].value, "fresh");
    assert_eq!(
        env[1].origin,
        Origin::Overridden {
            shared: "env-token".into()
        }
    );
    assert_eq!(env[2].origin, Origin::LocalOnly);
    assert!(
        w.variables.changed.environment
            && w.variables.changed.globals
            && w.variables.changed.collection
    );
    assert_eq!(
        w.variables.globals[1].value, "1",
        "unset brings the shared value back"
    );
}

#[test]
fn f15_ac5_scopes_that_do_not_exist_refuse_writes() {
    let mut w = World::new();
    w.variables.environment = None;
    w.variables.collection = None;
    assert_eq!(
        w.pre("pm.environment.set('a', '1');").error.as_deref(),
        Some("Pre-request script failed in the request: Error: No environment is selected")
    );
    assert_eq!(
        w.pre("pm.collectionVariables.set('a', '1');")
            .error
            .as_deref(),
        Some(
            "Pre-request script failed in the request: Error: This request is not in a collection"
        )
    );
    let outcome = w.pre(
        "console.log(pm.environment.get('a'), pm.environment.has('a'), JSON.stringify(pm.collectionVariables.toObject()));
         pm.environment.unset('a'); pm.collectionVariables.unset('a');",
    );
    assert_eq!(outcome.error, None);
    assert_eq!(w.logged(), ["undefined false {}"]);
    assert_eq!(w.variables.changed, ferry_script::Changed::default());
}

#[test]
fn f15_ac5_disabled_variables_are_invisible() {
    let mut w = World::new();
    w.variables.globals[1].enabled = false;
    w.pre("console.log(pm.globals.get('g'), pm.globals.has('g'), JSON.stringify(pm.globals.toObject()));");
    assert_eq!(w.logged(), [r#"undefined false {"token":"global"}"#]);
}

// --- F15-AC6 pm.variables ----------------------------------------------------------------

#[test]
fn f15_ac6_variables_resolve_in_precedence_order() {
    let mut w = World::new();
    w.variables.data = vec![
        Variable::new("row", "r1"),
        Variable::new("base", "https://data.test"),
    ];
    w.pre(
        "console.log(pm.variables.get('base'), pm.variables.get('token'), pm.variables.get('g'), pm.variables.get('row'));
         pm.variables.set('base', 'https://local.test');
         console.log(pm.variables.get('base'), pm.variables.has('row'), pm.variables.has('nope'));
         console.log(pm.variables.replaceIn('{{base}}/u/{{row}}/{{nope}}/{{$randomInt}}'));
         console.log(JSON.stringify(pm.variables.toObject()));
         pm.variables.unset('base');
         console.log(pm.variables.get('base'));",
    );
    assert_eq!(
        w.logged(),
        [
            "https://data.test env-token 1 r1",
            "https://local.test true false",
            "https://local.test/u/r1/{{nope}}/42",
            r#"{"token":"env-token","g":"1","base":"https://local.test","row":"r1"}"#,
            "https://data.test",
        ]
    );
    assert_eq!(
        w.variables.changed,
        ferry_script::Changed::default(),
        "never saved"
    );
}

// --- F15-AC7 pm.request -------------------------------------------------------------------

#[test]
fn f15_ac7_pre_request_scripts_change_the_request() {
    let mut w = World::new();
    let outcome = w.pre(
        "console.log(pm.request.method, pm.request.url.toString(), pm.request.headers.get('accept'), pm.request.body.mode, pm.request.body.raw);
         pm.request.method = 'PUT';
         pm.request.url = pm.request.url + '?x=1';
         pm.request.headers.add({key: 'X-A', value: 1});
         pm.request.headers.upsert({key: 'accept', value: 'application/json'});
         pm.request.headers.upsert({key: 'X-B', value: 'b'});
         pm.request.headers.remove('x-b');
         console.log(JSON.stringify(pm.request.headers.toObject()), pm.request.headers.has('X-A'), pm.request.headers.has('X-A', '2'));
         pm.request.body.update('{\"a\": 1}');",
    );
    assert_eq!(outcome.error, None);
    assert_eq!(
        w.logged(),
        [
            "POST {{base}}/users */* raw {}",
            r#"{"Accept":"application/json","X-A":"1"} true false"#,
        ]
    );
    assert_eq!(
        w.request,
        ScriptRequest {
            method: "PUT".into(),
            url: "{{base}}/users?x=1".into(),
            headers: vec![
                ("Accept".into(), "application/json".into()),
                ("X-A".into(), "1".into()),
            ],
            body: ScriptBody {
                mode: "raw".into(),
                raw: "{\"a\": 1}".into(),
            },
        }
    );
}

#[test]
fn f15_ac7_url_helpers() {
    let mut w = World::new();
    w.request.url = "https://user@api.test:8443/v1/users?x=1#f".into();
    w.pre("console.log(pm.request.url.getHost(), pm.request.url.getPath()); pm.request.url.update('https://b.test');");
    assert_eq!(w.logged(), ["api.test /v1/users"]);
    assert_eq!(w.request.url, "https://b.test");
}

#[test]
fn f15_ac7_post_response_request_is_read_only() {
    let mut w = World::new();
    let outcome =
        w.post("pm.request.method = 'PUT'; pm.request.headers.add({key: 'a', value: 'b'});");
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "Post-response script failed in the request: TypeError: These headers cannot be changed here"
        )
    );
    assert_eq!(w.request, request());
}

// --- F15-AC8 pm.response ------------------------------------------------------------------

#[test]
fn f15_ac8_response_fields() {
    let mut w = World::new();
    w.post(
        "const r = pm.response;
         console.log(r.code, r.status, r.responseTime, r.responseSize, r.headers.get('content-type'), r.headers.has('x-id', '7'));
         console.log(r.json().tags[1], r.text().length);",
    );
    assert_eq!(w.logged(), ["200 OK 85 29 application/json true", "b 29"]);
    let outcome = w.pre("console.log(typeof pm.response);");
    assert_eq!(outcome.error, None);
    assert_eq!(w.logged()[2], "undefined");
}

#[test]
fn f15_ac8_json_errors_say_why() {
    let mut w = World::new();
    w.response = Some(response(200, "not json"));
    w.fails(
        "pm.response.json();",
        "Response body is not valid JSON: unexpected token: 'not'",
    );
}

#[test]
fn f15_ac8_response_assertions() {
    let mut w = World::new();
    w.passes("pm.response.to.have.status(200);");
    w.passes("pm.response.to.have.status('OK');");
    w.passes("pm.response.to.not.have.status(404);");
    w.passes("pm.response.to.have.header('X-Id');");
    w.passes("pm.response.to.have.header('x-id', '7');");
    w.passes("pm.response.to.have.jsonBody();");
    w.passes("pm.response.to.be.ok; pm.response.to.be.success; pm.response.to.not.be.error;");
    w.fails(
        "pm.response.to.have.status(201);",
        "expected response to have status code 201 but got 200",
    );
    w.fails(
        "pm.response.to.have.status('Created');",
        "expected response to have status reason 'Created' but got 'OK'",
    );
    w.fails(
        "pm.response.to.not.have.status(200);",
        "expected response to not have status code 200",
    );
    w.fails(
        "pm.response.to.have.header('X-None');",
        "expected response to have header with key 'X-None'",
    );
    w.fails(
        "pm.response.to.have.header('X-Id', '8');",
        "expected 'X-Id' response header to be '8' but got '7'",
    );
    w.fails(
        "pm.response.to.have.body('x');",
        "expected response body to equal 'x' but got '{\"id\": 7, \"tags\": [\"a\", \"b\"]}'",
    );
    w.fails(
        "pm.response.to.be.clientError;",
        "expected response code to be 4XX but found 200",
    );
    w.fails(
        "pm.response.to.not.be.ok;",
        "expected response code to not be 200 but found 200",
    );
    w.response = Some(response(404, "x"));
    w.passes("pm.response.to.be.notFound; pm.response.to.be.clientError; pm.response.to.be.error;");
    w.fails(
        "pm.response.to.be.redirection;",
        "expected response code to be 3XX but found 404",
    );
    w.fails(
        "pm.response.to.be.serverError;",
        "expected response code to be 5XX but found 404",
    );
    w.fails(
        "pm.response.to.have.jsonBody();",
        "expected response body to be valid JSON but got error unexpected token: 'x'",
    );
}

// --- F15-AC9 pm.test and pm.expect ----------------------------------------------------------

#[test]
fn f15_ac9_tests_pass_fail_and_skip() {
    let mut w = World::new();
    let outcome = w.post(
        "pm.test('passes', () => {});
         pm.test('fails', () => { throw new Error('nope'); });
         pm.test('throws a value', () => { throw 7; });
         pm.test.skip('later', () => { throw new Error('never run'); });
         pm.test(3, () => {});",
    );
    let got: Vec<_> = outcome
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.status, t.error.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            ("passes", TestStatus::Passed, None),
            ("fails", TestStatus::Failed, Some("nope")),
            ("throws a value", TestStatus::Failed, Some("7")),
            ("later", TestStatus::Skipped, None),
            ("3", TestStatus::Passed, None),
        ]
    );
}

#[test]
fn f15_ac9_expect_passes() {
    let mut w = World::new();
    for body in [
        "pm.expect(1).to.equal(1); pm.expect(1).to.eq(1); pm.expect('a').equals('a');",
        "pm.expect({a: [1, {b: 2}]}).to.eql({a: [1, {b: 2}]}); pm.expect([1]).to.deep.equal([1]);",
        "pm.expect(NaN).to.eql(NaN); pm.expect(NaN).to.be.NaN;",
        "pm.expect(5).to.be.above(4).and.below(6); pm.expect(5).to.be.gt(4).lt(6);",
        "pm.expect(5).to.be.at.least(5); pm.expect(5).to.be.at.most(5); pm.expect(5).gte(5).lte(5);",
        "pm.expect(5).to.be.within(1, 5);",
        "pm.expect('s').to.be.a('string'); pm.expect([]).to.be.an('array'); pm.expect({}).to.be.an('object'); pm.expect(null).to.be.a('null');",
        "pm.expect('hello').to.include('ell'); pm.expect([1, 2]).to.contain(2); pm.expect({a: 1, b: 2}).to.include({a: 1});",
        "pm.expect([{a: 1}]).to.deep.include({a: 1});",
        "pm.expect(1).to.be.ok; pm.expect(0).to.not.be.ok; pm.expect(true).to.be.true; pm.expect(false).to.be.false;",
        "pm.expect(null).to.be.null; pm.expect(undefined).to.be.undefined; pm.expect(0).to.exist; pm.expect(null).to.not.exist;",
        "pm.expect('').to.be.empty; pm.expect([]).to.be.empty; pm.expect({}).to.be.empty; pm.expect([1]).to.not.be.empty;",
        "pm.expect([1, 2]).to.have.lengthOf(2); pm.expect('abc').to.have.length(3);",
        "pm.expect({a: {b: 1}}).to.have.property('a').that.has.property('b', 1);",
        "pm.expect({a: [1]}).to.have.deep.property('a', [1]);",
        "pm.expect('abc').to.match(/^a/); pm.expect(2).to.be.oneOf([1, 2]);",
        "pm.expect({a: 1, b: 2}).to.have.keys('a', 'b'); pm.expect({a: 1}).to.have.keys(['a']); pm.expect({a: 1}).to.not.have.keys('b');",
        "pm.expect(pm.response.json().id).to.equal(7);",
    ] {
        w.passes(body);
    }
}

#[test]
fn f15_ac9_expect_failures_read_like_chai() {
    let mut w = World::new();
    for (body, message) in [
        ("pm.expect(404).to.equal(200);", "expected 404 to equal 200"),
        (
            "pm.expect('a').to.not.equal('a');",
            "expected 'a' to not equal 'a'",
        ),
        (
            "pm.expect({a: 1}).to.eql({a: 2});",
            "expected { a: 1 } to deeply equal { a: 2 }",
        ),
        (
            "pm.expect([1, 'x']).to.deep.equal([]);",
            "expected [ 1, 'x' ] to deeply equal []",
        ),
        ("pm.expect(1).to.be.above(2);", "expected 1 to be above 2"),
        ("pm.expect(3).to.be.below(2);", "expected 3 to be below 2"),
        (
            "pm.expect(1).to.be.at.least(2);",
            "expected 1 to be at least 2",
        ),
        (
            "pm.expect(3).to.be.at.most(2);",
            "expected 3 to be at most 2",
        ),
        (
            "pm.expect(3).to.not.be.above(2);",
            "expected 3 to be at most 2",
        ),
        (
            "pm.expect(9).to.be.within(1, 5);",
            "expected 9 to be within 1..5",
        ),
        (
            "pm.expect(1).to.be.a('string');",
            "expected 1 to be a string",
        ),
        (
            "pm.expect(1).to.be.an('object');",
            "expected 1 to be an object",
        ),
        (
            "pm.expect('s').to.not.be.a('string');",
            "expected 's' not to be a string",
        ),
        (
            "pm.expect('abc').to.include('z');",
            "expected 'abc' to include 'z'",
        ),
        (
            "pm.expect([1]).to.not.include(1);",
            "expected [ 1 ] to not include 1",
        ),
        ("pm.expect(0).to.be.ok;", "expected 0 to be truthy"),
        ("pm.expect(1).to.be.true;", "expected 1 to be true"),
        ("pm.expect(1).to.be.false;", "expected 1 to be false"),
        ("pm.expect(1).to.be.null;", "expected 1 to be null"),
        (
            "pm.expect(1).to.be.undefined;",
            "expected 1 to be undefined",
        ),
        ("pm.expect(null).to.exist;", "expected null to exist"),
        ("pm.expect([1]).to.be.empty;", "expected [ 1 ] to be empty"),
        (
            "pm.expect([1]).to.have.lengthOf(2);",
            "expected [ 1 ] to have a length of 2 but got 1",
        ),
        (
            "pm.expect({}).to.have.property('a');",
            "expected {} to have property 'a'",
        ),
        (
            "pm.expect({a: 1}).to.have.property('a', 2);",
            "expected { a: 1 } to have property 'a' of 2, but got 1",
        ),
        (
            "pm.expect('abc').to.match(/z/);",
            "expected 'abc' to match /z/",
        ),
        (
            "pm.expect(3).to.be.oneOf([1, 2]);",
            "expected 3 to be one of [ 1, 2 ]",
        ),
        (
            "pm.expect({a: 1}).to.have.keys('a', 'b');",
            "expected { a: 1 } to have keys 'a', and 'b'",
        ),
        (
            "pm.expect({a: 1}).to.have.keys('b');",
            "expected { a: 1 } to have key 'b'",
        ),
        (
            "pm.expect(1, 'custom').to.equal(2);",
            "custom: expected 1 to equal 2",
        ),
        (
            "pm.expect({'a-b': 'it\\'s', n: null}).to.equal(1);",
            "expected { 'a-b': 'it\\'s', n: null } to equal 1",
        ),
    ] {
        w.fails(body, message);
    }
}

// --- F15-AC11 console -------------------------------------------------------------------------

#[test]
fn f15_ac11_console_levels_and_formatting() {
    let mut w = World::new();
    w.pre(
        "console.log('a', 1, {b: [true, null]}, undefined);
         console.info('i'); console.warn('w'); console.error(new Error('e')); console.debug('d');",
    );
    let lines = w.host.borrow().lines.clone();
    assert_eq!(
        lines,
        [
            (Level::Log, r#"a 1 {"b":[true,null]} undefined"#.to_owned()),
            (Level::Info, "i".to_owned()),
            (Level::Warn, "w".to_owned()),
            (Level::Error, "Error: e".to_owned()),
            (Level::Log, "d".to_owned()),
        ]
    );
}

// --- F15-AC12 pm.sendRequest ------------------------------------------------------------------

#[test]
fn f15_ac12_send_request_calls_back_with_the_response() {
    let mut w = World::new();
    let outcome = w.pre(
        "pm.sendRequest('https://a.test/x', (err, res) => {
           console.log(err, res.code, res.json().id, res.headers.get('x-id'));
           pm.environment.set('id', res.json().id);
         });
         pm.sendRequest({url: 'https://a.test/y', method: 'POST', header: [{key: 'A', value: 'b'}], body: {mode: 'raw', raw: 'hi'}}, (err, res) => {});
         pm.sendRequest({url: 'https://a.test/z', header: {C: 'd'}});",
    );
    assert_eq!(outcome.error, None);
    assert_eq!(w.logged(), ["null 200 7 7"]);
    let sent = w.host.borrow().sent.clone();
    assert_eq!(sent[0].method, "GET");
    assert_eq!(sent[0].url, "https://a.test/x");
    assert_eq!(sent[1].method, "POST");
    assert_eq!(sent[1].headers, [("A".to_owned(), "b".to_owned())]);
    assert_eq!(sent[1].body.raw, "hi");
    assert_eq!(sent[2].headers, [("C".to_owned(), "d".to_owned())]);
    assert_eq!(w.variables.environment.as_ref().unwrap()[2].value, "7");
}

#[test]
fn f15_ac12_send_errors_and_the_limit() {
    let mut w = World::new();
    w.host.borrow_mut().reply = Err("Could not connect to a.test".into());
    w.pre("pm.sendRequest('https://a.test', (err, res) => console.log(err.message, res));");
    assert_eq!(w.logged(), ["Could not connect to a.test null"]);
    let outcome = w.pre("for (let i = 0; i < 11; i++) pm.sendRequest('https://a.test');");
    assert_eq!(
        outcome.error.as_deref(),
        Some(
            "Pre-request script failed in the request: Error: A script may send at most 10 requests"
        )
    );
    assert_eq!(w.host.borrow().sent.len(), 11, "1 before, then 10 allowed");
}

// --- F15-AC13 pm.info, iterationData and globals -----------------------------------------------

#[test]
fn f15_ac13_info_iteration_data_and_helpers() {
    let mut w = World::new();
    w.info.iteration = 2;
    w.info.iteration_count = 5;
    w.variables.data = vec![Variable::new("user", "ada")];
    w.pre(
        "console.log(pm.info.requestName, pm.info.eventName, pm.info.iteration, pm.info.iterationCount);
         console.log(pm.iterationData.get('user'), pm.iterationData.has('x'), JSON.stringify(pm.iterationData.toObject()));
         console.log(btoa('hi'), atob('aGk='), encodeURIComponent('a b'), JSON.parse('[1]')[0]);",
    );
    w.post("console.log(pm.info.eventName);");
    assert_eq!(
        w.logged(),
        [
            "Create user prerequest 2 5",
            r#"ada false {"user":"ada"}"#,
            "aGk= hi a%20b 1",
            "test",
        ]
    );
}

// --- F22-AC5 and F16-AC5 hooks -----------------------------------------------------------

#[test]
fn f22_ac5_visualizer_set_and_clear() {
    let mut w = World::new();
    let outcome = w.post("pm.visualizer.set('<b>{{name}}</b>', {name: 'Ada'});");
    let v = outcome.visualizer.unwrap();
    assert_eq!(v.template, "<b>{{name}}</b>");
    assert_eq!(v.data, r#"{"name":"Ada"}"#);
    assert_eq!(
        w.post("pm.visualizer.set('x'); pm.visualizer.clear();")
            .visualizer,
        None
    );
    assert_eq!(
        w.post("pm.visualizer.set('x');").visualizer.unwrap().data,
        "{}"
    );
}

#[test]
fn f16_ac5_set_next_request() {
    let mut w = World::new();
    assert_eq!(w.post("").next_request, None);
    assert_eq!(
        w.post("pm.execution.setNextRequest('Login');").next_request,
        Some(Some("Login".into()))
    );
    assert_eq!(
        w.post("postman.setNextRequest(null);").next_request,
        Some(None)
    );
}
