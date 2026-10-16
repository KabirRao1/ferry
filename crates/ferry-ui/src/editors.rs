//! Widget state that iced keeps outside the view: the method combo box and
//! the raw body editor of each tab. It mirrors `ferry-app` state and is
//! re-synced after every update, so the app stays the source of truth.

use std::collections::HashMap;

use ferry_app::{App, Method, TabId, Table};
use iced::widget::{combo_box, text_editor};

/// Editor state for one tab.
pub struct TabEditors {
    pub method: combo_box::State<Method>,
    pub raw: text_editor::Content,
    /// The app's `raw_revision` this content was loaded from.
    raw_revision: u64,
}

impl TabEditors {
    fn new(method: &Method, raw: &str, raw_revision: u64) -> Self {
        Self {
            method: combo_box::State::with_selection(Method::STANDARD.to_vec(), Some(method)),
            raw: text_editor::Content::with_text(raw),
            raw_revision,
        }
    }
}

/// Editor state for every open tab.
#[derive(Default)]
pub struct Editors {
    tabs: HashMap<TabId, TabEditors>,
    /// Bulk-edit text of each table in bulk mode (F02-AC4).
    bulk: HashMap<(TabId, Table), text_editor::Content>,
}

impl Editors {
    /// State for every tab of `app`.
    #[must_use]
    pub fn for_app(app: &App) -> Self {
        let mut editors = Self::default();
        editors.sync(app);
        editors
    }

    /// Create state for new tabs, drop it for closed ones, and reload the
    /// raw editor when the app rewrote the body itself (Beautify).
    pub fn sync(&mut self, app: &App) {
        self.tabs
            .retain(|id, _| app.tabs().iter().any(|t| t.id == *id));
        for tab in app.tabs() {
            let draft = &tab.draft;
            let editors = self.tabs.entry(tab.id).or_insert_with(|| {
                TabEditors::new(&draft.method, &draft.body.raw, tab.raw_revision)
            });
            if editors.raw_revision != tab.raw_revision {
                editors.raw = text_editor::Content::with_text(&draft.body.raw);
                editors.raw_revision = tab.raw_revision;
            }
        }
        self.bulk.retain(|(id, table), _| {
            app.tabs()
                .iter()
                .any(|t| t.id == *id && t.bulk_text(*table).is_some())
        });
        for tab in app.tabs() {
            for (table, text) in &tab.bulk {
                self.bulk
                    .entry((tab.id, *table))
                    .or_insert_with(|| text_editor::Content::with_text(text));
            }
        }
    }

    /// The bulk-edit editor of `table` in tab `id`, if it is in bulk mode.
    #[must_use]
    pub fn bulk(&self, id: TabId, table: Table) -> Option<&text_editor::Content> {
        self.bulk.get(&(id, table))
    }

    pub fn bulk_mut(&mut self, id: TabId, table: Table) -> Option<&mut text_editor::Content> {
        self.bulk.get_mut(&(id, table))
    }

    /// The active tab's editors.
    #[must_use]
    pub fn get(&self, id: TabId) -> Option<&TabEditors> {
        self.tabs.get(&id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut TabEditors> {
        self.tabs.get_mut(&id)
    }
}
