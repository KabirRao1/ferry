//! The `ferry` desktop binary.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use ferry_app::{App, Client, Config, Loaded, Message, RealFs, SessionStore, data_dir};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::Shell;

fn main() -> iced::Result {
    let client = match Client::new(Config::default()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ferry: could not start the HTTP client: {error}");
            std::process::exit(1);
        }
    };
    // iced may call the boot function more than once; only the first call
    // gets the restored session and the store.
    let first = Mutex::new(Some(startup()));
    iced::application(
        move || {
            let (app, store) = first
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_default();
            Shell::new(app, client.clone(), store)
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

/// The restored workbench and where to save it (F07-AC5).
///
/// Under `FERRY_PROBE` (performance budgets) nothing is read or written, and
/// `FERRY_PROBE_TABS` opens that many tabs. `FERRY_DATA_DIR` overrides the
/// data directory.
fn startup() -> (App, Option<SessionStore<RealFs>>) {
    if std::env::var_os("FERRY_PROBE").is_some() {
        let mut app = App::default();
        let tabs: usize = std::env::var("FERRY_PROBE_TABS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(1);
        for _ in 1..tabs {
            app.update(Message::NewTab);
        }
        return (app, None);
    }
    let dir = std::env::var_os("FERRY_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| data_dir(|name| std::env::var(name).ok()));
    let Some(dir) = dir else {
        eprintln!("ferry: no data directory (set FERRY_DATA_DIR); tabs will not be saved");
        return (App::default(), None);
    };
    let store = SessionStore::new(RealFs, &dir);
    let app = match store.load() {
        Loaded::Missing => App::default(),
        Loaded::Restored(session) => App::restore(session),
        Loaded::Unreadable { reason, backup } => {
            let kept = backup.map_or_else(
                || "it could not be moved aside".to_owned(),
                |b| format!("it was moved to {}", b.display()),
            );
            eprintln!(
                "ferry: could not restore {} ({reason}); {kept}",
                store.path().display()
            );
            App::default()
        }
    };
    (app, Some(store))
}
