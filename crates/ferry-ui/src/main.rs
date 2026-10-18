//! The `ferry` desktop binary.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use ferry_app::{
    App, Client, Config, CookieStore, HistoryStore, Loaded, Message, RealFs, Restored, Session,
    SessionStore, WorkspaceStore, data_dir,
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
            let Some(started) = started else {
                return Shell::new(App::default(), client.clone(), None);
            };
            let mut shell = Shell::new(started.app, client.clone(), started.session);
            if let Some(workspace) = started.workspace {
                shell = shell.with_workspace(workspace);
            }
            if let Some((history, cookies)) = started.data {
                shell = shell.with_data(history, cookies);
            }
            shell
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

/// What startup restored, and where to save it.
struct Started {
    app: App,
    session: Option<SessionStore<RealFs>>,
    workspace: Option<WorkspaceStore<RealFs>>,
    data: Option<(HistoryStore<RealFs>, CookieStore<RealFs>)>,
}

impl Started {
    fn bare(app: App) -> Self {
        Self {
            app,
            session: None,
            workspace: None,
            data: None,
        }
    }
}

/// The restored workbench, history and cookie jar, where to save them, and
/// the workspace (F07-AC5, F09-AC1, F10-AC6, F17-AC6).
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
        return Started::bare(app);
    }
    let dir = std::env::var_os("FERRY_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| data_dir(|name| std::env::var(name).ok()));
    let Some(dir) = dir else {
        eprintln!("ferry: no data directory (set FERRY_DATA_DIR); nothing will be saved");
        return Started::bare(App::default());
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
    let history = HistoryStore::new(RealFs, &dir);
    let cookies = CookieStore::new(RealFs, &dir);
    let now = now_millis();
    let app = App::restore(session)
        .with_workspace(workspace.load())
        .with_workspace_root(root.clone())
        .with_history(restored("history", history.load()))
        .with_cookies(restored("cookies", cookies.load(now)));
    Started {
        app,
        session: Some(store),
        workspace: Some(workspace),
        data: Some((history, cookies)),
    }
}

/// Milliseconds since the Unix epoch.
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The loaded value, reporting an unreadable file (F10-AC6, F17-AC6).
fn restored<T: Default>(what: &str, loaded: Restored<T>) -> T {
    if let Restored::Unreadable { reason, backup } = &loaded {
        let kept = backup.as_ref().map_or_else(
            || "it could not be moved aside".to_owned(),
            |b| format!("it was moved to {}", b.display()),
        );
        eprintln!("ferry: could not restore {what} ({reason}); {kept}; starting empty");
    }
    loaded.or_default()
}

/// `FERRY_WORKSPACE`, else the folder chosen with `Open Workspace…`, else
/// `<data dir>/workspace` (F09-AC1).
fn workspace_root(data: &Path, chosen: Option<&Path>) -> PathBuf {
    std::env::var_os("FERRY_WORKSPACE")
        .map(PathBuf::from)
        .or_else(|| chosen.map(Path::to_path_buf))
        .unwrap_or_else(|| data.join(WORKSPACE_DIR))
}
