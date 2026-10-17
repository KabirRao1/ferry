//! Watching the workspace folder for changes made by other programs
//! (F09-AC10). Events are coalesced, so a burst of writes (a `git pull`)
//! triggers one reload.

use std::path::{Component, Path};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify::{RecursiveMode, Watcher as _};

/// How long to wait for more events before reporting a change.
pub const SETTLE: Duration = Duration::from_millis(150);

// CONTRACT
/// Whether a change at `path` can affect what Ferry shows: anything in the
/// workspace except inside hidden directories (`.git`) and Ferry's own
/// temporary files (`*.tmp`).
#[must_use]
pub fn is_relevant(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let hidden = relative.components().any(|c| match c {
        Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    });
    let temporary = relative
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("tmp"));
    !hidden && !temporary
}

// CONTRACT
/// Watches a folder until dropped.
pub struct Watcher {
    _watcher: notify::RecommendedWatcher,
}

impl Watcher {
    // CONTRACT
    /// Call `on_change` (from a background thread) after relevant changes
    /// under `root` settle.
    ///
    /// # Errors
    /// The reason watching could not start.
    pub fn start(root: &Path, on_change: impl Fn() + Send + 'static) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<()>();
        let watched = root.to_path_buf();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let relevant = match event {
                    Ok(event) => event.paths.iter().any(|p| is_relevant(&watched, p)),
                    // A lost or overflowing event stream: reload to be safe.
                    Err(_) => true,
                };
                if relevant {
                    let _ = tx.send(());
                }
            })
            .map_err(|e| e.to_string())?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;
        thread::Builder::new()
            .name("ferry-watch".into())
            .spawn(move || {
                // Ends when the watcher, and with it the sender, is dropped.
                while rx.recv().is_ok() {
                    while rx.recv_timeout(SETTLE).is_ok() {}
                    on_change();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { _watcher: watcher })
    }
}
