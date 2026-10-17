//! Top-level window: wires [`App`] to iced, runs effects (sends, file
//! dialogs, session saves, workspace changes), watches the workspace
//! folder, keeps editor widgets in sync, handles shortcuts.

use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferry_app::{
    App, Client, Doc, Edit, Effect, FileTarget, RealFs, SessionStore, Table, WorkspaceStore,
};
use iced::futures::Stream;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::text_editor;
use iced::{Subscription, Task, Theme, window};

use crate::editors::Editors;
use crate::probe::Probe;
use crate::{Element, screen, theme};

/// How often a changed session is saved (F07-AC5).
pub const AUTOSAVE_EVERY: Duration = Duration::from_secs(2);

/// Messages of the top-level window.
#[derive(Debug, Clone)]
pub enum Message {
    App(ferry_app::Message),
    /// An edit in the raw body editor of the active tab.
    RawAction(text_editor::Action),
    /// An edit in a bulk-edit editor of the active tab.
    BulkAction(Table, text_editor::Action),
    /// An edit in the multi-line field of a collection, folder or example tab.
    DocAction(text_editor::Action),
    /// Something in the workspace folder changed on disk (F09-AC10).
    WorkspaceChanged,
    /// The request/response divider (F14-AC4).
    ResizeStart,
    /// Pointer at this share of the workbench height while resizing.
    ResizeMoved(f32),
    ResizeEnd,
    /// A frame was rendered (only while probing startup).
    Frame,
    CloseRequested(window::Id),
    /// A session save finished; `Some` carries the error text.
    Saved(Option<String>),
}

impl From<ferry_app::Message> for Message {
    fn from(message: ferry_app::Message) -> Self {
        Self::App(message)
    }
}

/// Top-level UI state.
pub struct Shell {
    app: App,
    editors: Editors,
    client: Client,
    store: Option<Arc<SessionStore<RealFs>>>,
    /// The workspace folder, when Ferry reads and writes one.
    workspace: Option<Arc<WorkspaceStore<RealFs>>>,
    probe: Probe,
    resizing: bool,
    /// Last session save error, shown in the footer.
    save_error: Option<String>,
}

impl Shell {
    /// A window for `app`. Sessions are saved to `store` when given.
    #[must_use]
    pub fn new(app: App, client: Client, store: Option<SessionStore<RealFs>>) -> Self {
        Self {
            editors: Editors::for_app(&app),
            app,
            client,
            store: store.map(Arc::new),
            workspace: None,
            probe: Probe::from_env(),
            resizing: false,
            save_error: None,
        }
    }

    /// Read and write the workspace in `store` (F09).
    #[must_use]
    pub fn with_workspace(mut self, store: WorkspaceStore<RealFs>) -> Self {
        self.workspace = Some(Arc::new(store));
        self
    }

    #[must_use]
    pub fn app(&self) -> &App {
        &self.app
    }

    #[must_use]
    pub fn resizing(&self) -> bool {
        self.resizing
    }

    /// Apply a message; sends and saves run as background tasks so the UI
    /// never blocks (F05-AC7).
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Frame => {
                self.probe.frame();
                Task::none()
            }
            Message::App(message) => self.update_app(message),
            Message::RawAction(action) => {
                let id = self.app.tab().id;
                match self.editors.get_mut(id) {
                    Some(editors) => {
                        let is_edit = action.is_edit();
                        editors.raw.perform(action);
                        if is_edit {
                            let text = editors.raw.text();
                            self.update_app(ferry_app::Message::Edit(Edit::RawText(text)))
                        } else {
                            Task::none()
                        }
                    }
                    None => Task::none(),
                }
            }
            Message::BulkAction(table, action) => {
                let id = self.app.tab().id;
                match self.editors.bulk_mut(id, table) {
                    Some(content) => {
                        let is_edit = action.is_edit();
                        content.perform(action);
                        if is_edit {
                            let text = content.text();
                            self.update_app(ferry_app::Message::Edit(Edit::BulkText(table, text)))
                        } else {
                            Task::none()
                        }
                    }
                    None => Task::none(),
                }
            }
            Message::DocAction(action) => {
                let id = self.app.tab().id;
                let doc = &self.app.tab().doc;
                let edit: Option<fn(String) -> Edit> = match doc {
                    Doc::Container(_) => Some(Edit::Description),
                    Doc::Example(_) => Some(Edit::ExampleBody),
                    _ => None,
                };
                match (edit, self.editors.doc_action(id, action)) {
                    (Some(edit), Some(text)) => {
                        self.update_app(ferry_app::Message::Edit(edit(text)))
                    }
                    _ => Task::none(),
                }
            }
            Message::WorkspaceChanged => self.reload_workspace(),
            Message::ResizeStart => {
                self.resizing = true;
                Task::none()
            }
            Message::ResizeMoved(split) => {
                if self.resizing {
                    self.app.update(ferry_app::Message::SplitDragged(split));
                }
                Task::none()
            }
            Message::ResizeEnd => {
                self.resizing = false;
                Task::none()
            }
            Message::CloseRequested(id) => {
                self.save_now();
                window::close(id)
            }
            Message::Saved(error) => {
                self.save_error = error;
                Task::none()
            }
        };
        self.editors.sync(&self.app);
        task
    }

    fn update_app(&mut self, message: ferry_app::Message) -> Task<Message> {
        let Some(effect) = self.app.update(message) else {
            return Task::none();
        };
        match effect {
            effect @ Effect::Send { .. } => {
                Task::perform(ferry_app::run(effect, self.client.clone()), |message| {
                    message.map(Message::App)
                })
                .and_then(Task::done)
            }
            Effect::SaveSession(session) => match &self.store {
                Some(store) => {
                    let store = Arc::clone(store);
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || store.save(&session))
                                .await
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.map_err(|e| e.to_string()))
                                .err()
                        },
                        Message::Saved,
                    )
                }
                None => Task::none(),
            },
            Effect::PickFile { tab, target } => Task::perform(pick_file(target), move |path| {
                Message::App(ferry_app::Message::FilePicked { tab, target, path })
            }),
            Effect::Workspace { action, ops } => match &self.workspace {
                Some(store) => {
                    let store = Arc::clone(store);
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let error = store.apply(&ops).err().map(|e| e.to_string());
                                (error, store.load())
                            })
                            .await
                        },
                        move |result| {
                            let (error, workspace) = result.unwrap_or_else(|e| {
                                (Some(e.to_string()), ferry_app::Workspace::default())
                            });
                            Message::App(ferry_app::Message::WorkspaceApplied {
                                action: action.clone(),
                                error,
                                workspace: Arc::new(workspace),
                            })
                        },
                    )
                }
                None => Task::none(),
            },
            Effect::PickWorkspace => Task::perform(pick_folder(), |path| {
                Message::App(ferry_app::Message::WorkspacePicked(path))
            }),
            Effect::SwitchWorkspace(path) => {
                let store = Arc::new(WorkspaceStore::new(RealFs, &path));
                self.workspace = Some(Arc::clone(&store));
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let error = store.init().err().map(|e| e.to_string());
                            (error, store.load())
                        })
                        .await
                    },
                    |result| {
                        let (error, workspace) = result.unwrap_or_else(|e| {
                            (Some(e.to_string()), ferry_app::Workspace::default())
                        });
                        Message::App(ferry_app::Message::WorkspaceApplied {
                            action: "open the workspace".to_owned(),
                            error,
                            workspace: Arc::new(workspace),
                        })
                    },
                )
            }
        }
    }

    /// Load the workspace again after an outside change (F09-AC10).
    fn reload_workspace(&self) -> Task<Message> {
        let Some(store) = &self.workspace else {
            return Task::none();
        };
        let store = Arc::clone(store);
        Task::perform(
            async move { tokio::task::spawn_blocking(move || store.load()).await },
            |result| match result {
                Ok(workspace) => {
                    Message::App(ferry_app::Message::WorkspaceLoaded(Arc::new(workspace)))
                }
                // The load task panicked; keep what is shown.
                Err(_) => Message::Saved(None),
            },
        )
    }

    /// Save synchronously, for window close.
    fn save_now(&mut self) {
        if let Some(store) = &self.store
            && let Err(e) = store.save(&self.app.session())
        {
            self.save_error = Some(e.to_string());
        }
    }

    /// Render the window.
    #[must_use]
    pub fn view(&self) -> Element<'_, Message> {
        screen::view(
            &self.app,
            &self.editors,
            self.resizing,
            self.save_error.as_deref(),
        )
    }

    /// Shortcuts, autosave, close requests, workspace watching and the
    /// startup probe.
    pub fn subscription(&self) -> Subscription<Message> {
        let watch = match &self.workspace {
            Some(store) => {
                Subscription::run_with(WatchRoot(store.root().to_path_buf()), watch_workspace)
            }
            None => Subscription::none(),
        };
        Subscription::batch([
            keyboard::listen().filter_map(shortcut),
            iced::time::every(AUTOSAVE_EVERY).map(|_| Message::App(ferry_app::Message::Autosave)),
            window::close_requests().map(Message::CloseRequested),
            self.probe.subscription(|| Message::Frame),
            watch,
        ])
    }

    #[must_use]
    pub fn theme(&self) -> Theme {
        theme::theme()
    }
}

/// The folder a watch subscription follows; a new folder restarts it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct WatchRoot(PathBuf);

/// Report changes in the workspace folder (F09-AC10).
fn watch_workspace(root: &WatchRoot) -> impl Stream<Item = Message> + use<> {
    let root = root.0.clone();
    iced::stream::channel(16, async move |output| {
        let sender = output.clone();
        let watcher = ferry_app::watch::Watcher::start(&root, move || {
            let _ = sender.clone().try_send(Message::WorkspaceChanged);
        });
        if let Err(error) = watcher.as_ref() {
            eprintln!(
                "ferry: not watching {} for changes: {error}",
                root.display()
            );
        }
        // Keep the watcher alive for as long as the subscription runs.
        std::future::pending::<()>().await;
        drop((watcher, output));
    })
}

/// Ask the user for a workspace folder (F09-AC14).
async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Open a workspace folder")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
}

/// Ask the user for a file (F03-AC5, F03-AC6).
async fn pick_file(target: FileTarget) -> Option<std::path::PathBuf> {
    let title = match target {
        FileTarget::Binary => "Select a file to send as the body",
        FileTarget::FormField(_) => "Select a file for this field",
    };
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
}

/// Global keyboard shortcuts (F05-AC1, F07-AC1..AC3, F09-AC5, F14-AC4).
#[must_use]
pub fn shortcut(event: keyboard::Event) -> Option<Message> {
    use ferry_app::Message as M;
    let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
        return None;
    };
    let message = match key.as_ref() {
        Key::Named(Named::Enter) if modifiers.command() => M::SendPressed,
        Key::Named(Named::Tab) if modifiers.control() && modifiers.shift() => M::PreviousTab,
        Key::Named(Named::Tab) if modifiers.control() => M::NextTab,
        Key::Character("t") if modifiers.command() => M::NewTab,
        Key::Character("w") if modifiers.command() => M::CloseActiveTab,
        Key::Character("b") if modifiers.command() => M::ToggleSidebar,
        Key::Character("s") if modifiers.command() => M::Save,
        Key::Named(Named::Escape) => M::DialogCancel,
        _ => return None,
    };
    Some(Message::App(message))
}
