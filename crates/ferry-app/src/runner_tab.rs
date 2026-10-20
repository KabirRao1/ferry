//! The Runner tab (F16-AC1..AC6): its settings, and driving a [`Runner`]
//! one send at a time.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferry_core::collection::{self, Node};
use ferry_core::datafile::{self, Row};
use ferry_http::{CancellationToken, Client, Cookies};

use crate::app::{App, Effect, Finished, Message};
use crate::doc::Doc;
use crate::runner::{self, RunConfig, RunStatus, Runner};
use crate::scripting::{ScriptReport, ScriptedSend, execute, variables};
use crate::tab::TabId;

// CONTRACT
/// One request in the Runner tab's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerItem {
    pub path: String,
    pub name: String,
    pub method: String,
    pub checked: bool,
}

// CONTRACT
/// A Runner tab (F16-AC1).
#[derive(Debug, Clone)]
pub struct RunnerDoc {
    /// The collection or folder.
    pub path: String,
    pub name: String,
    pub items: Vec<RunnerItem>,
    /// As typed.
    pub iterations: String,
    pub delay: String,
    pub data_file: Option<PathBuf>,
    /// The data file's rows, or why it was refused (F16-AC3).
    pub data: Option<Result<Vec<Row>, String>>,
    /// `Stop on first failure`.
    pub bail: bool,
    pub run: Option<Runner>,
    /// The result whose tests are shown.
    pub selected: Option<usize>,
    /// The outcome of the last `Export Results`.
    pub notice: Option<String>,
    /// Bumped by every run, so a late result of an old run is ignored.
    pub(crate) generation: u64,
    pub(crate) cancel: Option<CancellationToken>,
}

impl PartialEq for RunnerDoc {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.generation == other.generation
    }
}

impl Eq for RunnerDoc {}

impl RunnerDoc {
    // CONTRACT
    /// Why `Iterations` is invalid, if it is.
    #[must_use]
    pub fn iterations_error(&self) -> Option<&'static str> {
        runner::parse_iterations(&self.iterations).err()
    }

    // CONTRACT
    /// Why `Delay` is invalid, if it is.
    #[must_use]
    pub fn delay_error(&self) -> Option<&'static str> {
        runner::parse_delay(&self.delay).err()
    }

    // CONTRACT
    /// Whether a run is going.
    #[must_use]
    pub fn running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.status == RunStatus::Running)
    }

    // CONTRACT
    /// Whether Run is enabled: valid numbers, a usable data file, at least
    /// one checked request and no run going (F16-AC1).
    #[must_use]
    pub fn can_run(&self) -> bool {
        self.iterations_error().is_none()
            && self.delay_error().is_none()
            && !matches!(self.data, Some(Err(_)))
            && self.items.iter().any(|i| i.checked)
            && !self.running()
    }
}

// CONTRACT
/// An edit to a Runner tab's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerEdit {
    Iterations(String),
    Delay(String),
    Bail(bool),
    Item(usize, bool),
    ClearData,
}

pub(crate) fn edit(doc: &mut RunnerDoc, edit: RunnerEdit) {
    if doc.running() {
        return;
    }
    match edit {
        RunnerEdit::Iterations(text) => doc.iterations = text,
        RunnerEdit::Delay(text) => doc.delay = text,
        RunnerEdit::Bail(on) => doc.bail = on,
        RunnerEdit::Item(i, on) => {
            if let Some(item) = doc.items.get_mut(i) {
                item.checked = on;
            }
        }
        RunnerEdit::ClearData => {
            doc.data_file = None;
            doc.data = None;
        }
    }
}

/// Wait `delay` unless cancelled, then run `send` (F16-AC2).
pub async fn run_step(
    send: ScriptedSend,
    delay: Duration,
    cancel: CancellationToken,
    cookies: Cookies,
    client: Client,
) -> (Finished, ScriptReport) {
    tokio::select! {
        () = cancel.cancelled() => (Finished::Cancelled, ScriptReport::default()),
        () = tokio::time::sleep(delay) => execute(send, cancel, cookies, client).await,
    }
}

impl App {
    // CONTRACT
    /// The time on the injected clock.
    #[must_use]
    pub fn now(&self) -> std::time::SystemTime {
        self.ambient.now()
    }

    // CONTRACT
    /// What the footer's Runner opens: the active tab's collection, or
    /// the first collection; `None` without collections.
    #[must_use]
    pub fn runner_target(&self) -> Option<String> {
        let tab = self.tab();
        let path = match &tab.doc {
            Doc::Runner(r) => Some(r.path.clone()),
            Doc::Container(c) => Some(c.path.clone()),
            _ => tab.link.clone(),
        };
        let collections = &self.workspace.collections;
        path.and_then(|p| {
            collection::ancestors(collections, &p)
                .first()
                .map(|c| c.path.clone())
                .or_else(|| {
                    collections
                        .iter()
                        .find(|c| c.path == p)
                        .map(|c| c.path.clone())
                })
        })
        .or_else(|| {
            collections
                .iter()
                .find(|c| matches!(c.node, Node::Collection(_)))
                .map(|c| c.path.clone())
        })
    }

    fn runner_doc(&mut self, tab: TabId) -> Option<&mut RunnerDoc> {
        let i = self.index_of(tab)?;
        match &mut self.tabs[i].doc {
            Doc::Runner(doc) => Some(doc),
            _ => None,
        }
    }

    /// Open, or bring forward, the Runner tab for the collection or folder
    /// at `path` (F16-AC1).
    pub(crate) fn open_runner(&mut self, path: &str) {
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| matches!(&t.doc, Doc::Runner(d) if d.path == path))
        {
            self.active = i;
            return;
        }
        let Some(item) = collection::find(&self.workspace.collections, path) else {
            return;
        };
        if !matches!(item.node, Node::Collection(_) | Node::Folder(_)) {
            return;
        }
        let items = runner::requests(&self.workspace, path)
            .into_iter()
            .map(|r| RunnerItem {
                method: r.draft.method.to_string(),
                path: r.path,
                name: r.name,
                checked: true,
            })
            .collect();
        let doc = RunnerDoc {
            path: path.to_owned(),
            name: item.name().to_owned(),
            items,
            iterations: "1".to_owned(),
            delay: "0".to_owned(),
            data_file: None,
            data: None,
            bail: false,
            run: None,
            selected: None,
            notice: None,
            generation: 0,
            cancel: None,
        };
        self.push_doc(Doc::Runner(Box::new(doc)));
    }

    /// A data file was read (F16-AC3).
    pub(crate) fn data_loaded(&mut self, tab: TabId, path: PathBuf, text: Result<String, String>) {
        if let Some(doc) = self.runner_doc(tab) {
            let parsed = text.and_then(|t| datafile::parse(&t).map_err(|e| e.to_string()));
            doc.data_file = Some(path);
            doc.data = Some(parsed);
        }
    }

    /// Start a run in the active Runner tab.
    pub(crate) fn run_start(&mut self) -> Option<Effect> {
        let tab = self.tab().id;
        let environment = self.environment.clone();
        let workspace = Arc::clone(&self.workspace);
        let now = self.ambient.now();
        let doc = self.runner_doc(tab)?;
        if !doc.can_run() {
            return None;
        }
        let checked: Vec<&str> = doc
            .items
            .iter()
            .filter(|i| i.checked)
            .map(|i| i.path.as_str())
            .collect();
        let requests: Vec<_> = runner::requests(&workspace, &doc.path)
            .into_iter()
            .filter(|r| checked.contains(&r.path.as_str()))
            .collect();
        let link = requests.first().map(|r| r.path.clone());
        let (vars, targets) = variables(&workspace, environment.as_deref(), link.as_deref());
        let config = RunConfig {
            iterations: runner::parse_iterations(&doc.iterations).unwrap_or(1),
            delay: runner::parse_delay(&doc.delay).unwrap_or_default(),
            data: match &doc.data {
                Some(Ok(rows)) => rows.clone(),
                _ => Vec::new(),
            },
            bail: doc.bail,
        };
        doc.run = Some(Runner::new(
            doc.name.clone(),
            requests,
            config,
            vars,
            targets,
            now,
        ));
        doc.generation += 1;
        doc.selected = None;
        doc.notice = None;
        doc.cancel = Some(CancellationToken::new());
        self.run_next(tab)
    }

    /// The next step of tab `tab`'s run, if any.
    fn run_next(&mut self, tab: TabId) -> Option<Effect> {
        let now = self.ambient.now();
        let seed = self.ambient.random();
        let cookies = Cookies {
            jar: self.cookies.clone(),
            now: self.now_ms(),
        };
        let doc = self.runner_doc(tab)?;
        let run = doc.run.as_mut()?;
        let delay = run.delay();
        let send = run.next_send(now, seed)?;
        Some(Effect::RunStep {
            tab,
            generation: doc.generation,
            send: Box::new(send),
            delay,
            cancel: doc.cancel.clone().unwrap_or_default(),
            cookies,
        })
    }

    /// Stop the active Runner tab's run (F16-AC4).
    pub(crate) fn run_stop(&mut self) {
        let tab = self.tab().id;
        let now = self.ambient.now();
        if let Some(doc) = self.runner_doc(tab) {
            if let Some(cancel) = &doc.cancel {
                cancel.cancel();
            }
            if let Some(run) = doc.run.as_mut() {
                run.stop(now);
            }
        }
    }

    /// Record a step's result and start the next one.
    pub(crate) fn run_step_finished(
        &mut self,
        tab: TabId,
        generation: u64,
        result: &Finished,
        report: &ScriptReport,
    ) -> Option<Effect> {
        let now = self.ambient.now();
        let doc = self.runner_doc(tab)?;
        if doc.generation != generation {
            return None;
        }
        doc.run.as_mut()?.record(result, report, now);
        if let Finished::Received(r) = result {
            self.store_cookies(&r.response);
        }
        let ops = report.variable_ops();
        let saves = (!ops.is_empty()).then(|| Effect::Workspace {
            action: "save script variables".to_owned(),
            ops,
        });
        match (saves, self.run_next(tab)) {
            (Some(a), Some(b)) => Some(Effect::Batch(vec![a, b])),
            (a, b) => a.or(b),
        }
    }

    /// The report of the active Runner tab's run, for `Export Results`
    /// (F16-AC6).
    pub(crate) fn run_export(&mut self) -> Option<Effect> {
        let now = self.ambient.now();
        let tab = self.tab().id;
        let doc = self.runner_doc(tab)?;
        let run = doc
            .run
            .as_ref()
            .filter(|r| r.status != RunStatus::Running)?;
        Some(Effect::SaveReport {
            tab,
            name: format!("{}-run.json", collection::file_name(&doc.path)),
            contents: run.report(now),
        })
    }

    /// The report was saved, or could not be.
    pub(crate) fn report_saved(&mut self, tab: TabId, result: Result<PathBuf, String>) {
        if let Some(doc) = self.runner_doc(tab) {
            doc.notice = Some(match result {
                Ok(path) => format!("Saved to {}", path.display()),
                Err(error) => error,
            });
        }
    }

    /// Select a result to show its tests.
    pub(crate) fn run_select(&mut self, index: usize) {
        let tab = self.tab().id;
        if let Some(doc) = self.runner_doc(tab) {
            doc.selected = Some(index);
        }
    }
}

// CONTRACT
/// Run a [`Effect::RunStep`] and report [`Message::RunStepFinished`].
pub async fn run_effect(effect: Effect, client: Client) -> Option<Message> {
    let Effect::RunStep {
        tab,
        generation,
        send,
        delay,
        cancel,
        cookies,
    } = effect
    else {
        return None;
    };
    let (result, report) = run_step(*send, delay, cancel, cookies, client).await;
    Some(Message::RunStepFinished {
        tab,
        generation,
        result: Arc::new(result),
        report: Arc::new(report),
    })
}
