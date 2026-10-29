//! Modal dialogs: discard changes (F07-AC2), Save (F09-AC5), Rename
//! (F09-AC7, F08-AC8), Delete, Clear history (F10-AC5), the cookie
//! manager (F17-AC5) and Import (F11-AC2, AC4).

use ferry_app::{
    App, CookieManager, Dialog, Item, ItemKind, Message as AppMessage, Node, Target, format,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, button, center, column, container, row, scrollable, space, text, text_editor,
    text_input,
};
use iced::{Alignment, Fill, Length};

use crate::editors::Editors;
use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::{Element, theme};

/// Id of the dialog's name field, for focus.
pub const NAME_INPUT: &str = "dialog-name";
pub const CLEAR_HISTORY: &str = "Clear all history?";
pub const MANAGE_COOKIES: &str = "Manage Cookies";
pub const NO_DOMAINS: &str = "No cookies yet";
pub const ADD_COOKIE: &str = "Add Cookie";
pub const CHOOSE_FILE: &str = "Choose File…";
pub const IMPORT_HINT: &str =
    "Paste a Postman collection or environment, or a cURL command, or choose a file.";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

/// The dialog to show over the window, if any.
#[must_use]
pub fn view<'a>(state: &'a App, editors: &'a Editors) -> Option<Element<'a, Message>> {
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
        Dialog::ClearHistory => clear_history(),
        Dialog::Cookies(manager) => cookies(state, manager),
        Dialog::Import { error, summary, .. } => {
            import(editors.import(), error.as_deref(), summary)
        }
    };
    Some(dialog)
}

/// Pasted text or a file to import, then what was imported (F11-AC2, AC4).
fn import<'a>(
    content: Option<&'a text_editor::Content>,
    error: Option<&'a str>,
    summary: &'a [String],
) -> Element<'a, Message> {
    let title = text("Import").size(16);
    if let [first, skipped @ ..] = summary {
        let mut body = column![title, text(first.as_str()).size(TEXT)].spacing(12);
        if !skipped.is_empty() {
            let lines = skipped.iter().map(|line| {
                text(format!("•  {line}"))
                    .size(SMALL)
                    .style(theme::muted)
                    .into()
            });
            body = body.push(text("Not carried over").size(SMALL).style(theme::muted));
            body = body
                .push(scrollable(Column::with_children(lines).spacing(4)).height(Length::Shrink));
        }
        body = body.push(row![
            space().width(Fill),
            button(text("Done").size(TEXT))
                .on_press(app(AppMessage::DialogConfirm))
                .padding([6, 14])
                .style(theme::primary),
        ]);
        return frame(body, 560.0);
    }
    let editor: Element<'a, Message> = match content {
        Some(content) => text_editor(content)
            .on_action(Message::ImportAction)
            .placeholder("Paste here")
            .size(SMALL)
            .font(iced::Font::MONOSPACE)
            .height(Length::Fixed(220.0))
            .padding(8)
            .into(),
        None => space().height(Length::Fixed(220.0)).into(),
    };
    let mut body = column![
        title,
        text(IMPORT_HINT).size(TEXT).style(theme::muted),
        button(text(CHOOSE_FILE).size(TEXT))
            .on_press(app(AppMessage::ImportChooseFile))
            .padding([6, 14])
            .style(theme::secondary),
        editor,
    ]
    .spacing(12);
    if let Some(error) = error {
        body = body.push(text(error).size(SMALL).style(theme::error));
    }
    body = body.push(buttons(
        "Import",
        app(AppMessage::DialogConfirm),
        app(AppMessage::DialogCancel),
        false,
    ));
    frame(body, 560.0)
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

fn clear_history<'a>() -> Element<'a, Message> {
    frame(
        column![
            text(CLEAR_HISTORY).size(16),
            text("Every request in history and its response is removed.")
                .size(TEXT)
                .style(theme::muted),
            buttons(
                "Clear",
                app(AppMessage::DialogConfirm),
                app(AppMessage::DialogCancel),
                true
            ),
        ]
        .spacing(12),
        420.0,
    )
}

/// `Session`, or the expiry as an HTTP date (F17-AC4).
#[must_use]
pub fn expires_label(expires: Option<i64>) -> String {
    expires.map_or_else(|| "Session".to_owned(), format::http_date)
}

/// The cookie manager: domains on the left, the selected domain's cookies
/// on the right, each editable as `Set-Cookie` text (F17-AC5).
fn cookies<'a>(state: &'a App, manager: &'a CookieManager) -> Element<'a, Message> {
    let add_domain = row![
        text_input("Type a domain name", &manager.new_domain)
            .on_input(|s| app(AppMessage::CookieNewDomain(s)))
            .on_submit(app(AppMessage::CookieAddDomain))
            .size(TEXT)
            .padding([6, 8])
            .style(theme::input),
        button(text("Add").size(TEXT))
            .on_press(app(AppMessage::CookieAddDomain))
            .padding([6, 14])
            .style(theme::secondary),
    ]
    .spacing(8);
    let domains = state.cookie_domains();
    let domain_list: Element<'a, Message> = if domains.is_empty() {
        container(text(NO_DOMAINS).size(TEXT).style(theme::muted))
            .padding(10)
            .into()
    } else {
        let rows = domains.into_iter().map(|domain| {
            let selected = manager.selected.as_deref() == Some(domain.as_str());
            row![
                button(text(domain.clone()).size(TEXT).wrapping(Wrapping::None))
                    .on_press(app(AppMessage::CookieDomain(domain.clone())))
                    .padding([5, 8])
                    .width(Fill)
                    .style(theme::choice(selected)),
                button(text("×").size(TEXT).style(theme::muted))
                    .on_press(app(AppMessage::CookieDeleteDomain(domain)))
                    .padding([2, 6])
                    .style(theme::flat(false)),
            ]
            .align_y(Alignment::Center)
            .into()
        });
        scrollable(Column::with_children(rows).spacing(1)).into()
    };
    let detail: Element<'a, Message> = match &manager.selected {
        None => container(
            text("Select a domain to see its cookies.")
                .size(TEXT)
                .style(theme::muted),
        )
        .padding(10)
        .into(),
        Some(domain) => domain_cookies(state, manager, domain),
    };
    frame(
        column![
            text(MANAGE_COOKIES).size(16),
            add_domain,
            row![
                container(domain_list)
                    .width(220)
                    .height(320)
                    .style(theme::raised),
                container(detail).width(Fill).height(320),
            ]
            .spacing(12),
            row![
                space().width(Fill),
                button(text("Close").size(TEXT))
                    .on_press(app(AppMessage::DialogCancel))
                    .padding([6, 14])
                    .style(theme::secondary),
            ],
        ]
        .spacing(12),
        760.0,
    )
}

fn domain_cookies<'a>(
    state: &'a App,
    manager: &'a CookieManager,
    domain: &'a str,
) -> Element<'a, Message> {
    let mut list = column![].spacing(4);
    for cookie in state.cookies().for_domain(domain) {
        let key = (cookie.name.clone(), cookie.path.clone());
        let summary = column![
            text(format!("{}={}", cookie.name, cookie.value))
                .size(TEXT)
                .wrapping(Wrapping::None),
            text(format!(
                "Path {} · {}{}{}",
                cookie.path,
                expires_label(cookie.expires),
                if cookie.secure { " · Secure" } else { "" },
                if cookie.http_only { " · HttpOnly" } else { "" },
            ))
            .size(SMALL)
            .style(theme::faint)
            .wrapping(Wrapping::None),
        ]
        .spacing(2);
        list = list.push(
            container(
                row![
                    container(summary).width(Fill).clip(true),
                    button(text("Edit").size(SMALL))
                        .on_press(app(AppMessage::CookieEdit(Some(key.clone()))))
                        .padding([3, 8])
                        .style(theme::secondary),
                    button(text("Delete").size(SMALL))
                        .on_press(app(AppMessage::CookieDelete(key.0, key.1)))
                        .padding([3, 8])
                        .style(theme::secondary),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .padding([6, 8])
            .style(theme::raised),
        );
    }
    let mut content = column![
        row![
            text(domain).size(TEXT),
            space().width(Fill),
            button(text(ADD_COOKIE).size(SMALL))
                .on_press(app(AppMessage::CookieEdit(None)))
                .padding([3, 8])
                .style(theme::secondary),
        ]
        .align_y(Alignment::Center),
        scrollable(list).height(Fill),
    ]
    .spacing(8);
    if let Some(editor) = &manager.editor {
        let mut field = column![
            text(if editor.original.is_some() {
                "Edit cookie"
            } else {
                "New cookie"
            })
            .size(SMALL)
            .style(theme::muted),
            text_input("name=value; Path=/", &editor.text)
                .on_input(|s| app(AppMessage::CookieText(s)))
                .on_submit(app(AppMessage::CookieSave))
                .size(TEXT)
                .padding([6, 8])
                .style(theme::input),
        ]
        .spacing(4);
        if let Some(error) = &editor.error {
            field = field.push(text(error.as_str()).size(SMALL).style(theme::error));
        }
        field = field.push(row![
            space().width(Fill),
            button(text("Cancel").size(SMALL))
                .on_press(app(AppMessage::CookieDomain(domain.to_owned())))
                .padding([4, 10])
                .style(theme::secondary),
            space().width(6),
            button(text("Save").size(SMALL))
                .on_press(app(AppMessage::CookieSave))
                .padding([4, 10])
                .style(theme::primary),
        ]);
        content = content.push(container(field).padding(8).style(theme::raised));
    }
    content.into()
}
