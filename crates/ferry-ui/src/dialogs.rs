//! Modal dialogs: discard changes (F07-AC2), Save (F09-AC5), Rename
//! (F09-AC7, F08-AC8) and Delete.

use ferry_app::{App, Dialog, Item, ItemKind, Message as AppMessage, Node, Target};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, button, center, column, container, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Fill, Length};

use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::{Element, theme};

/// Id of the dialog's name field, for focus.
pub const NAME_INPUT: &str = "dialog-name";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

/// The dialog to show over the window, if any.
#[must_use]
pub fn view(state: &App) -> Option<Element<'_, Message>> {
    if state.confirm_close().is_some() {
        return Some(confirm_close());
    }
    let dialog = match state.dialog()? {
        Dialog::SaveAs {
            name,
            target,
            new_collection,
            error,
        } => save_as(
            state,
            name,
            target.as_deref(),
            new_collection,
            error.as_deref(),
        ),
        Dialog::Rename { name, error, .. } => rename(name, error.as_deref()),
        Dialog::ConfirmDelete { target, name } => confirm_delete(target, name),
    };
    Some(dialog)
}

fn frame<'a>(content: impl Into<Element<'a, Message>>, width: f32) -> Element<'a, Message> {
    let dialog = container(content)
        .padding(20)
        .width(Length::Fixed(width))
        .style(theme::dialog);
    container(center(dialog))
        .width(Fill)
        .height(Fill)
        .style(theme::scrim)
        .into()
}

fn buttons<'a>(
    confirm: &'a str,
    on_confirm: Message,
    on_cancel: Message,
    danger: bool,
) -> Element<'a, Message> {
    row![
        space().width(Fill),
        button(text("Cancel").size(TEXT))
            .on_press(on_cancel)
            .padding([6, 14])
            .style(theme::secondary),
        button(text(confirm).size(TEXT))
            .on_press(on_confirm)
            .padding([6, 14])
            .style(if danger {
                theme::danger
            } else {
                theme::primary
            }),
    ]
    .spacing(8)
    .into()
}

fn confirm_close<'a>() -> Element<'a, Message> {
    frame(
        column![
            text("Discard unsaved changes?").size(16),
            text("This tab has changes that are not saved. Closing it discards them.")
                .size(TEXT)
                .style(theme::muted),
            buttons(
                "Discard",
                app(AppMessage::ConfirmClose(true)),
                app(AppMessage::ConfirmClose(false)),
                true
            ),
        ]
        .spacing(12),
        420.0,
    )
}

fn name_field<'a>(name: &'a str, error: Option<&'a str>) -> Element<'a, Message> {
    let mut field = column![
        text("Name").size(SMALL).style(theme::muted),
        text_input("Name", name)
            .id(iced::widget::Id::new(NAME_INPUT))
            .on_input(|s| app(AppMessage::DialogInput(s)))
            .on_submit(app(AppMessage::DialogConfirm))
            .size(TEXT)
            .padding([6, 8])
            .style(theme::input),
    ]
    .spacing(4);
    if let Some(error) = error {
        field = field.push(text(error).size(SMALL).style(theme::error));
    }
    field.into()
}

/// Every collection and folder, depth first, with its depth.
fn containers(items: &[Item], depth: u16, out: &mut Vec<(u16, String, String)>) {
    for item in items {
        if item.kind() == ItemKind::Request {
            continue;
        }
        if !matches!(item.node, Node::Broken { .. }) {
            out.push((depth, item.path.clone(), item.name().to_owned()));
        }
        containers(&item.items, depth + 1, out);
    }
}

fn save_as<'a>(
    state: &'a App,
    name: &'a str,
    target: Option<&'a str>,
    new_collection: &'a str,
    error: Option<&'a str>,
) -> Element<'a, Message> {
    let mut places = Vec::new();
    containers(&state.workspace().collections, 0, &mut places);
    let destination: Element<'a, Message> = if places.is_empty() {
        column![
            text("There are no collections yet. Ferry will create this one:")
                .size(SMALL)
                .style(theme::muted),
            text_input("New Collection", new_collection)
                .on_input(|s| app(AppMessage::DialogCollectionName(s)))
                .size(TEXT)
                .padding([6, 8])
                .style(theme::input),
        ]
        .spacing(4)
        .into()
    } else {
        let rows = places.into_iter().map(|(depth, path, label)| {
            let selected = target == Some(path.as_str());
            button(
                row![
                    space().width(f32::from(depth) * 14.0),
                    text(label).size(TEXT).wrapping(Wrapping::None),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(app(AppMessage::DialogPick(path)))
            .padding([5, 8])
            .width(Fill)
            .style(theme::choice(selected))
            .into()
        });
        column![
            text("Save to").size(SMALL).style(theme::muted),
            container(scrollable(Column::with_children(rows).spacing(1)))
                .max_height(240)
                .style(theme::raised),
        ]
        .spacing(4)
        .into()
    };
    frame(
        column![
            text("Save Request").size(16),
            name_field(name, error),
            destination,
            buttons(
                "Save",
                app(AppMessage::DialogConfirm),
                app(AppMessage::DialogCancel),
                false
            ),
        ]
        .spacing(12),
        460.0,
    )
}

fn rename<'a>(name: &'a str, error: Option<&'a str>) -> Element<'a, Message> {
    frame(
        column![
            text("Rename").size(16),
            name_field(name, error),
            buttons(
                "Rename",
                app(AppMessage::DialogConfirm),
                app(AppMessage::DialogCancel),
                false
            ),
        ]
        .spacing(12),
        420.0,
    )
}

/// `Delete "<name>"?`, or `Delete environment "<name>"?` (F08-AC8, F09-AC7).
#[must_use]
pub fn delete_question(target: &Target, name: &str) -> String {
    match target {
        Target::Environment(_) => format!("Delete environment \"{name}\"?"),
        _ => format!("Delete \"{name}\"?"),
    }
}

fn confirm_delete<'a>(target: &'a Target, name: &'a str) -> Element<'a, Message> {
    let detail = match target {
        Target::Item(_) => "Its files are removed from the workspace folder.",
        Target::Environment(_) => "Its file, and its local file with secrets, are removed.",
        Target::Example(..) => "The example is removed from its request.",
    };
    frame(
        column![
            text(delete_question(target, name)).size(16),
            text(detail).size(TEXT).style(theme::muted),
            buttons(
                "Delete",
                app(AppMessage::DialogConfirm),
                app(AppMessage::DialogCancel),
                true
            ),
        ]
        .spacing(12),
        420.0,
    )
}
