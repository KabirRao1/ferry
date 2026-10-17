//! The `ferry` desktop binary.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use ferry_app::{
    App, Client, Config, Loaded, Message, RealFs, Session, SessionStore, WorkspaceStore, data_dir,
};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::Shell;

/// Name of the default workspace folder inside the data directory.
const WORKSPACE_DIR: &str = "workspace";

fn main() -> iced::Result {
    let client = match Client::new(Config::default()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ferry: could not start the HTTP client: {error}");
            std::process::exit(1);
        }
    };
    // iced may call the boot function more than once; only the first call
    // gets the restored session and the stores.
    let first = Mutex::new(Some(startup()));
    iced::application(
        move || {
            let started = first.lock().unwrap_or_else(PoisonError::into_inner).take();
            let Some((app, store, workspace)) = started else {
                return Shell::new(App::default(), client.clone(), None);
            };
            let shell = Shell::new(app, client.clone(), store);
            match workspace {
                Some(workspace) => shell.with_workspace(workspace),
                None => shell,
            }
        },
        Shell::update,
        Shell::view,
    )
    .title("Ferry")
    .theme(Shell::theme)
    .subscription(Shell::subscription)
    .settings(iced::Settings {
        default_text_size: TEXT.into(),
        ..iced::Settings::default()
    })
    .exit_on_close_request(false)
    .window_size((1280.0, 800.0))
    .run()
}

type Started = (
    App,
    Option<SessionStore<RealFs>>,
    Option<WorkspaceStore<RealFs>>,
);

/// The restored workbench, where to save it, and the workspace (F07-AC5,
/// F09-AC1).
///
/// Under `FERRY_PROBE` (performance budgets) nothing is read or written, and
/// `FERRY_PROBE_TABS` opens that many tabs. `FERRY_DATA_DIR` overrides the
/// data directory and `FERRY_WORKSPACE` the workspace folder.
fn startup() -> Started {
    if std::env::var_os("FERRY_PROBE").is_some() {
        let mut app = App::default();
        let tabs: usize = std::env::var("FERRY_PROBE_TABS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(1);
        for _ in 1..tabs {
            app.update(Message::NewTab);
        }
        return (app, None, None);
    }
    let dir = std::env::var_os("FERRY_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| data_dir(|name| std::env::var(name).ok()));
    let Some(dir) = dir else {
        eprintln!("ferry: no data directory (set FERRY_DATA_DIR); nothing will be saved");
        return (App::default(), None, None);
    };
    let store = SessionStore::new(RealFs, &dir);
    let session = match store.load() {
        Loaded::Missing => Session::default(),
        Loaded::Restored(session) => session,
        Loaded::Unreadable { reason, backup } => {
            let kept = backup.map_or_else(
                || "it could not be moved aside".to_owned(),
                |b| format!("it was moved to {}", b.display()),
            );
            eprintln!(
                "ferry: could not restore {} ({reason}); {kept}",
                store.path().display()
            );
            Session::default()
        }
    };
    let root = workspace_root(&dir, session.workspace.as_deref());
    let workspace = WorkspaceStore::new(RealFs, &root);
    if let Err(error) = workspace.init() {
        eprintln!(
            "ferry: could not prepare the workspace {}: {error}",
            root.display()
        );
    }
    let app = App::restore(session)
        .with_workspace(workspace.load())
        .with_workspace_root(root.clone());
    (app, Some(store), Some(workspace))
}

/// `FERRY_WORKSPACE`, else the folder chosen with `Open Workspace…`, else
/// `<data dir>/workspace` (F09-AC1).
fn workspace_root(data: &Path, chosen: Option<&Path>) -> PathBuf {
    std::env::var_os("FERRY_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| chosen.map(Path::to_path_buf))
        .unwrap_or_else(|| data.join(WORKSPACE_DIR))
}
