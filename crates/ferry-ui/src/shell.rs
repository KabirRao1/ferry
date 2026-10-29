//! Top-level window: wires [`App`] to iced, runs effects (sends, file
//! dialogs, session, history and cookie saves, workspace changes, searches),
//! watches the workspace folder, keeps editor widgets in sync, handles
//! shortcuts.

use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferry_app::{
    App, Client, CookieStore, Doc, Edit, Effect, FileTarget, HistoryStore, RealFs, SessionStore,
    Table, WorkspaceStore, history_response, run_find,
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
    /// An edit in a script editor of the active tab (F15-AC1).
    ScriptAction(ferry_app::ScriptPhase, text_editor::Action),
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
    /// A session, history or cookie save finished; `Some` carries the error
    /// text.
    Saved(Option<String>),
    /// Enter in the find bar: the next match, or the previous one with
    /// Shift (F06-AC8).
    FindSubmit,
    /// The keyboard modifiers changed.
    Modifiers(keyboard::Modifiers),
    /// The Preview area moved or changed size (F22-AC1).
    PreviewMoved,
    /// A dialog was cancelled: nothing to do.
    Ignore,
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
    history: Option<Arc<HistoryStore<RealFs>>>,
    cookies: Option<Arc<CookieStore<RealFs>>>,
    modifiers: keyboard::Modifiers,
    preview: crate::preview::Preview,
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
            history: None,
            cookies: None,
            modifiers: keyboard::Modifiers::default(),
            preview: crate::preview::Preview::default(),
        }
    }

    /// Save history to `history` and the cookie jar to `cookies` (F10-AC6,
    /// F17-AC6).
    #[must_use]
    pub fn with_data(
        mut self,
        history: HistoryStore<RealFs>,
        cookies: CookieStore<RealFs>,
    ) -> Self {
        self.history = Some(Arc::new(history));
        self.cookies = Some(Arc::new(cookies));
        self
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
            Message::App(ferry_app::Message::FindOpen) => {
                let task = self.update_app(ferry_app::Message::FindOpen);
                let focus = iced::widget::operation::focus(iced::widget::Id::new(
                    crate::response::FIND_INPUT,
                ));
                Task::batch([task, focus])
            }
            // Escape closes a dialog, else the find bar (F06-AC8).
            Message::App(ferry_app::Message::DialogCancel)
                if self.app.dialog().is_none() && self.app.confirm_close().is_none() =>
            {
                self.update_app(ferry_app::Message::FindClose)
            }
            Message::App(message) => self.update_app(message),
            Message::FindSubmit => self.update_app(if self.modifiers.shift() {
                ferry_app::Message::FindPrevious
            } else {
                ferry_app::Message::FindNext
            }),
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::Ignore => Task::none(),
            Message::PreviewMoved => self.preview.place(),
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
            Message::ScriptAction(phase, action) => {
                let id = self.app.tab().id;
                match self.editors.script_action(id, phase, action) {
                    Some(text) => {
                        self.update_app(ferry_app::Message::Edit(Edit::Script(phase, text)))
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
        // Create, replace or destroy the web view to match (F22-AC1).
        let preview = self.preview.sync(self.app.preview());
        Task::batch([task, preview])
    }

    fn update_app(&mut self, message: ferry_app::Message) -> Task<Message> {
        match self.app.update(message) {
            Some(effect) => self.run(effect),
            None => Task::none(),
        }
    }

    /// Run an effect off the UI thread.
    fn run(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::Batch(effects) => {
                Task::batch(effects.into_iter().map(|effect| self.run(effect)))
            }
            effect @ (Effect::Find { .. } | Effect::Filter { .. }) => Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || run_find(effect))
                        .await
                        .ok()
                        .flatten()
                },
                |message| message.map(Message::App),
            )
            .and_then(Task::done),
            Effect::History {
                history,
                body,
                remove,
            } => match &self.history {
                Some(store) => {
                    let store = Arc::clone(store);
                    blocking(move || {
                        let saved = store.save(&history).map_err(|e| e.to_string());
                        let body = match &body {
                            Some((id, bytes)) => {
                                store.write_body(*id, bytes).map_err(|e| e.to_string())
                            }
                            None => Ok(()),
                        };
                        store.remove_bodies(&remove);
                        saved.and(body).err()
                    })
                }
                None => Task::none(),
            },
            Effect::LoadHistory { tab, id, entry } => {
                let store = self.history.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let body = store.and_then(|s| s.read_body(entry.id));
                            history_response(&entry, body)
                        })
                        .await
                        .ok()
                    },
                    move |finished| {
                        finished.map(|result| {
                            Message::App(ferry_app::Message::Finished {
                                tab,
                                id,
                                result: Arc::new(result),
                            })
                        })
                    },
                )
                .and_then(Task::done)
            }
            Effect::OpenInBrowser { body, extension } => {
                blocking(move || crate::preview::open_in_browser(&body, extension).err())
            }
            Effect::SaveCookies(jar) => match &self.cookies {
                Some(store) => {
                    let store = Arc::clone(store);
                    blocking(move || store.save(&jar).err().map(|e| e.to_string()))
                }
                None => Task::none(),
            },
            Effect::CopyText(text) => iced::clipboard::write(text),
            Effect::PickDataFile { tab } => {
                Task::perform(load_data_file(), move |picked| match picked {
                    Some((path, result)) => {
                        Message::App(ferry_app::Message::DataFileLoaded { tab, path, result })
                    }
                    None => Message::Ignore,
                })
            }
            Effect::SaveReport {
                tab,
                name,
                contents,
            } => Task::perform(save_report(name, contents), move |saved| match saved {
                Some(result) => Message::App(ferry_app::Message::ReportSaved { tab, result }),
                None => Message::Ignore,
            }),
            effect @ (Effect::Send { .. }
            | Effect::ScriptedSend { .. }
            | Effect::RunStep { .. }) => {
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

    /// Save synchronously, for window close: the session and the cookie
    /// jar (F17-AC6).
    fn save_now(&mut self) {
        if let Some(store) = &self.store
            && let Err(e) = store.save(&self.app.session())
        {
            self.save_error = Some(e.to_string());
        }
        if let Some(store) = &self.cookies
            && let Err(e) = store.save(self.app.cookies())
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

/// Run `save` on a worker thread and report its error, if any.
fn blocking(save: impl FnOnce() -> Option<String> + Send + 'static) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(save)
                .await
                .unwrap_or_else(|e| Some(e.to_string()))
        },
        Message::Saved,
    )
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
/// Ask for a Runner data file and read it (F16-AC3); `None` if cancelled.
async fn load_data_file() -> Option<(std::path::PathBuf, Result<String, String>)> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Select a data file")
        .add_filter("CSV or JSON", &["csv", "json", "txt"])
        .pick_file()
        .await?;
    let path = handle.path().to_path_buf();
    let read = path.clone();
    let result = tokio::task::spawn_blocking(move || std::fs::read_to_string(&read))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))
        .map_err(|e| format!("Could not read {}: {e}", path.display()));
    Some((path, result))
}

/// Ask where to save a run report and write it (F16-AC6); `None` if
/// cancelled.
async fn save_report(name: String, contents: String) -> Option<Result<std::path::PathBuf, String>> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Export run results")
        .set_file_name(name)
        .add_filter("JSON", &["json"])
        .save_file()
        .await?;
    let path = handle.path().to_path_buf();
    let target = path.clone();
    let written = tokio::task::spawn_blocking(move || {
        std::fs::write(&target, contents.as_bytes()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    Some(
        written
            .map(|()| path.clone())
            .map_err(|e| format!("Could not save {}: {e}", path.display())),
    )
}

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

/// Global keyboard shortcuts (F05-AC1, F07-AC1..AC3, F09-AC5, F14-AC4,
/// F06-AC8).
#[must_use]
pub fn shortcut(event: keyboard::Event) -> Option<Message> {
    use ferry_app::Message as M;
    let (key, modifiers) = match event {
        keyboard::Event::KeyPressed { key, modifiers, .. } => (key, modifiers),
        keyboard::Event::ModifiersChanged(modifiers) => return Some(Message::Modifiers(modifiers)),
        keyboard::Event::KeyReleased { .. } => return None,
    };
    let message = match key.as_ref() {
        Key::Named(Named::Enter) if modifiers.command() => M::SendPressed,
        Key::Named(Named::Tab) if modifiers.control() && modifiers.shift() => M::PreviousTab,
        Key::Named(Named::Tab) if modifiers.control() => M::NextTab,
        Key::Character("t") if modifiers.command() => M::NewTab,
        Key::Character("w") if modifiers.command() => M::CloseActiveTab,
        Key::Character("b") if modifiers.command() => M::ToggleSidebar,
        Key::Character("s") if modifiers.command() => M::Save,
        Key::Character("f") if modifiers.command() => M::FindOpen,
        // Matches of an open find bar; ignored otherwise.
        Key::Named(Named::ArrowDown) => M::FindNext,
        Key::Named(Named::ArrowUp) => M::FindPrevious,
        Key::Named(Named::Escape) => M::DialogCancel,
        _ => return None,
    };
    Some(Message::App(message))
}
