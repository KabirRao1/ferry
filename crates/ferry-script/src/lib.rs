//! Pre-request and post-response scripts with the `pm` API (F15, ADR-0044).
//!
//! Each script runs in a fresh `QuickJS` runtime with a memory limit and a
//! time limit, and has no file system, process, network or module access.
//! The app reaches the outside world for it through [`Host`].
#![warn(clippy::pedantic)]

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ferry_core::environment;
use ferry_core::scripts::Phase;
use ferry_core::variables::{Ambient, Resolver, ScopeKind, Variable, Vars};
use rquickjs::context::EvalOptions;
use rquickjs::{CaughtError, Context, Ctx, Exception, Function, Object, Runtime};
use serde::{Deserialize, Serialize};

/// The longest a script may run, not counting `pm.sendRequest` waits
/// (F15-AC4).
pub const TIME_LIMIT: Duration = Duration::from_secs(5);
/// The most memory a script may allocate (F15-AC4).
pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// The most requests one script may send (F15-AC12).
pub const MAX_SENDS: usize = 10;
/// Native stack for scripts, well inside a worker thread's stack.
const STACK_LIMIT: usize = 512 * 1024;

const PRELUDE: &str = include_str!("prelude.js");

// CONTRACT
/// A request as scripts see and change it (F15-AC7).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: ScriptBody,
}

// CONTRACT
/// `pm.request.body`: its mode (`none`, `raw`, `urlencoded`, `formdata`,
/// `file`) and, for raw bodies, the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBody {
    pub mode: String,
    pub raw: String,
}

impl Default for ScriptBody {
    fn default() -> Self {
        Self {
            mode: "none".to_owned(),
            raw: String::new(),
        }
    }
}

// CONTRACT
/// A response as scripts see it (F15-AC8, F15-AC12).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResponse {
    pub code: u16,
    /// The reason phrase.
    pub status: String,
    pub headers: Vec<(String, String)>,
    /// The body as text (invalid UTF-8 replaced).
    pub body: String,
    /// Milliseconds.
    pub response_time: u64,
    /// Bytes.
    pub response_size: u64,
}

// CONTRACT
/// `pm.info` and the runner's data row (F15-AC13).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Info {
    pub request_name: String,
    pub iteration: usize,
    pub iteration_count: usize,
}

// CONTRACT
/// The variables scripts read and write (F15-AC5, F15-AC6). Script writes
/// to `environment`, `globals` and `collection` go to their local layer
/// (ADR-0045), and the scope is marked in `changed`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Variables {
    /// `None` when no environment is selected.
    pub environment: Option<Vec<Variable>>,
    pub environment_name: Option<String>,
    pub globals: Vec<Variable>,
    /// `None` for a request outside any collection.
    pub collection: Option<Vec<Variable>>,
    /// `pm.variables.set` values.
    pub local: Vec<Variable>,
    /// The runner's data row (F16-AC3).
    pub data: Vec<Variable>,
    pub changed: Changed,
}

// CONTRACT
/// Which saved scopes scripts changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    pub environment: bool,
    pub globals: bool,
    pub collection: bool,
}

impl Variables {
    // CONTRACT
    /// The scopes in precedence order (F08-AC2, F15-AC6): `pm.variables`
    /// values, the data row, the environment, the collection, globals.
    #[must_use]
    pub fn vars(&self) -> Vars<'_> {
        let mut vars = Vars::new()
            .with(ScopeKind::Local, &self.local)
            .with(ScopeKind::Data, &self.data);
        if let Some(env) = &self.environment {
            vars = vars.with(ScopeKind::Environment, env);
        }
        if let Some(collection) = &self.collection {
            vars = vars.with(ScopeKind::Collection, collection);
        }
        vars.with(ScopeKind::Global, &self.globals)
    }
}

/// The first enabled variable named `key`.
fn find<'a>(list: &'a [Variable], key: &str) -> Option<&'a Variable> {
    list.iter().find(|v| v.enabled && v.key == key)
}

/// A scope's enabled variables as `toObject()` lists them, in order: the
/// first definition of a key wins.
fn entries(list: &[Variable]) -> Vec<(&str, &str)> {
    let mut out: Vec<(&str, &str)> = Vec::new();
    for v in list.iter().filter(|v| v.enabled) {
        if !out.iter().any(|(k, _)| *k == v.key) {
            out.push((&v.key, &v.value));
        }
    }
    out
}

/// Entries as a JSON object, keeping their order; a later entry with the
/// same key replaces the value in place.
fn json_object<'a>(lists: impl IntoIterator<Item = Vec<(&'a str, &'a str)>>) -> String {
    let mut merged: Vec<(&str, &str)> = Vec::new();
    for list in lists {
        for (key, value) in list {
            match merged.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => merged.push((key, value)),
            }
        }
    }
    let fields: Vec<String> = merged
        .iter()
        .map(|(k, v)| {
            format!(
                "{}:{}",
                serde_json::Value::from(*k),
                serde_json::Value::from(*v)
            )
        })
        .collect();
    format!("{{{}}}", fields.join(","))
}

// CONTRACT
/// A `console` call's level (F15-AC11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    Log,
    Info,
    Warn,
    Error,
}

impl Level {
    fn parse(text: &str) -> Self {
        match text {
            "info" => Self::Info,
            "warn" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Log,
        }
    }
}

// CONTRACT
/// What scripts need from outside: sending, logging, a clock and
/// randomness (for `replaceIn`'s dynamic variables).
pub trait Host {
    /// Send a request for `pm.sendRequest` (F15-AC12).
    ///
    /// # Errors
    /// The text a failed send shows, such as a connection error.
    fn send(&mut self, request: ScriptRequest) -> Result<ScriptResponse, String>;
    /// A `console` line (F15-AC11).
    fn console(&mut self, level: Level, text: String);
    /// The clock and randomness for dynamic variables (F08-AC7).
    fn ambient(&mut self) -> &mut dyn Ambient;
}

// CONTRACT
/// Where a script comes from, as error messages name it (F15-AC3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Collection,
    Folder(String),
    Request,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Collection => f.write_str("the collection"),
            Self::Folder(name) => write!(f, "folder {name}"),
            Self::Request => f.write_str("the request"),
        }
    }
}

// CONTRACT
/// One script to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub location: Location,
    pub source: String,
}

// CONTRACT
/// A test's result (F15-AC9).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    /// Why a failed test failed.
    pub error: Option<String>,
}

// CONTRACT
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

// CONTRACT
/// What `pm.visualizer.set` asked for (F22-AC5): a Handlebars template and
/// its data as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Visualizer {
    pub template: String,
    pub data: String,
}

// CONTRACT
/// What a phase's scripts did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    pub tests: Vec<TestResult>,
    /// `Pre-request script failed in <location>: <message>` or the
    /// post-response equivalent (F15-AC3), or the cancellation note.
    pub error: Option<String>,
    pub visualizer: Option<Visualizer>,
    /// `pm.execution.setNextRequest`: `Some(None)` ends the iteration
    /// (F16-AC5).
    pub next_request: Option<Option<String>>,
    /// The run was cancelled; the caller discards everything.
    pub cancelled: bool,
}

// CONTRACT
/// Everything a phase's scripts work on.
pub struct Input<'a> {
    pub phase: Phase,
    /// Changed in place by pre-request scripts (F15-AC7).
    pub request: &'a mut ScriptRequest,
    /// The response, for post-response scripts.
    pub response: Option<&'a ScriptResponse>,
    pub variables: &'a mut Variables,
    pub info: &'a Info,
    /// Set to stop the scripts (F15-AC4).
    pub cancel: &'a Arc<AtomicBool>,
}

/// State the native functions share with the run.
struct Shared {
    variables: RefCell<Variables>,
    host: Rc<RefCell<dyn Host>>,
    sends: Cell<usize>,
    /// Time spent in `pm.sendRequest`, which the time limit leaves out.
    waiting: Cell<Duration>,
}

#[derive(Deserialize)]
struct Output {
    request: ScriptRequest,
    tests: Vec<TestResult>,
    visualizer: Option<Visualizer>,
    #[serde(rename = "nextRequest")]
    next_request: NextRequest,
}

#[derive(Deserialize)]
struct NextRequest {
    set: bool,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InputJson<'a> {
    event: &'static str,
    request: &'a ScriptRequest,
    response: Option<&'a ScriptResponse>,
    info: InfoJson<'a>,
    data: serde_json::Value,
    environment_name: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoJson<'a> {
    request_name: &'a str,
    iteration: usize,
    iteration_count: usize,
}

// CONTRACT
/// Run `scripts` in order for `input.phase` (F15-AC2, F15-AC3): each in a
/// fresh context, stopping at the first that fails. Pre-request scripts
/// change `input.request`; all of them may change `input.variables`.
pub fn run(scripts: &[Script], input: Input<'_>, host: &Rc<RefCell<dyn Host>>) -> Outcome {
    let Input {
        phase,
        request,
        response,
        variables,
        info,
        cancel,
    } = input;
    let mut outcome = Outcome::default();
    let shared = Rc::new(Shared {
        variables: RefCell::new(std::mem::take(variables)),
        host: Rc::clone(host),
        sends: Cell::new(0),
        waiting: Cell::new(Duration::ZERO),
    });
    for script in scripts.iter().filter(|s| !s.source.trim().is_empty()) {
        let data: serde_json::Value =
            serde_json::from_str(&json_object([entries(&shared.variables.borrow().data)]))
                .unwrap_or_default();
        let environment_name = shared.variables.borrow().environment_name.clone();
        let json = InputJson {
            event: phase.event_name(),
            request,
            response,
            info: InfoJson {
                request_name: &info.request_name,
                iteration: info.iteration,
                iteration_count: info.iteration_count,
            },
            data,
            environment_name: environment_name.as_deref(),
        };
        let json = serde_json::to_string(&json).unwrap_or_default();
        let (output, error) = run_one(&script.source, json, &shared, cancel);
        if let Some(output) = output {
            outcome.tests.extend(output.tests);
            if output.visualizer.is_some() {
                outcome.visualizer = output.visualizer;
            }
            if output.next_request.set {
                outcome.next_request = Some(output.next_request.name);
            }
            if phase == Phase::PreRequest && error.is_none() {
                *request = output.request;
            }
        }
        match error {
            Some(Failure::Cancelled) => {
                outcome.cancelled = true;
                outcome.error = Some("Request cancelled".to_owned());
                break;
            }
            Some(Failure::Message(message)) => {
                let phase = match phase {
                    Phase::PreRequest => "Pre-request",
                    Phase::PostResponse => "Post-response",
                };
                outcome.error = Some(format!(
                    "{phase} script failed in {}: {message}",
                    script.location
                ));
                break;
            }
            None => {}
        }
    }
    *variables = shared.variables.replace(Variables::default());
    outcome
}

enum Failure {
    Cancelled,
    Message(String),
}

/// Run one script; its output (if it could be read) and why it failed.
fn run_one(
    source: &str,
    input: String,
    shared: &Rc<Shared>,
    cancel: &Arc<AtomicBool>,
) -> (Option<Output>, Option<Failure>) {
    let Ok(runtime) = Runtime::new() else {
        return (
            None,
            Some(Failure::Message(
                "the script engine could not start".to_owned(),
            )),
        );
    };
    runtime.set_memory_limit(MEMORY_LIMIT);
    runtime.set_max_stack_size(STACK_LIMIT);
    let started = Instant::now();
    let timed_out = Rc::new(Cell::new(false));
    let cancelled = Rc::new(Cell::new(false));
    // Set while reading the output, which must not be interrupted.
    let reading = Rc::new(Cell::new(false));
    {
        let (shared, cancel) = (Rc::clone(shared), Arc::clone(cancel));
        let (timed_out, cancelled) = (Rc::clone(&timed_out), Rc::clone(&cancelled));
        let reading = Rc::clone(&reading);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            if reading.get() {
                return false;
            }
            if cancel.load(Ordering::Relaxed) {
                cancelled.set(true);
            } else if started.elapsed().saturating_sub(shared.waiting.get()) > TIME_LIMIT {
                timed_out.set(true);
            }
            cancelled.get() || timed_out.get()
        })));
    }
    let Ok(context) = Context::full(&runtime) else {
        return (
            None,
            Some(Failure::Message(
                "the script engine could not start".to_owned(),
            )),
        );
    };
    context.with(|ctx| {
        let setup = install(&ctx, shared, input).and_then(|()| eval(&ctx, PRELUDE, "prelude"));
        let result = setup.and_then(|()| eval(&ctx, source, "script"));
        let failure = result.err().map(|message| {
            if cancelled.get() {
                Failure::Cancelled
            } else if timed_out.get() {
                Failure::Message(format!("Script timed out after {} s", TIME_LIMIT.as_secs()))
            } else {
                Failure::Message(message)
            }
        });
        // Read what the script did, even after an error: tests recorded
        // before a throw still count.
        reading.set(true);
        let output = ctx
            .globals()
            .get::<_, Function>("__ferry_output")
            .and_then(|f| f.call::<_, String>(()))
            .ok()
            .and_then(|json| serde_json::from_str::<Output>(&json).ok());
        (output, failure)
    })
}

/// Evaluate `source` as global code; the error's text if it throws.
fn eval(ctx: &Ctx<'_>, source: &str, filename: &str) -> Result<(), String> {
    let mut options = EvalOptions::default();
    options.global = true;
    options.strict = false;
    options.filename = Some(filename.to_owned());
    let result: rquickjs::Result<()> = ctx.eval_with_options(source, options);
    CaughtError::catch(ctx, result).map_err(|e| describe(&e))
}

/// `<Name>: <message>`, with the script line for syntax errors.
fn describe(error: &CaughtError<'_>) -> String {
    match error {
        CaughtError::Exception(e) => {
            let message = e.message().unwrap_or_default();
            if message == "out of memory" {
                return format!(
                    "Script ran out of memory ({} MB)",
                    MEMORY_LIMIT / (1024 * 1024)
                );
            }
            let name: String = e
                .as_object()
                .get::<_, String>("name")
                .unwrap_or_else(|_| "Error".to_owned());
            let line = e.stack().and_then(|stack| script_line(&stack));
            match line {
                Some(line) if name == "SyntaxError" => format!("{name}: {message} (line {line})"),
                _ => format!("{name}: {message}"),
            }
        }
        CaughtError::Value(value) => match value.as_string().and_then(|s| s.to_string().ok()) {
            Some(text) => text,
            None => format!("{value:?}"),
        },
        CaughtError::Error(e) => e.to_string(),
    }
}

/// The line of the first `script:<line>:<column>` frame in a stack.
fn script_line(stack: &str) -> Option<usize> {
    let at = stack.find("script:")? + "script:".len();
    let digits: String = stack[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Put the native bridge `__ferry` and the input JSON into the context.
fn install<'js>(ctx: &Ctx<'js>, shared: &Rc<Shared>, input: String) -> Result<(), String> {
    let bridge = Object::new(ctx.clone()).map_err(|e| e.to_string())?;
    let mut functions = variable_functions(ctx, shared);
    let s = Rc::clone(shared);
    let replace_in = Function::new(ctx.clone(), move |text: String| {
        let vars = s.variables.borrow();
        let all = vars.vars();
        let mut host = s.host.borrow_mut();
        let mut resolver = Resolver::new(&all, host.ambient());
        resolver.resolve(&text)
    });
    let s = Rc::clone(shared);
    let console = Function::new(ctx.clone(), move |level: String, text: String| {
        s.host.borrow_mut().console(Level::parse(&level), text);
    });
    let s = Rc::clone(shared);
    let send = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, json: String| -> rquickjs::Result<String> {
            if s.sends.get() >= MAX_SENDS {
                return Err(Exception::throw_message(
                    &ctx,
                    &format!("A script may send at most {MAX_SENDS} requests"),
                ));
            }
            s.sends.set(s.sends.get() + 1);
            let request: ScriptRequest = serde_json::from_str(&json)
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            let started = Instant::now();
            let result = s.host.borrow_mut().send(request);
            s.waiting.set(s.waiting.get() + started.elapsed());
            let value = match result {
                Ok(response) => serde_json::json!({ "response": response }),
                Err(error) => serde_json::json!({ "error": error }),
            };
            Ok(value.to_string())
        },
    );
    functions.extend([
        ("replaceIn", replace_in),
        ("console", console),
        ("send", send),
    ]);
    for (name, function) in functions {
        let function = function.map_err(|e| e.to_string())?;
        bridge.set(name, function).map_err(|e| e.to_string())?;
    }
    let globals = ctx.globals();
    globals.set("__ferry", bridge).map_err(|e| e.to_string())?;
    globals
        .set("__ferry_input", input)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The scope a bridge call names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// `pm.variables`: every scope, in precedence order.
    All,
    Environment,
    Globals,
    Collection,
}

impl Scope {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "variables" => Self::All,
            "environment" => Self::Environment,
            "globals" => Self::Globals,
            "collection" => Self::Collection,
            _ => return None,
        })
    }
}

impl Variables {
    /// The lists `scope` reads, highest precedence first.
    fn lists(&self, scope: Scope) -> Vec<&[Variable]> {
        match scope {
            Scope::All => [
                Some(self.local.as_slice()),
                Some(self.data.as_slice()),
                self.environment.as_deref(),
                self.collection.as_deref(),
                Some(self.globals.as_slice()),
            ]
            .into_iter()
            .flatten()
            .collect(),
            Scope::Environment => self.environment.as_deref().into_iter().collect(),
            Scope::Globals => vec![self.globals.as_slice()],
            Scope::Collection => self.collection.as_deref().into_iter().collect(),
        }
    }

    fn get(&self, scope: Scope, key: &str) -> Option<String> {
        self.lists(scope)
            .into_iter()
            .find_map(|list| find(list, key))
            .map(|v| v.value.clone())
    }

    /// `toObject()`: lowest precedence first, so higher scopes overwrite.
    fn all(&self, scope: Scope) -> String {
        json_object(self.lists(scope).into_iter().rev().map(entries))
    }

    fn set(&mut self, scope: Scope, key: &str, value: &str) -> Result<(), &'static str> {
        match scope {
            Scope::All => set_plain(&mut self.local, key, value),
            Scope::Environment => {
                let env = self
                    .environment
                    .as_mut()
                    .ok_or("No environment is selected")?;
                environment::set_local(env, key, value);
                self.changed.environment = true;
            }
            Scope::Globals => {
                environment::set_local(&mut self.globals, key, value);
                self.changed.globals = true;
            }
            Scope::Collection => {
                let list = self
                    .collection
                    .as_mut()
                    .ok_or("This request is not in a collection")?;
                environment::set_local(list, key, value);
                self.changed.collection = true;
            }
        }
        Ok(())
    }

    fn unset(&mut self, scope: Scope, key: &str) {
        match scope {
            Scope::All => self.local.retain(|v| v.key != key),
            Scope::Environment => {
                if let Some(env) = self.environment.as_mut() {
                    environment::unset_local(env, key);
                    self.changed.environment = true;
                }
            }
            Scope::Globals => {
                environment::unset_local(&mut self.globals, key);
                self.changed.globals = true;
            }
            Scope::Collection => {
                if let Some(list) = self.collection.as_mut() {
                    environment::unset_local(list, key);
                    self.changed.collection = true;
                }
            }
        }
    }
}

/// `get`, `has`, `set`, `unset` and `all` over the variable scopes
/// (F15-AC5, F15-AC6).
fn variable_functions<'js>(
    ctx: &Ctx<'js>,
    shared: &Rc<Shared>,
) -> Vec<(&'static str, rquickjs::Result<Function<'js>>)> {
    let s = Rc::clone(shared);
    let get = Function::new(ctx.clone(), move |scope: String, key: String| {
        Scope::parse(&scope).and_then(|scope| s.variables.borrow().get(scope, &key))
    });
    let s = Rc::clone(shared);
    let has = Function::new(ctx.clone(), move |scope: String, key: String| {
        Scope::parse(&scope).is_some_and(|scope| s.variables.borrow().get(scope, &key).is_some())
    });
    let s = Rc::clone(shared);
    let set = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, scope: String, key: String, value: String| -> rquickjs::Result<()> {
            match Scope::parse(&scope) {
                Some(scope) => s
                    .variables
                    .borrow_mut()
                    .set(scope, &key, &value)
                    .map_err(|message| Exception::throw_message(&ctx, message)),
                None => Ok(()),
            }
        },
    );
    let s = Rc::clone(shared);
    let unset = Function::new(ctx.clone(), move |scope: String, key: String| {
        if let Some(scope) = Scope::parse(&scope) {
            s.variables.borrow_mut().unset(scope, &key);
        }
    });
    let s = Rc::clone(shared);
    let all = Function::new(ctx.clone(), move |scope: String| {
        Scope::parse(&scope)
            .map_or_else(|| "{}".to_owned(), |scope| s.variables.borrow().all(scope))
    });
    vec![
        ("get", get),
        ("has", has),
        ("set", set),
        ("unset", unset),
        ("all", all),
    ]
}

/// Set a `pm.variables` value: replace the first with the key, or add one.
fn set_plain(list: &mut Vec<Variable>, key: &str, value: &str) {
    match list.iter_mut().find(|v| v.key == key) {
        Some(v) => value.clone_into(&mut v.value),
        None => list.push(Variable::new(key, value)),
    }
}
