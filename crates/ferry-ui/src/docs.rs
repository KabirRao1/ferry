//! Workbench tabs that are not requests: environments and globals
//! (F08-AC8), collection and folder settings (F09-AC11) and saved examples
//! (F32-AC3).

use ferry_app::{
    App, ContainerDoc, ContainerSection, Doc, Edit, EnvDoc, EnvTarget, ExampleDoc, ItemKind,
    Message as AppMessage, Tab, Table, VarEdit, Variable, collection, environment,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, button, checkbox, column, container, row, scrollable, space, text, text_editor,
    text_input,
};
use iced::{Alignment, Fill, Font, Length, font};

use crate::editors::Editors;
use crate::request::{auth_view, table_rows, underline};
use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::{Element, theme};

pub const SAVE: &str = "Save";

fn edit(e: Edit) -> Message {
    Message::App(AppMessage::Edit(e))
}

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// The view for a non-request tab.
#[must_use]
pub fn view<'a>(state: &'a App, tab: &'a Tab, editors: &'a Editors) -> Element<'a, Message> {
    let content: Element<'a, Message> = match &tab.doc {
        Doc::Request => space().into(),
        Doc::Environment(env) => environment_view(tab, env),
        Doc::Container(c) => container_view(state, tab, c, editors),
        Doc::Example(e) => example_view(tab, e, editors),
    };
    container(content)
        .padding([10, 16])
        .width(Fill)
        .height(Fill)
        .style(theme::surface)
        .into()
}

/// A title with a kind label and the Save button.
fn title_bar<'a>(kind: &'a str, title: Element<'a, Message>, dirty: bool) -> Element<'a, Message> {
    row![
        text(kind).size(SMALL).style(theme::faint),
        title,
        space().width(Fill),
        button(text(SAVE).size(TEXT))
            .on_press(Message::App(AppMessage::Save))
            .padding([6, 16])
            .style(if dirty {
                theme::primary
            } else {
                theme::secondary
            }),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

// --- variables tables -------------------------------------------------------------

/// The variables table of an environment, the globals or a collection.
fn variables<'a>(rows: &'a [Variable], with_secret: bool) -> Element<'a, Message> {
    let mut header = row![
        space().width(28),
        text("Variable").size(SMALL).style(theme::muted).width(Fill),
        text("Value").size(SMALL).style(theme::muted).width(Fill),
    ]
    .spacing(4)
    .padding([4, 0]);
    if with_secret {
        header = header.push(text("Secret").size(SMALL).style(theme::muted).width(60));
    }
    header = header.push(space().width(28));
    let mut list = column![header].spacing(1);
    for (i, v) in rows.iter().enumerate() {
        list = list.push(variable_row(i, Some(v), with_secret));
    }
    list = list.push(variable_row(rows.len(), None, with_secret));
    scrollable(list).height(Fill).into()
}

fn variable_row(i: usize, v: Option<&Variable>, with_secret: bool) -> Element<'_, Message> {
    let blank = v.is_none();
    let (key, value, enabled, secret) = v.map_or(("", "", true, false), |v| {
        (v.key.as_str(), v.value.as_str(), v.enabled, v.secret)
    });
    let enabled: Element<'_, Message> = if blank {
        space().width(28).into()
    } else {
        container(
            checkbox(enabled)
                .on_toggle(move |on| edit(Edit::Var(i, VarEdit::Enabled(on))))
                .size(14),
        )
        .width(28)
        .align_x(Alignment::Center)
        .into()
    };
    let cell = |placeholder: &str, current: &str, f: fn(String) -> VarEdit, secure: bool| {
        text_input(if blank { placeholder } else { "" }, current)
            .on_input(move |s| edit(Edit::Var(i, f(s))))
            .secure(secure)
            .size(TEXT)
            .padding([5, 6])
            .width(Fill)
            .style(theme::cell)
    };
    let mut line = row![
        enabled,
        cell("Variable", key, VarEdit::Key, false),
        cell("Value", value, VarEdit::Value, secret),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if with_secret {
        let toggle: Element<'_, Message> = if blank {
            space().width(60).into()
        } else {
            container(
                checkbox(secret)
                    .on_toggle(move |on| edit(Edit::Var(i, VarEdit::Secret(on))))
                    .size(14),
            )
            .width(60)
            .into()
        };
        line = line.push(toggle);
    }
    let remove: Element<'_, Message> = if blank {
        space().width(28).into()
    } else {
        button(text("×").size(14))
            .on_press(edit(Edit::Var(i, VarEdit::Remove)))
            .padding([0, 8])
            .width(28)
            .style(theme::flat(false))
            .into()
    };
    container(line.push(remove)).style(theme::raised).into()
}

// --- environments -------------------------------------------------------------------

fn environment_view<'a>(tab: &'a Tab, env: &'a EnvDoc) -> Element<'a, Message> {
    let (kind, note) = match &env.target {
        EnvTarget::Globals => (
            "GLOBALS",
            "Global variables apply to every request, below the environment and the collection. Secret values are saved only to globals.local.toml, which git ignores."
                .to_owned(),
        ),
        EnvTarget::Environment(path) => (
            "ENVIRONMENT",
            format!(
                "Secret values are masked and saved only to {}, which git ignores.",
                collection::file_name(&environment::local_path(path))
            ),
        ),
    };
    column![
        title_bar(
            kind,
            text(env.name.as_str()).size(16).font(bold()).into(),
            tab.is_dirty()
        ),
        text(note).size(SMALL).style(theme::muted),
        variables(&env.draft, true),
    ]
    .spacing(10)
    .into()
}

// --- collections and folders ---------------------------------------------------------

fn container_view<'a>(
    state: &'a App,
    tab: &'a Tab,
    c: &'a ContainerDoc,
    editors: &'a Editors,
) -> Element<'a, Message> {
    let kind = if c.kind == ItemKind::Collection {
        "COLLECTION"
    } else {
        "FOLDER"
    };
    let mut sections = vec![ContainerSection::Overview, ContainerSection::Authorization];
    if c.kind == ItemKind::Collection {
        sections.push(ContainerSection::Variables);
    }
    sections.push(ContainerSection::Scripts);
    let tabs = row(sections.into_iter().map(|s| {
        let label = match s {
            ContainerSection::Overview => "Overview".to_owned(),
            ContainerSection::Authorization => "Authorization".to_owned(),
            ContainerSection::Variables => match c.draft.variables.len() {
                0 => "Variables".to_owned(),
                n => format!("Variables ({n})"),
            },
            ContainerSection::Scripts if !c.draft.scripts.is_blank() => "Scripts ●".to_owned(),
            ContainerSection::Scripts => "Scripts".to_owned(),
        };
        column![
            button(text(label).size(TEXT))
                .on_press(edit(Edit::ContainerSection(s)))
                .padding([4, 10])
                .style(theme::sub_tab(s == c.section)),
            underline(s == c.section),
        ]
        .width(Length::Shrink)
        .into()
    }))
    .spacing(2);
    let content: Element<'a, Message> = match c.section {
        ContainerSection::Overview => overview(tab, c, editors),
        ContainerSection::Authorization => {
            let note = (c.draft.auth.kind == ferry_app::AuthKind::Inherit).then(|| {
                match collection::inherited_auth(&state.workspace().collections, &c.path) {
                    Some((auth, source)) => format!(
                        "This {} is using {} from {} \"{}\".",
                        if c.kind == ItemKind::Collection {
                            "collection"
                        } else {
                            "folder"
                        },
                        auth.kind,
                        source.kind,
                        source.name
                    ),
                    None => "A collection has no parent, so Inherit means No Auth.".to_owned(),
                }
            });
            auth_view(&c.draft.auth, note)
        }
        ContainerSection::Variables => column![
            text("Collection variables are saved in collection.toml and apply to every request in the collection, below the active environment. Values set by scripts go to collection.local.toml, which git ignores.")
                .size(SMALL)
                .style(theme::muted),
            variables(&c.draft.variables, false),
        ]
        .spacing(8)
        .into(),
        ContainerSection::Scripts => {
            crate::scripts::view(tab.id, &c.draft.scripts, tab.script_phase, editors)
        }
    };
    column![
        title_bar(
            kind,
            text(c.name.as_str()).size(16).font(bold()).into(),
            tab.is_dirty()
        ),
        tabs,
        container(content).height(Fill).width(Fill),
    ]
    .spacing(10)
    .into()
}

fn overview<'a>(tab: &'a Tab, c: &'a ContainerDoc, editors: &'a Editors) -> Element<'a, Message> {
    let editor: Element<'a, Message> = match editors.doc(tab.id) {
        Some(content) => text_editor(content)
            .on_action(Message::DocAction)
            .placeholder("Describe what this is for, how to authenticate, and anything a teammate should know.")
            .size(TEXT)
            .height(Fill)
            .padding(8)
            .into(),
        None => text(c.draft.description.as_str()).size(TEXT).into(),
    };
    column![
        text("Description").size(SMALL).style(theme::muted),
        editor,
        text(format!("{}  ·  {}", c.path, settings_note(c.kind)))
            .size(SMALL)
            .style(theme::faint),
    ]
    .spacing(6)
    .into()
}

/// Where a collection's or folder's settings are saved.
fn settings_note(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Collection => "Saved in collection.toml",
        _ => "Saved in folder.toml",
    }
}

// --- examples -------------------------------------------------------------------------

fn example_view<'a>(tab: &'a Tab, e: &'a ExampleDoc, editors: &'a Editors) -> Element<'a, Message> {
    let name = text_input("Example name", &e.draft.name)
        .on_input(|s| edit(Edit::ExampleName(s)))
        .size(15)
        .padding([6, 8])
        .width(Length::Fixed(360.0))
        .style(theme::input);
    let method = e.request.method.as_str();
    let request = row![
        text(method)
            .size(SMALL)
            .font(bold())
            .color(theme::method_color(method)),
        text(if e.request.url.is_empty() {
            "No URL was saved with this example"
        } else {
            e.request.url.as_str()
        })
        .size(TEXT)
        .style(theme::muted)
        .wrapping(Wrapping::None),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let status = row![
        text("Status").size(TEXT).width(60),
        text_input("200", &e.draft.status)
            .on_input(|s| edit(Edit::ExampleStatus(s)))
            .size(TEXT)
            .padding([5, 8])
            .width(Length::Fixed(90.0))
            .style(theme::input),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let data: Vec<(&ferry_app::KeyValue, Option<&std::path::Path>)> =
        e.draft.headers.iter().map(|h| (h, None)).collect();
    let headers = column![
        text("Response headers").size(SMALL).style(theme::muted),
        table_rows(Table::ExampleHeaders, &data),
    ]
    .spacing(4);
    let body: Element<'a, Message> = match editors.doc(tab.id) {
        Some(content) => text_editor(content)
            .on_action(Message::DocAction)
            .font(Font::MONOSPACE)
            .size(TEXT)
            .height(Fill)
            .padding(8)
            .into(),
        None => text(e.draft.body.as_str()).into(),
    };
    let mut page: Column<'a, Message> =
        column![title_bar("EXAMPLE", name.into(), tab.is_dirty()), request,].spacing(10);
    if let Some(error) = &e.error {
        page = page.push(text(error.as_str()).size(SMALL).style(theme::error));
    }
    page.push(status)
        .push(container(scrollable(headers)).max_height(200))
        .push(text("Response body").size(SMALL).style(theme::muted))
        .push(body)
        .into()
}
