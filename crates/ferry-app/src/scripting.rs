//! Running scripts around a send (F15-AC2..AC12, ADR-0044, ADR-0045).
//!
//! A request with scripts is not built on the UI thread: its pre-request
//! scripts may change variables it uses. [`execute`] runs the scripts,
//! resolves and builds the request, sends it and runs the post-response
//! scripts, all off the UI thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use ferry_core::body::BodyMode;
use ferry_core::collection::{self, COLLECTION_FILE, CollectionFile, Item, Node};
use ferry_core::draft::RequestDraft;
use ferry_core::environment::{self, VariablesFile};
use ferry_core::kv::KeyValue;
use ferry_core::params;
use ferry_core::request::{Method, Request, RequestBody};
use ferry_core::response::Response;
use ferry_core::scripts::Phase;
use ferry_core::variables::{Ambient, Resolver};
use ferry_core::workspace::{GLOBALS_FILE, Op, Workspace};
use ferry_http::{CancellationToken, Client, Cookies, Outcome as HttpOutcome};
use ferry_script::{
    Host, Info, Input, Level, Location, Script, ScriptBody, ScriptRequest, ScriptResponse,
    TestResult, Variables, Visualizer,
};

use crate::ambient::FixedAmbient;
use crate::app::Finished;
use crate::console::describe_body;
use crate::workspace::{join, local_op};

// CONTRACT
/// A send whose scripts still have to run.
#[derive(Debug, Clone)]
pub struct ScriptedSend {
    /// The draft as edited, inherited auth filled in, variables unresolved.
    pub draft: RequestDraft,
    pub pre: Vec<Script>,
    pub post: Vec<Script>,
    pub variables: Variables,
    pub info: Info,
    /// Where changed variables are saved.
    pub targets: Targets,
    /// The clock and a random seed for dynamic variables (F08-AC7).
    pub now: SystemTime,
    pub seed: u64,
}

// CONTRACT
/// The files script variable changes go to (ADR-0045).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Targets {
    /// The selected environment's file.
    pub environment: Option<String>,
    /// The collection's directory.
    pub collection: Option<String>,
}

// CONTRACT
/// A console line from a script (F15-AC11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptLine {
    pub level: Level,
    pub text: String,
}

// CONTRACT
/// The request as finally sent, for the console (F18-AC1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub method: String,
    pub url: String,
    pub headers: Vec<ferry_core::response::Header>,
    pub body: String,
}

// CONTRACT
/// What a send's scripts did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptReport {
    /// Tests from both phases.
    pub tests: Vec<TestResult>,
    /// Why the post-response scripts failed (F15-AC3).
    pub error: Option<String>,
    pub console: Vec<ScriptLine>,
    /// Every scope after the scripts; `changed` says which to save.
    pub variables: Variables,
    pub targets: Targets,
    pub visualizer: Option<Visualizer>,
    pub sent: Option<Sent>,
    /// `pm.execution.setNextRequest` (F16-AC5).
    pub next_request: Option<Option<String>>,
}

impl ScriptReport {
    // CONTRACT
    /// Writes saving the changed scopes to their local files (ADR-0045).
    #[must_use]
    pub fn variable_ops(&self) -> Vec<Op> {
        let v = &self.variables;
        let mut ops = Vec::new();
        if v.changed.environment
            && let (Some(path), Some(list)) = (&self.targets.environment, &v.environment)
        {
            let file = VariablesFile {
                name: v.environment_name.clone(),
                variables: list.clone(),
            };
            ops.push(local_op(path, environment::write(&file).1));
        }
        if v.changed.globals {
            let file = VariablesFile {
                name: None,
                variables: v.globals.clone(),
            };
            ops.push(local_op(GLOBALS_FILE, environment::write(&file).1));
        }
        if v.changed.collection
            && let (Some(dir), Some(list)) = (&self.targets.collection, &v.collection)
        {
            let file = CollectionFile {
                variables: list.clone(),
                ..CollectionFile::default()
            };
            ops.push(local_op(
                &join(dir, COLLECTION_FILE),
                collection::write_collection_local(&file),
            ));
        }
        ops
    }
}

/// The scripts of the containers holding `link`, collection first, and of
/// the request itself (F15-AC2).
#[must_use]
pub fn chain(
    workspace: &Workspace,
    link: Option<&str>,
    own: &ferry_core::scripts::Scripts,
) -> (Vec<Script>, Vec<Script>) {
    let mut pre = Vec::new();
    let mut post = Vec::new();
    let mut push = |location: Location, scripts: &ferry_core::scripts::Scripts| {
        for (phase, list) in [
            (Phase::PreRequest, &mut pre),
            (Phase::PostResponse, &mut post),
        ] {
            let source = scripts.get(phase);
            if !source.trim().is_empty() {
                list.push(Script {
                    location: location.clone(),
                    source: source.to_owned(),
                });
            }
        }
    };
    if let Some(link) = link {
        for item in collection::ancestors(&workspace.collections, link) {
            match &item.node {
                Node::Collection(c) => push(Location::Collection, &c.scripts),
                Node::Folder(f) => push(Location::Folder(item.name().to_owned()), &f.scripts),
                _ => {}
            }
        }
    }
    push(Location::Request, own);
    (pre, post)
}

/// The variables and their files for a request linked to `link`, with the
/// environment at `environment` selected.
#[must_use]
pub fn variables(
    workspace: &Workspace,
    environment: Option<&str>,
    link: Option<&str>,
) -> (Variables, Targets) {
    let env = environment.and_then(|path| {
        workspace
            .environment(path)
            .and_then(|e| e.file.as_ref().ok())
            .map(|file| (path, file))
    });
    let collection: Option<&Item> = link.and_then(|l| {
        collection::ancestors(&workspace.collections, l)
            .first()
            .copied()
    });
    let collection_vars = collection.and_then(|item| match &item.node {
        Node::Collection(c) => Some(c.variables.clone()),
        _ => None,
    });
    let variables = Variables {
        environment: env.map(|(_, file)| file.variables.clone()),
        environment_name: env.and_then(|(_, file)| file.name.clone()),
        globals: workspace
            .globals
            .as_ref()
            .map(|g| g.variables.clone())
            .unwrap_or_default(),
        collection: collection_vars,
        ..Variables::default()
    };
    let targets = Targets {
        environment: env.map(|(path, _)| path.to_owned()),
        collection: collection
            .filter(|item| matches!(item.node, Node::Collection(_)))
            .map(|item| item.path.clone()),
    };
    (variables, targets)
}

fn body_mode(mode: BodyMode) -> &'static str {
    match mode {
        BodyMode::None => "none",
        BodyMode::FormData => "formdata",
        BodyMode::UrlEncoded => "urlencoded",
        BodyMode::Raw => "raw",
        BodyMode::Binary => "file",
    }
}

// CONTRACT
/// `pm.request` for a pre-request script: the draft's method, URL, enabled
/// headers and body (F15-AC7).
#[must_use]
pub fn script_request(draft: &RequestDraft) -> ScriptRequest {
    ScriptRequest {
        method: draft.method.to_string(),
        url: draft.url.clone(),
        headers: draft
            .headers
            .iter()
            .filter(|h| h.is_active())
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect(),
        body: ScriptBody {
            mode: body_mode(draft.body.mode).to_owned(),
            raw: draft.body.raw.clone(),
        },
    }
}

// CONTRACT
/// Apply a pre-request script's changes to `draft` (F15-AC7): method, URL
/// (and so the params), headers, and a raw body.
///
/// # Errors
/// The method error's text for a method the script made invalid.
pub fn apply(draft: &mut RequestDraft, request: &ScriptRequest) -> Result<(), String> {
    if request.method != draft.method.to_string() {
        draft.method = Method::parse(request.method.trim()).map_err(|e| e.to_string())?;
    }
    if request.url != draft.url {
        draft.params = params::rows_from_url(&request.url, &draft.params);
        draft.url.clone_from(&request.url);
    }
    if request.headers != script_request(draft).headers {
        draft.headers = request
            .headers
            .iter()
            .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
            .collect();
    }
    if request.body.mode == "raw"
        && (draft.body.mode != BodyMode::Raw || request.body.raw != draft.body.raw)
    {
        draft.body.mode = BodyMode::Raw;
        draft.body.raw.clone_from(&request.body.raw);
    }
    Ok(())
}

/// `pm.request` for a post-response script: the request as sent.
fn sent_request(request: &Request, response: &Response) -> ScriptRequest {
    let headers = if response.sent.is_empty() {
        &request.headers
    } else {
        &response.sent
    };
    let body = match &request.body {
        RequestBody::Empty => ScriptBody::default(),
        RequestBody::Bytes(bytes) => ScriptBody {
            mode: "raw".to_owned(),
            raw: String::from_utf8_lossy(bytes).into_owned(),
        },
        RequestBody::Multipart(_) => ScriptBody {
            mode: "formdata".to_owned(),
            raw: String::new(),
        },
        RequestBody::File(_) => ScriptBody {
            mode: "file".to_owned(),
            raw: String::new(),
        },
    };
    ScriptRequest {
        method: request.method.to_string(),
        url: request.url.to_string(),
        headers: headers
            .iter()
            .map(|h| (h.name.clone(), h.value.clone()))
            .collect(),
        body,
    }
}

// CONTRACT
/// `pm.response` for a response (F15-AC8).
#[must_use]
pub fn script_response(response: &Response) -> ScriptResponse {
    ScriptResponse {
        code: response.status,
        status: response.reason.clone(),
        headers: response
            .headers
            .iter()
            .map(|h| (h.name.clone(), h.value.clone()))
            .collect(),
        body: String::from_utf8_lossy(&response.body).into_owned(),
        response_time: u64::try_from(response.elapsed.as_millis()).unwrap_or(u64::MAX),
        response_size: response.body.len() as u64,
    }
}

/// What the host scripts run against is made of: Ferry's client on the
/// send's runtime.
#[derive(Clone)]
struct HostParts {
    client: Client,
    runtime: tokio::runtime::Handle,
    cookies: Cookies,
    cancel: CancellationToken,
    ambient: FixedAmbient,
}

struct SendHost {
    parts: HostParts,
    console: Vec<ScriptLine>,
}

impl Host for SendHost {
    fn send(&mut self, request: ScriptRequest) -> Result<ScriptResponse, String> {
        let mut draft = RequestDraft::default();
        apply(&mut draft, &request)?;
        let built = draft.build().map_err(|e| e.to_string())?;
        let p = &self.parts;
        let outcome = p.runtime.block_on(p.client.send_with(
            &built,
            Some(p.cookies.clone()),
            &p.cancel,
        ));
        match outcome {
            HttpOutcome::Completed(response) => Ok(script_response(&response)),
            HttpOutcome::Failed(error) => Err(error.to_string()),
            HttpOutcome::Cancelled => Err("Request cancelled".to_owned()),
        }
    }

    fn console(&mut self, level: Level, text: String) {
        self.console.push(ScriptLine { level, text });
    }

    fn ambient(&mut self) -> &mut dyn Ambient {
        &mut self.parts.ambient
    }
}

/// What one phase produced.
struct PhaseResult {
    outcome: ferry_script::Outcome,
    request: ScriptRequest,
    variables: Variables,
    console: Vec<ScriptLine>,
}

/// Shown if the engine itself fails, which only a bug in it can cause.
const ENGINE_FAILED: &str = "The script engine stopped unexpectedly";

/// What one phase's scripts run on.
struct PhaseInput {
    phase: Phase,
    scripts: Vec<Script>,
    request: ScriptRequest,
    response: Option<ScriptResponse>,
    variables: Variables,
    info: Info,
}

/// Run one phase's scripts on a blocking thread.
async fn phase(input: PhaseInput, parts: HostParts, cancel: Arc<AtomicBool>) -> PhaseResult {
    let PhaseInput {
        phase,
        scripts,
        request,
        response,
        variables,
        info,
    } = input;
    let fallback = (request.clone(), variables.clone());
    let joined = tokio::task::spawn_blocking(move || {
        let (mut request, mut variables) = (request, variables);
        let host = Rc::new(RefCell::new(SendHost {
            parts,
            console: Vec::new(),
        }));
        let dyn_host: Rc<RefCell<dyn Host>> = host.clone();
        let outcome = ferry_script::run(
            &scripts,
            Input {
                phase,
                request: &mut request,
                response: response.as_ref(),
                variables: &mut variables,
                info: &info,
                cancel: &cancel,
            },
            &dyn_host,
        );
        let console = std::mem::take(&mut host.borrow_mut().console);
        PhaseResult {
            outcome,
            request,
            variables,
            console,
        }
    })
    .await;
    joined.unwrap_or_else(|_| PhaseResult {
        outcome: ferry_script::Outcome {
            error: Some(ENGINE_FAILED.to_owned()),
            ..ferry_script::Outcome::default()
        },
        request: fallback.0,
        variables: fallback.1,
        console: Vec::new(),
    })
}

// CONTRACT
/// Run `send`'s pre-request scripts, resolve and build the request, send
/// it and run the post-response scripts (F15-AC2). Cancelling `cancel`
/// stops the scripts and the send (F15-AC4).
pub async fn execute(
    send: ScriptedSend,
    cancel: CancellationToken,
    cookies: Cookies,
    client: Client,
) -> (Finished, ScriptReport) {
    let flag = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (flag, cancel) = (Arc::clone(&flag), cancel.clone());
        tokio::spawn(async move {
            cancel.cancelled().await;
            flag.store(true, Ordering::Relaxed);
        })
    };
    let result = execute_inner(send, &cancel, cookies, client, flag).await;
    watcher.abort();
    result
}

async fn execute_inner(
    send: ScriptedSend,
    cancel: &CancellationToken,
    cookies: Cookies,
    client: Client,
    flag: Arc<AtomicBool>,
) -> (Finished, ScriptReport) {
    let ScriptedSend {
        mut draft,
        pre,
        post,
        variables,
        info,
        targets,
        now,
        seed,
    } = send;
    let parts = HostParts {
        client: client.clone(),
        runtime: tokio::runtime::Handle::current(),
        cookies: cookies.clone(),
        cancel: cancel.clone(),
        ambient: FixedAmbient::with_seed(now, 0, seed),
    };
    let mut report = ScriptReport {
        targets,
        ..ScriptReport::default()
    };
    let pre = phase(
        PhaseInput {
            phase: Phase::PreRequest,
            scripts: pre,
            request: script_request(&draft),
            response: None,
            variables,
            info: info.clone(),
        },
        parts.clone(),
        Arc::clone(&flag),
    )
    .await;
    report.tests = pre.outcome.tests;
    report.visualizer = pre.outcome.visualizer;
    report.next_request = pre.outcome.next_request;
    report.console = pre.console;
    report.variables = pre.variables;
    if pre.outcome.cancelled {
        return (Finished::Cancelled, report);
    }
    if let Some(error) = pre.outcome.error {
        return (Finished::Failed(error), report);
    }
    let built = apply(&mut draft, &pre.request).and_then(|()| {
        let vars = report.variables.vars();
        let mut ambient = parts.ambient.clone();
        let mut resolver = Resolver::new(&vars, &mut ambient);
        let resolved = draft.resolve(&mut resolver);
        resolver.finish().map_err(|e| e.to_string())?;
        resolved.build().map_err(|e| e.to_string())
    });
    let built = match built {
        Ok(built) => built,
        Err(error) => return (Finished::Failed(error), report),
    };
    report.sent = Some(Sent {
        method: built.method.to_string(),
        url: built.url.to_string(),
        headers: built.headers.clone(),
        body: describe_body(&built.body),
    });
    let response = match client.send_with(&built, Some(cookies), cancel).await {
        HttpOutcome::Completed(response) => response,
        HttpOutcome::Failed(error) => return (Finished::Failed(error.to_string()), report),
        HttpOutcome::Cancelled => return (Finished::Cancelled, report),
    };
    let post = phase(
        PhaseInput {
            phase: Phase::PostResponse,
            scripts: post,
            request: sent_request(&built, &response),
            response: Some(script_response(&response)),
            variables: std::mem::take(&mut report.variables),
            info,
        },
        parts,
        flag,
    )
    .await;
    report.tests.extend(post.outcome.tests);
    report.console.extend(post.console);
    report.variables = post.variables;
    if post.outcome.visualizer.is_some() {
        report.visualizer = post.outcome.visualizer;
    }
    if post.outcome.next_request.is_some() {
        report.next_request = post.outcome.next_request;
    }
    if post.outcome.cancelled {
        return (Finished::Cancelled, report);
    }
    report.error = post.outcome.error;
    let result = match crate::app::received(response) {
        Ok(received) => Finished::Received(Box::new(received)),
        Err(error) => Finished::Failed(error),
    };
    (result, report)
}

impl crate::App {
    /// The scripted send for tab `index`, or `None` when nothing in its
    /// chain has a script (F15-AC2).
    pub(crate) fn scripted(&mut self, index: usize) -> Option<ScriptedSend> {
        let tab = &self.tabs[index];
        let link = tab.link.as_deref();
        let (pre, post) = chain(&self.workspace, link, &tab.draft.scripts);
        if pre.is_empty() && post.is_empty() {
            return None;
        }
        let mut draft = tab.draft.clone();
        if draft.auth.kind == ferry_core::auth::AuthKind::Inherit {
            draft.auth = self
                .inherited_auth(tab)
                .map(|(auth, _)| auth)
                .unwrap_or_default();
        }
        let (variables, targets) = variables(&self.workspace, self.environment.as_deref(), link);
        let info = Info {
            request_name: tab.title().to_owned(),
            iteration: 0,
            iteration_count: 1,
        };
        Some(ScriptedSend {
            draft,
            pre,
            post,
            variables,
            info,
            targets,
            now: self.ambient.now(),
            seed: self.ambient.random(),
        })
    }

    /// Start `send` from the active tab, as `send` does for a built request.
    pub(crate) fn send_scripted(&mut self, send: ScriptedSend) -> crate::Effect {
        self.last_send += 1;
        let id = self.last_send;
        self.started_scripted(id, &send.draft);
        let cookies = Cookies {
            jar: self.cookies.clone(),
            now: self.now_ms(),
        };
        let cancel = CancellationToken::new();
        let tab = &mut self.tabs[self.active];
        tab.phase = crate::Phase::Sending {
            id,
            cancel: cancel.clone(),
        };
        crate::Effect::ScriptedSend {
            tab: tab.id,
            id,
            send: Box::new(send),
            cancel,
            cookies,
        }
    }

    /// Log what the scripts did and save the variables they changed.
    pub(crate) fn scripts_finished(
        &mut self,
        id: u64,
        report: &ScriptReport,
    ) -> Option<crate::Effect> {
        if let (Some(pending), Some(sent)) = (self.pending.get_mut(&id), &report.sent) {
            pending.method.clone_from(&sent.method);
            pending.url.clone_from(&sent.url);
        }
        if let Some(console) = self.pending.get(&id).map(|p| p.console)
            && let Some(entry) = self.console.entry_mut(console)
        {
            if let Some(sent) = &report.sent {
                entry.method.clone_from(&sent.method);
                entry.url.clone_from(&sent.url);
                entry.request_headers.clone_from(&sent.headers);
                entry.request_body.clone_from(&sent.body);
            }
            entry.script_lines.clone_from(&report.console);
        }
        let ops = report.variable_ops();
        (!ops.is_empty()).then(|| crate::Effect::Workspace {
            action: "save script variables".to_owned(),
            ops,
        })
    }
}

impl crate::Tab {
    /// Attach a scripted send's results to the response it produced.
    pub(crate) fn scripts_ran(&mut self, id: u64, report: &ScriptReport) {
        if let crate::Phase::Received(received) = &mut self.phase
            && received.send == id
        {
            received.tests.clone_from(&report.tests);
            received.script_error.clone_from(&report.error);
            received.visualizer.clone_from(&report.visualizer);
        }
    }
}
