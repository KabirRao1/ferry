//! Ferry application state, messages and `update()` logic.
//!
//! Everything here is testable headless; `ferry-ui` only renders it and runs
//! the [`Effect`]s that `update()` returns.

mod activity;
mod ambient;
mod app;
pub mod body_view;
pub mod console;
mod doc;
mod find;
mod preview;
pub mod runner;
mod runner_tab;
mod scripting;
mod tab;
mod workspace;

pub use activity::{CookieEditor, CookieManager, Pending, TRUNCATED_NOTE, history_response};
pub use ambient::{Ambient, Clock, FixedAmbient, SystemAmbient};
pub use app::{App, Effect, Finished, Message, SidebarSection, WRAP_BYTES, received, run};
pub use bytes::Bytes;
pub use doc::{
    ContainerDoc, ContainerFields, ContainerSection, Doc, EnvDoc, EnvTarget, ExampleDoc,
    ExampleFields, VarEdit, apply_var,
};
pub use ferry_core::auth::{ApiKeyLocation, AuthDraft, AuthKind};
pub use ferry_core::body::{BodyMode, RawLanguage};
pub use ferry_core::collection::{
    self as collection, AuthSource, CollectionFile, Example, FolderFile, Hit, Item, ItemKind, Node,
    RequestFile,
};
pub use ferry_core::cookies::{Cookie, Jar};
pub use ferry_core::draft::{AutoHeader, RequestDraft};
pub use ferry_core::environment::{self as environment, VariablesFile};
pub use ferry_core::error::SendError;
pub use ferry_core::format;
pub use ferry_core::history::{
    Entry as HistoryEntry, History, MAX_BODY as HISTORY_MAX_BODY, Outcome as HistoryOutcome,
};
pub use ferry_core::json::JsonError;
pub use ferry_core::kv::KeyValue;
pub use ferry_core::request::{Method, Part, PartValue, Request, RequestBody};
pub use ferry_core::response::{Header, Response, SetCookie, Timing};
pub use ferry_core::scripts::{Phase as ScriptPhase, Scripts};
pub use ferry_core::session::{Session, SessionTab};
pub use ferry_core::text::IndexError;
pub use ferry_core::variables::{Origin, ScopeKind, Variable};
pub use ferry_core::workspace::{EnvironmentEntry, Op, Workspace};
pub use ferry_core::{datafile, time};
pub use ferry_http::CancellationToken;
pub use ferry_http::{Client, Config, Cookies};
pub use ferry_script::{
    Info as ScriptInfo, Level as ScriptLevel, Location as ScriptLocation, Script, TestResult,
    TestStatus, Variables as ScriptVariables, Visualizer,
};
pub use ferry_store::{
    CookieStore, HistoryStore, Loaded, MemFs, OpError, RealFs, Restored, SessionStore,
    WorkspaceStore, data_dir, watch,
};
pub use find::{FILTER_TOO_LARGE, Filter, FilterState, Find, NO_RESULTS, run as run_find};
pub use preview::{PREVIEW_UNAVAILABLE, PreviewTarget, extension, previewable};
pub use runner_tab::{RunnerDoc, RunnerEdit, RunnerItem, run_step};
pub use scripting::{
    ScriptLine, ScriptReport, ScriptedSend, Sent, Targets as ScriptTargets, apply as apply_script,
    execute as execute_scripted, script_request, script_response, variables as run_variables,
};
pub use tab::{
    BodyFormat, Edit, FileTarget, Phase, Received, RequestTab, ResponseTab, RowEdit, Tab, TabId,
    Table, TestFilter,
};
pub use workspace::{Dialog, Target, VarState, VarUse};
