//! The Import dialog (F11-AC2..AC5) and `Export…` (F11-AC6).

use std::sync::Arc;

use ferry_core::collection::{
    self, COLLECTION_FILE, COLLECTIONS_DIR, CollectionFile, FOLDER_FILE, ORDER_FILE, REQUEST_EXT,
};
use ferry_core::environment::VariablesFile;
use ferry_core::import::{self, Collection, Entry, Import, postman};
use ferry_core::names;
use ferry_core::workspace::{ENVIRONMENTS_DIR, Op};

use crate::app::{App, Effect, Message};
use crate::doc::Doc;
use crate::workspace::{
    Dialog, Target, as_strs, children, env_file_names, join, local_op, variables_ops,
    workspace_effect,
};

/// `1 request`, `2 requests`.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Files and directories for `items` inside `dir`, with an order file
/// keeping them in the imported order.
fn entry_ops(dir: &str, items: &[Entry], ops: &mut Vec<Op>) {
    let mut order: Vec<String> = Vec::new();
    for item in items {
        match item {
            Entry::Folder { file, items } => {
                let name = names::item_file(&file.name, "", &as_strs(&order));
                let sub = join(dir, &name);
                ops.push(Op::CreateDir(sub.clone()));
                ops.push(Op::Write {
                    path: join(&sub, FOLDER_FILE),
                    contents: collection::write_folder(file),
                });
                entry_ops(&sub, items, ops);
                order.push(name);
            }
            Entry::Request(file) => {
                let name = names::item_file(&file.name, REQUEST_EXT, &as_strs(&order));
                ops.push(Op::Write {
                    path: join(dir, &name),
                    contents: collection::write_request(file),
                });
                order.push(name);
            }
        }
    }
    if !order.is_empty() {
        ops.push(Op::Write {
            path: join(dir, ORDER_FILE),
            contents: collection::write_order(&order),
        });
    }
}

impl App {
    pub(crate) fn import_message(&mut self, message: Message) -> Option<Effect> {
        match message {
            Message::OpenImport => {
                self.dialog = Some(Dialog::Import {
                    text: String::new(),
                    error: None,
                    summary: Vec::new(),
                });
            }
            Message::ImportText(new) => {
                if let Some(Dialog::Import { text, error, .. }) = &mut self.dialog {
                    *text = new;
                    *error = None;
                }
            }
            Message::ImportChooseFile => {
                return matches!(self.dialog, Some(Dialog::Import { .. }))
                    .then_some(Effect::PickImportFile);
            }
            Message::ImportFileLoaded(Ok(text)) => {
                if matches!(self.dialog, Some(Dialog::Import { .. })) {
                    return self.run_import(&text);
                }
            }
            Message::ImportFileLoaded(Err(reason)) => self.import_error(reason),
            Message::Export(target) => {
                self.menu = None;
                return self.export(&target);
            }
            Message::Exported(result) => match result {
                Ok(path) => self.notice = Some(format!("Exported to {}", path.display())),
                Err(error) => self.status = Some(error),
            },
            _ => {}
        }
        None
    }

    fn import_error(&mut self, reason: String) {
        if let Some(Dialog::Import { error, .. }) = &mut self.dialog {
            *error = Some(reason);
        }
    }

    /// Import `text`: a collection or environment is written to the
    /// workspace and the dialog shows what happened; a cURL command opens in
    /// a new tab.
    pub(crate) fn run_import(&mut self, text: &str) -> Option<Effect> {
        let imported = match import::import(text) {
            Ok(imported) => imported,
            Err(e) => {
                self.import_error(e.to_string());
                return None;
            }
        };
        let (first, effect) = match imported.import {
            Import::Request(draft) => {
                self.dialog = None;
                self.push_doc(Doc::Request);
                self.tabs[self.active].set_draft(draft);
                self.session_changed = true;
                return None;
            }
            Import::Collection(c) => {
                let (requests, folders) = c.counts();
                let (dir, name, ops) = self.collection_ops_for(&c);
                self.expanded.insert(dir);
                (
                    format!(
                        "Imported {name}: {} in {}",
                        count(requests, "request"),
                        count(folders, "folder")
                    ),
                    workspace_effect("import the collection", ops),
                )
            }
            Import::Environment(file) => {
                let siblings = env_file_names(&self.workspace, None);
                let wanted = file.name.as_deref().unwrap_or("Imported Environment");
                let file_name = names::item_file(wanted, ".toml", &as_strs(&siblings));
                let path = join(ENVIRONMENTS_DIR, &file_name);
                let name = file_name
                    .strip_suffix(".toml")
                    .unwrap_or(&file_name)
                    .to_owned();
                let first = format!(
                    "Imported environment {name}: {}",
                    count(file.variables.len(), "variable")
                );
                let file = VariablesFile {
                    name: Some(name),
                    ..file
                };
                (
                    first,
                    workspace_effect("import the environment", variables_ops(&path, &file)),
                )
            }
        };
        let mut summary = vec![first];
        summary.extend(imported.skipped);
        self.dialog = Some(Dialog::Import {
            text: String::new(),
            error: None,
            summary,
        });
        effect
    }

    /// The new collection's directory, its name (made unique among the
    /// collections, F09-AC8) and the operations creating it.
    fn collection_ops_for(&self, c: &Collection) -> (String, String, Vec<Op>) {
        let siblings = children(&self.workspace, COLLECTIONS_DIR);
        let dir_name = names::item_file(&c.file.name, "", &as_strs(&siblings));
        let dir = join(COLLECTIONS_DIR, &dir_name);
        let file = CollectionFile {
            name: dir_name.clone(),
            ..c.file.clone()
        };
        let settings = join(&dir, COLLECTION_FILE);
        let mut ops = vec![
            Op::CreateDir(dir.clone()),
            Op::Write {
                path: settings.clone(),
                contents: collection::write_collection(&file),
            },
        ];
        if let Some(local) = collection::write_collection_local(&file) {
            ops.push(local_op(&settings, Some(local)));
        }
        entry_ops(&dir, &c.items, &mut ops);
        (dir, dir_name, ops)
    }

    fn export(&mut self, target: &Target) -> Option<Effect> {
        let ws = Arc::clone(&self.workspace);
        let (name, contents) = match target {
            Target::Item(path) => {
                let c = import::from_tree(collection::find(&ws.collections, path)?)?;
                (
                    format!("{}.postman_collection.json", names::sanitize(&c.file.name)),
                    postman::export_collection(&c),
                )
            }
            Target::Environment(path) => {
                let entry = ws.environment(path)?;
                let file = entry.file.as_ref().ok()?;
                (
                    format!("{}.postman_environment.json", names::sanitize(entry.name())),
                    postman::export_environment(entry.name(), &file.variables),
                )
            }
            Target::Example(..) => return None,
        };
        Some(Effect::SaveExport { name, contents })
    }
}
