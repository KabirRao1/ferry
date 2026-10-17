//! Ferry application state, messages and `update()` logic.
//!
//! Everything here is testable headless; `ferry-ui` only renders it and runs
//! the [`Effect`]s that `update()` returns.

mod ambient;
mod app;
pub mod body_view;
mod doc;
mod tab;
mod workspace;

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
pub use ferry_core::draft::{AutoHeader, RequestDraft};
pub use ferry_core::environment::{self as environment, VariablesFile};
pub use ferry_core::error::SendError;
pub use ferry_core::format;
pub use ferry_core::json::JsonError;
pub use ferry_core::kv::KeyValue;
pub use ferry_core::request::{Method, Request, RequestBody};
pub use ferry_core::response::{Header, Response, Timing};
pub use ferry_core::session::{Session, SessionTab};
pub use ferry_core::text::IndexError;
pub use ferry_core::variables::{Origin, ScopeKind, Variable};
pub use ferry_core::workspace::{EnvironmentEntry, Op, Workspace};
pub use ferry_http::{Client, Config};
pub use ferry_store::{
    Loaded, MemFs, OpError, RealFs, SessionStore, WorkspaceStore, data_dir, watch,
};
pub use tab::{
    BodyFormat, Edit, FileTarget, Phase, Received, RequestTab, ResponseTab, RowEdit, Tab, TabId,
    Table,
};
pub use workspace::{Dialog, Target, VarState, VarUse};
