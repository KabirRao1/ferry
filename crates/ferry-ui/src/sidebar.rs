//! The left sidebar: Collections tree, Environments and History, or the
//! header search's results (F08-AC8, F09-AC3, AC4, AC6, AC7, AC9, AC12,
//! AC13, F14-AC1).

use ferry_app::{
    App, EnvTarget, Item, ItemKind, Message as AppMessage, Node, SidebarSection, Target,
};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, button, column, container, mouse_area, row, scrollable, space, text, tooltip,
};
use iced::{Alignment, Element as IcedElement, Fill, Font, font};

use crate::chrome::short_method;
use crate::screen::{self, SMALL, TEXT, coming};
use crate::shell::Message;
use crate::{Element, theme};

/// Width of the whole sidebar, rail included.
pub const SIDEBAR_WIDTH: f32 = 300.0;
const INDENT: f32 = 14.0;
pub const NO_COLLECTIONS: &str = "No collections yet";
pub const NEW_COLLECTION: &str = "New Collection";
pub const NEW_ENVIRONMENT: &str = "New Environment";
pub const GLOBALS: &str = "Globals";
pub const NO_RESULTS: &str = "No saved requests match";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// The sidebar: section rail plus the selected section, or search results.
#[must_use]
pub fn view(state: &App) -> Element<'_, Message> {
    let selected = state.sidebar();
    let rail = column(SidebarSection::ALL.into_iter().map(|section| {
        button(text(screen::section_label(section)).size(SMALL))
            .on_press(app(AppMessage::SidebarSection(section)))
            .width(Fill)
            .padding([8, 10])
            .style(theme::flat(section == selected))
            .into()
    }))
    .spacing(2)
    .padding(6)
    .width(108);
    let panel: Element<'_, Message> = if state.search().is_empty() {
        match selected {
            SidebarSection::Collections => collections(state),
            SidebarSection::Environments => environments(state),
            SidebarSection::History => history_placeholder(),
        }
    } else {
        search_results(state)
    };
    container(row![rail, screen::vline(), container(panel).width(Fill)])
        .width(SIDEBAR_WIDTH)
        .height(Fill)
        .style(theme::surface)
        .into()
}

/// The panel title with an optional `+` action.
fn panel_header<'a>(title: &'a str, add: Option<(Message, &'a str)>) -> Element<'a, Message> {
    let mut header = row![text(title).size(TEXT).font(bold()), space().width(Fill)]
        .align_y(Alignment::Center)
        .padding([8, 10]);
    if let Some((message, hint)) = add {
        header = header.push(tooltip(
            button(text("New").size(SMALL))
                .on_press(message)
                .padding([2, 8])
                .style(theme::secondary),
            container(text(hint).size(SMALL))
                .padding(6)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        ));
    }
    column![header, screen::hline()].into()
}

// --- collections ----------------------------------------------------------------------

fn collections(state: &App) -> Element<'_, Message> {
    let header = panel_header(
        "Collections",
        Some((app(AppMessage::NewCollection), NEW_COLLECTION)),
    );
    let items = &state.workspace().collections;
    if items.is_empty() {
        let empty = column![
            text(NO_COLLECTIONS).size(14),
            text("Save requests into collections: plain TOML files in your workspace folder that you can keep in git.")
                .size(TEXT)
                .style(theme::muted)
                .center(),
            button(text(NEW_COLLECTION).size(TEXT))
                .on_press(app(AppMessage::NewCollection))
                .padding([6, 12])
                .style(theme::secondary),
        ]
        .spacing(8)
        .align_x(Alignment::Center)
        .padding(16);
        return column![header, empty].into();
    }
    let mut rows: Vec<Element<'_, Message>> = Vec::new();
    tree(state, items, 0, &mut rows);
    column![
        header,
        scrollable(Column::with_children(rows).padding([4, 0])).height(Fill)
    ]
    .into()
}

/// Rows for `items` and, when expanded, their children.
fn tree<'a>(state: &'a App, items: &'a [Item], depth: u16, rows: &mut Vec<Element<'a, Message>>) {
    for item in items {
        rows.push(item_row(state, item, depth));
        if state.menu() == Some(item.path.as_str()) {
            rows.push(item_menu(item, depth));
        }
        if !state.is_expanded(&item.path) {
            continue;
        }
        match &item.node {
            Node::Request(file) => {
                for (index, example) in file.examples.iter().enumerate() {
                    rows.push(example_row(state, item, index, &example.name, depth + 1));
                }
            }
            _ => tree(state, &item.items, depth + 1, rows),
        }
    }
}

fn chevron<'a>(path: &str, expanded: bool) -> Element<'a, Message> {
    button(text(if expanded { "▾" } else { "▸" }).size(15))
        .on_press(app(AppMessage::ToggleExpanded(path.to_owned())))
        .padding([0, 4])
        .width(20)
        .style(theme::flat(false))
        .into()
}

fn item_row<'a>(state: &'a App, item: &'a Item, depth: u16) -> Element<'a, Message> {
    let path = item.path.as_str();
    let expandable = match &item.node {
        Node::Request(file) => !file.examples.is_empty(),
        _ => item.kind() != ItemKind::Request,
    };
    let toggle: Element<'a, Message> = if expandable {
        chevron(path, state.is_expanded(path))
    } else {
        space().width(20).into()
    };
    let badge: Element<'a, Message> = match &item.node {
        Node::Request(file) => {
            let method = file.request.method.as_str();
            text(short_method(method))
                .size(10)
                .font(bold())
                .color(theme::method_color(method))
                .width(38)
                .into()
        }
        _ => space().width(0).into(),
    };
    let name = text(item.name()).size(TEXT).wrapping(Wrapping::None).font(
        if item.kind() == ItemKind::Collection {
            bold()
        } else {
            Font::DEFAULT
        },
    );
    let mut label = row![badge, container(name).width(Fill).clip(true)]
        .spacing(4)
        .align_y(Alignment::Center);
    if let Some(problem) = item.problem() {
        label = label.push(tooltip(
            text("!").size(TEXT).font(bold()).style(theme::error),
            container(text(problem).size(SMALL))
                .padding(6)
                .max_width(360)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        ));
    }
    let active = state.tab().link.as_deref() == Some(path);
    let (_, target) = state.tree_drag();
    let hovered = target == Some(path);
    let body = mouse_area(
        container(label)
            .padding([4, 2])
            .width(Fill)
            .style(theme::tree_row(active, hovered)),
    )
    .on_press(app(AppMessage::ItemPressed(path.to_owned())))
    .on_enter(app(AppMessage::ItemEntered(path.to_owned())))
    .on_release(app(AppMessage::ItemReleased));
    row![
        space().width(f32::from(depth) * INDENT + 4.0),
        toggle,
        body,
        menu_button(path),
    ]
    .align_y(Alignment::Center)
    .padding([0, 4])
    .into()
}

fn menu_button<'a>(path: &str) -> Element<'a, Message> {
    button(text("…").size(TEXT))
        .on_press(app(AppMessage::Menu(Some(path.to_owned()))))
        .padding([0, 6])
        .style(theme::flat(false))
        .into()
}

fn menu_entry<'a>(label: &'a str, message: AppMessage) -> Element<'a, Message> {
    button(text(label).size(SMALL))
        .on_press(app(message))
        .padding([3, 8])
        .width(Fill)
        .style(theme::flat(false))
        .into()
}

/// The `…` actions of an item, shown under its row.
fn item_menu(item: &Item, depth: u16) -> Element<'_, Message> {
    let path = item.path.clone();
    let target = || Target::Item(path.clone());
    let mut entries: Vec<Element<'_, Message>> = Vec::new();
    let broken = matches!(item.node, Node::Broken { .. });
    if item.kind() != ItemKind::Request && !broken {
        entries.push(menu_entry(
            "Add Request",
            AppMessage::NewRequest(path.clone()),
        ));
        entries.push(menu_entry(
            "Add Folder",
            AppMessage::NewFolder(path.clone()),
        ));
        entries.push(menu_entry("Settings", AppMessage::OpenItem(path.clone())));
    }
    if !broken {
        entries.push(menu_entry("Rename", AppMessage::Rename(target())));
        entries.push(menu_entry("Duplicate", AppMessage::Duplicate(target())));
    }
    entries.push(menu_entry("Delete", AppMessage::Delete(target())));
    entries.push(menu_entry("Close menu", AppMessage::Menu(None)));
    menu_box(entries, depth)
}

fn menu_box(entries: Vec<Element<'_, Message>>, depth: u16) -> Element<'_, Message> {
    row![
        space().width(f32::from(depth) * INDENT + 28.0),
        container(Column::with_children(entries).spacing(1))
            .padding(4)
            .width(Fill)
            .style(theme::dialog),
    ]
    .padding([2, 10])
    .into()
}

fn example_row<'a>(
    state: &'a App,
    request: &'a Item,
    index: usize,
    name: &'a str,
    depth: u16,
) -> Element<'a, Message> {
    let path = request.path.clone();
    let menu_key = format!("{path}#{index}");
    let label = row![
        text("e.g.").size(10).style(theme::faint).width(38),
        container(text(name).size(TEXT).wrapping(Wrapping::None))
            .width(Fill)
            .clip(true),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let open = button(label)
        .on_press(app(AppMessage::OpenExample(path.clone(), index)))
        .padding([4, 2])
        .width(Fill)
        .style(theme::flat(false));
    let mut column = column![
        row![
            space().width(f32::from(depth) * INDENT + 24.0),
            open,
            button(text("…").size(TEXT))
                .on_press(app(AppMessage::Menu(Some(menu_key.clone()))))
                .padding([0, 6])
                .style(theme::flat(false)),
        ]
        .align_y(Alignment::Center)
        .padding([0, 4])
    ];
    if state.menu() == Some(menu_key.as_str()) {
        let target = || Target::Example(path.clone(), index);
        column = column.push(menu_box(
            vec![
                menu_entry("Rename", AppMessage::Rename(target())),
                menu_entry("Duplicate", AppMessage::Duplicate(target())),
                menu_entry("Delete", AppMessage::Delete(target())),
                menu_entry("Close menu", AppMessage::Menu(None)),
            ],
            depth,
        ));
    }
    column.into()
}

// --- environments ------------------------------------------------------------------------

fn environments(state: &App) -> Element<'_, Message> {
    let header = panel_header(
        "Environments",
        Some((app(AppMessage::NewEnvironment), NEW_ENVIRONMENT)),
    );
    let mut rows: Vec<Element<'_, Message>> = vec![env_row(
        GLOBALS,
        None,
        AppMessage::OpenEnvironment(EnvTarget::Globals),
        false,
        None,
    )];
    for entry in &state.workspace().environments {
        let path = entry.path.as_str();
        let selected = state.environment() == Some(path);
        rows.push(env_row(
            entry.name(),
            entry.problem(),
            AppMessage::OpenEnvironment(EnvTarget::Environment(path.to_owned())),
            selected,
            Some(path),
        ));
        if state.menu() == Some(path) {
            let target = || Target::Environment(path.to_owned());
            let mut entries = Vec::new();
            if entry.file.is_ok() {
                entries.push(menu_entry("Rename", AppMessage::Rename(target())));
                entries.push(menu_entry("Duplicate", AppMessage::Duplicate(target())));
            }
            entries.push(menu_entry("Delete", AppMessage::Delete(target())));
            entries.push(menu_entry("Close menu", AppMessage::Menu(None)));
            rows.push(menu_box(entries, 0));
        }
    }
    let note = text(
        "Pick the active environment in the tab bar. Its variables replace {{name}} in requests.",
    )
    .size(SMALL)
    .style(theme::faint);
    column![
        header,
        scrollable(Column::with_children(rows).padding([4, 0])).height(Fill),
        container(note).padding(10),
    ]
    .into()
}

fn env_row<'a>(
    name: &'a str,
    problem: Option<String>,
    open: AppMessage,
    selected: bool,
    menu: Option<&'a str>,
) -> Element<'a, Message> {
    let mut label = row![
        text(if selected { "●" } else { "" })
            .size(10)
            .style(theme::accent)
            .width(14),
        container(text(name).size(TEXT).wrapping(Wrapping::None))
            .width(Fill)
            .clip(true),
    ]
    .align_y(Alignment::Center);
    let broken = problem.is_some();
    if let Some(problem) = problem {
        label = label.push(tooltip(
            text("!").size(TEXT).font(bold()).style(theme::error),
            container(text(problem).size(SMALL))
                .padding(6)
                .max_width(360)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        ));
    }
    let mut open_button = button(label)
        .padding([5, 6])
        .width(Fill)
        .style(theme::flat(false));
    if !broken {
        open_button = open_button.on_press(app(open));
    }
    let mut line = row![open_button].align_y(Alignment::Center).padding([0, 6]);
    if let Some(path) = menu {
        line = line.push(menu_button(path));
    }
    line.into()
}

// --- history and search -------------------------------------------------------------------

fn history_placeholder<'a>() -> Element<'a, Message> {
    column![
        panel_header("History", None),
        container(coming(
            "No history yet",
            "Every request you send will be listed here, searchable and one click to open again.",
        ))
        .padding(16),
    ]
    .into()
}

fn search_results(state: &App) -> Element<'_, Message> {
    let hits = state.search_hits();
    let header = panel_header("Search results", None);
    if hits.is_empty() {
        return column![
            header,
            container(text(NO_RESULTS).size(TEXT).style(theme::muted)).padding(16),
        ]
        .into();
    }
    let rows = hits.into_iter().map(|hit| -> IcedElement<'_, Message> {
        let method = hit.method.clone();
        let label = column![
            row![
                text(short_method(&method).to_owned())
                    .size(10)
                    .font(bold())
                    .color(theme::method_color(&method))
                    .width(38),
                text(hit.name).size(TEXT).wrapping(Wrapping::None),
            ]
            .align_y(Alignment::Center),
            text(hit.location.join(" / "))
                .size(SMALL)
                .style(theme::faint)
                .wrapping(Wrapping::None),
        ]
        .spacing(2);
        button(label)
            .on_press(app(AppMessage::OpenItem(hit.path)))
            .padding([5, 10])
            .width(Fill)
            .style(theme::flat(false))
            .into()
    });
    column![
        header,
        scrollable(column(rows).padding([4, 0])).height(Fill)
    ]
    .into()
}
