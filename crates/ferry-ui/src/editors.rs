//! Widget state that iced keeps outside the view: the method combo box and
//! the multi-line editors of each tab. It mirrors `ferry-app` state and is
//! re-synced after every update, so the app stays the source of truth
//! (ADR-0031).

use std::collections::HashMap;

use ferry_app::{App, Doc, Method, ScriptPhase, Scripts, Tab, TabId, Table};
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

/// The multi-line field of a non-request tab: a collection's or folder's
/// description, or an example's body.
struct DocEditor {
    content: text_editor::Content,
    /// The text last loaded into or typed in `content`.
    known: String,
}

/// The text a non-request tab edits in a multi-line editor.
fn doc_text(tab: &Tab) -> Option<&str> {
    match &tab.doc {
        Doc::Container(c) => Some(&c.draft.description),
        Doc::Example(e) => Some(&e.draft.body),
        Doc::Request | Doc::Environment(_) => None,
    }
}

/// The scripts a tab edits: a request's, collection's or folder's.
fn scripts(tab: &Tab) -> Option<&Scripts> {
    match &tab.doc {
        Doc::Request => Some(&tab.draft.scripts),
        Doc::Container(c) => Some(&c.draft.scripts),
        Doc::Environment(_) | Doc::Example(_) => None,
    }
}

const PHASES: [ScriptPhase; 2] = [ScriptPhase::PreRequest, ScriptPhase::PostResponse];

/// Keep `editor` showing `text`, reloading it if the app changed the text.
fn follow(
    editors: &mut HashMap<(TabId, ScriptPhase), DocEditor>,
    key: (TabId, ScriptPhase),
    text: &str,
) {
    let editor = editors.entry(key).or_insert_with(|| DocEditor {
        content: text_editor::Content::with_text(text),
        known: text.to_owned(),
    });
    if editor.known != text {
        editor.content = text_editor::Content::with_text(text);
        text.clone_into(&mut editor.known);
    }
}

/// Apply `action` to `editor`, returning the new text if it was an edit.
fn perform(editor: &mut DocEditor, action: text_editor::Action) -> Option<String> {
    let is_edit = action.is_edit();
    editor.content.perform(action);
    if !is_edit {
        return None;
    }
    let text = editor.content.text();
    editor.known.clone_from(&text);
    Some(text)
}

/// Editor state for every open tab.
#[derive(Default)]
pub struct Editors {
    tabs: HashMap<TabId, TabEditors>,
    /// Bulk-edit text of each table in bulk mode (F02-AC4).
    bulk: HashMap<(TabId, Table), text_editor::Content>,
    docs: HashMap<TabId, DocEditor>,
    /// Script editors (F15-AC1).
    scripts: HashMap<(TabId, ScriptPhase), DocEditor>,
}

impl Editors {
    /// State for every tab of `app`.
    #[must_use]
    pub fn for_app(app: &App) -> Self {
        let mut editors = Self::default();
        editors.sync(app);
        editors
    }

    /// Create state for new tabs, drop it for closed ones, and reload
    /// editors whose text the app changed itself (Beautify, a reload from
    /// disk).
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
                editors.method = combo_box::State::with_selection(
                    Method::STANDARD.to_vec(),
                    Some(&draft.method),
                );
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
        self.docs.retain(|id, _| {
            app.tabs()
                .iter()
                .any(|t| t.id == *id && doc_text(t).is_some())
        });
        for tab in app.tabs() {
            let Some(text) = doc_text(tab) else {
                continue;
            };
            let editor = self.docs.entry(tab.id).or_insert_with(|| DocEditor {
                content: text_editor::Content::with_text(text),
                known: text.to_owned(),
            });
            if editor.known != text {
                editor.content = text_editor::Content::with_text(text);
                text.clone_into(&mut editor.known);
            }
        }
        self.scripts.retain(|(id, _), _| {
            app.tabs()
                .iter()
                .any(|t| t.id == *id && scripts(t).is_some())
        });
        for tab in app.tabs() {
            if let Some(s) = scripts(tab) {
                for phase in PHASES {
                    follow(&mut self.scripts, (tab.id, phase), s.get(phase));
                }
            }
        }
    }

    /// The editor of tab `id`'s `phase` script.
    #[must_use]
    pub fn script(&self, id: TabId, phase: ScriptPhase) -> Option<&text_editor::Content> {
        self.scripts.get(&(id, phase)).map(|d| &d.content)
    }

    /// Apply an editor action to tab `id`'s `phase` script, returning the
    /// new text if the action edited it.
    pub fn script_action(
        &mut self,
        id: TabId,
        phase: ScriptPhase,
        action: text_editor::Action,
    ) -> Option<String> {
        perform(self.scripts.get_mut(&(id, phase))?, action)
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

    /// The multi-line editor of a collection, folder or example tab.
    #[must_use]
    pub fn doc(&self, id: TabId) -> Option<&text_editor::Content> {
        self.docs.get(&id).map(|d| &d.content)
    }

    /// Apply an editor action to tab `id`'s multi-line field, returning the
    /// new text if the action edited it.
    pub fn doc_action(&mut self, id: TabId, action: text_editor::Action) -> Option<String> {
        perform(self.docs.get_mut(&id)?, action)
    }
}
