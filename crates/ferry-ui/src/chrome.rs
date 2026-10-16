//! Window chrome: header, sidebar, tab strip and footer (F07, F14).

use ferry_app::{App, Message as AppMessage, SidebarSection, Tab};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, space, text, text_input, tooltip,
};
use iced::{Alignment, Fill, Font, font};

use crate::screen::{self, SMALL, TEXT, coming};
use crate::shell::Message;
use crate::{Element, theme};

/// Height of the workbench tab strip, in logical pixels.
pub const TAB_STRIP_HEIGHT: f32 = 36.0;
const TAB_WIDTH: f32 = 200.0;
const SIDEBAR_WIDTH: f32 = 280.0;

/// Placeholder of the (not yet available) search field.
pub const SEARCH_PLACEHOLDER: &str = "Search Ferry";
/// Title of tabs without a URL.
pub const UNTITLED: &str = "Untitled Request";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// Top bar: name, search and settings.
#[must_use]
pub fn header<'a>() -> Element<'a, Message> {
    let search = tooltip(
        text_input(SEARCH_PLACEHOLDER, "")
            .size(TEXT)
            .padding([5, 10])
            .width(360)
            .style(theme::input),
        container(text("Search arrives with collections in Phase 3").size(SMALL))
            .padding(6)
            .style(theme::dialog),
        tooltip::Position::Bottom,
    );
    let settings = tooltip(
        button(text("Settings").size(TEXT))
            .padding([5, 10])
            .style(theme::flat(false)),
        container(text("Settings arrive in Phase 7").size(SMALL))
            .padding(6)
            .style(theme::dialog),
        tooltip::Position::Left,
    );
    container(
        row![
            text("Ferry").size(15).font(bold()).style(theme::accent),
            space().width(Fill),
            search,
            space().width(Fill),
            settings,
        ]
        .align_y(Alignment::Center)
        .padding([0, 12]),
    )
    .height(44)
    .align_y(Alignment::Center)
    .style(theme::surface)
    .into()
}

/// Left sidebar: section rail plus the selected section (F14-AC1, AC5).
#[must_use]
pub fn sidebar<'a>(selected: SidebarSection) -> Element<'a, Message> {
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
    let (title, detail) = match selected {
        SidebarSection::Collections => (
            "No collections yet",
            "Collections arrive in Phase 3: save requests into folders, stored as plain files you can keep in git.",
        ),
        SidebarSection::Environments => (
            "No environments yet",
            "Environments arrive in Phase 3: switch variables like {{base_url}} between dev, staging and production.",
        ),
        SidebarSection::History => (
            "No history yet",
            "History arrives in Phase 3: every request you send, searchable and one click to replay.",
        ),
    };
    let panel = column![
        container(
            text(screen::section_label(selected))
                .size(TEXT)
                .font(bold())
        )
        .padding([10, 12]),
        screen::hline(),
        container(coming(title, detail)).padding(16),
    ]
    .width(Fill);
    container(row![rail, screen::vline(), panel])
        .width(SIDEBAR_WIDTH)
        .height(Fill)
        .style(theme::surface)
        .into()
}

/// Workbench tabs, + and the environment selector (F07, F14-AC1).
#[must_use]
pub fn tab_strip(app_state: &App) -> Element<'_, Message> {
    let active = app_state.tab().id;
    let tabs = row(app_state
        .tabs()
        .iter()
        .map(|tab| workbench_tab(tab, tab.id == active)))
    .spacing(1);
    let new_tab = button(text("+").size(16))
        .on_press(app(AppMessage::NewTab))
        .padding([4, 12])
        .style(theme::flat(false));
    let environment = tooltip(
        button(text("No environment").size(SMALL))
            .padding([5, 10])
            .style(theme::secondary),
        container(text("Environments arrive in Phase 3").size(SMALL))
            .padding(6)
            .style(theme::dialog),
        tooltip::Position::Bottom,
    );
    container(
        row![
            scrollable(row![tabs, new_tab].align_y(Alignment::Center))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(2).scroller_width(2),
                ))
                .width(Fill),
            container(environment).padding([0, 10]),
        ]
        .align_y(Alignment::Center),
    )
    .height(TAB_STRIP_HEIGHT)
    .style(theme::app_background)
    .into()
}

fn workbench_tab(tab: &Tab, active: bool) -> Element<'_, Message> {
    let method = tab.draft.method.as_str();
    let close: Element<'_, Message> = if tab.is_dirty() && !active {
        text("●").size(10).style(theme::accent).into()
    } else {
        button(text("×").size(14))
            .on_press(app(AppMessage::CloseTab(tab.id)))
            .padding([0, 5])
            .style(theme::flat(false))
            .into()
    };
    let dirty: Element<'_, Message> = if tab.is_dirty() && active {
        text("●").size(10).style(theme::accent).into()
    } else {
        space().width(0).into()
    };
    let label = row![
        text(short_method(method))
            .size(10)
            .font(bold())
            .color(theme::method_color(method)),
        container(text(tab.title()).size(SMALL).wrapping(Wrapping::None),)
            .width(Fill)
            .clip(true),
        dirty,
        close,
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let accent: Element<'_, Message> = if active {
        container(space().height(2).width(Fill))
            .style(theme::accent_line)
            .into()
    } else {
        space().height(2).into()
    };
    let body = container(column![
        accent,
        container(label)
            .padding([0, 10])
            .height(Fill)
            .align_y(Alignment::Center)
    ])
    .width(TAB_WIDTH)
    .height(TAB_STRIP_HEIGHT)
    .style(theme::workbench_tab(active));
    mouse_area(body)
        .on_press(app(AppMessage::TabPressed(tab.id)))
        .on_release(app(AppMessage::TabReleased))
        .on_enter(app(AppMessage::TabEntered(tab.id)))
        .on_middle_press(app(AppMessage::CloseTab(tab.id)))
        .into()
}

/// Method labels as tabs show them: long ones are shortened like Postman's.
#[must_use]
pub fn short_method(method: &str) -> &str {
    match method {
        "DELETE" => "DEL",
        "OPTIONS" => "OPT",
        "PATCH" => "PATCH",
        m if m.len() > 6 => &m[..m.floor_char_boundary(6)],
        m => m,
    }
}

/// Bottom bar: sidebar toggle, Console, Cookies, Runner, version.
#[must_use]
pub fn footer<'a>(sidebar_open: bool, save_error: Option<&'a str>) -> Element<'a, Message> {
    let toggle = button(
        text(if sidebar_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        })
        .size(SMALL),
    )
    .on_press(app(AppMessage::ToggleSidebar))
    .padding([3, 8])
    .style(theme::flat(false));
    let later = |label: &'a str, when: &'a str| -> Element<'a, Message> {
        tooltip(
            button(text(label).size(SMALL))
                .padding([3, 8])
                .style(theme::flat(false)),
            container(text(when).size(SMALL))
                .padding(6)
                .style(theme::dialog),
            tooltip::Position::Top,
        )
        .into()
    };
    let status: Element<'a, Message> = match save_error {
        Some(error) => text(error).size(SMALL).style(theme::error).into(),
        None => space().width(0).into(),
    };
    container(
        row![
            toggle,
            later("Console", "The console arrives in Phase 3"),
            space().width(Fill),
            status,
            space().width(12),
            later("Cookies", "The cookie manager arrives in Phase 3"),
            later("Runner", "The Collection Runner arrives in Phase 4"),
            text(concat!("Ferry ", env!("CARGO_PKG_VERSION")))
                .size(SMALL)
                .style(theme::faint),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([0, 8]),
    )
    .height(28)
    .align_y(Alignment::Center)
    .style(theme::surface)
    .into()
}
