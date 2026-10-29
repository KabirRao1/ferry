//! Window chrome: header, tab strip and footer (F07, F08-AC9, F09-AC13,
//! F09-AC14, F14).

use std::fmt;

use ferry_app::{App, Doc, ItemKind, Message as AppMessage, Tab};
use iced::widget::text::Wrapping;
use iced::widget::{
    button, column, container, mouse_area, pick_list, row, scrollable, space, text, text_input,
    tooltip,
};
use iced::{Alignment, Fill, Font, font};

use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::{Element, theme};

/// Height of the workbench tab strip, in logical pixels.
pub const TAB_STRIP_HEIGHT: f32 = 36.0;
const TAB_WIDTH: f32 = 200.0;

/// Placeholder of the header search field (F09-AC13).
pub const SEARCH_PLACEHOLDER: &str = "Search saved requests";
/// Id of the search field, for focus.
pub const SEARCH_INPUT: &str = "search";
/// Title of tabs without a URL.
pub const UNTITLED: &str = "Untitled Request";
/// The environment selector's empty choice (F08-AC9).
pub const NO_ENVIRONMENT: &str = "No environment";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..Font::DEFAULT
    }
}

/// Top bar: name and workspace, search, settings.
#[must_use]
pub fn header(state: &App) -> Element<'_, Message> {
    let folder = state
        .workspace_root()
        .and_then(|p| p.file_name())
        .map_or_else(
            || "Workspace".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
    let workspace = tooltip(
        button(text(format!("{folder} ▾")).size(TEXT))
            .on_press(app(AppMessage::OpenWorkspace))
            .padding([5, 10])
            .style(theme::flat(false)),
        container(text("Open Workspace… (a folder of collections and environments)").size(SMALL))
            .padding(6)
            .style(theme::dialog),
        tooltip::Position::Bottom,
    );
    // Import (F11-AC2): here rather than in the narrow sidebar panels.
    let import = button(text(crate::sidebar::IMPORT).size(TEXT))
        .on_press(app(AppMessage::OpenImport))
        .padding([4, 10])
        .style(theme::secondary);
    let mut search = row![
        text_input(SEARCH_PLACEHOLDER, state.search())
            .id(iced::widget::Id::new(SEARCH_INPUT))
            .on_input(|s| app(AppMessage::Search(s)))
            .size(TEXT)
            .padding([5, 10])
            .width(360)
            .style(theme::input),
    ]
    .align_y(Alignment::Center);
    if !state.search().is_empty() {
        search = search.push(
            button(text("×").size(14))
                .on_press(app(AppMessage::Search(String::new())))
                .padding([0, 8])
                .style(theme::flat(false)),
        );
    }
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
            workspace,
            import,
            space().width(Fill),
            search,
            space().width(Fill),
            settings,
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([0, 12]),
    )
    .height(44)
    .align_y(Alignment::Center)
    .style(theme::surface)
    .into()
}

/// One entry of the environment selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvChoice {
    pub path: Option<String>,
    pub name: String,
}

impl fmt::Display for EnvChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// The selector's choices: `No environment`, then every readable one.
#[must_use]
pub fn env_choices(state: &App) -> Vec<EnvChoice> {
    let mut choices = vec![EnvChoice {
        path: None,
        name: NO_ENVIRONMENT.to_owned(),
    }];
    choices.extend(
        state
            .workspace()
            .environments
            .iter()
            .filter(|e| e.file.is_ok())
            .map(|e| EnvChoice {
                path: Some(e.path.clone()),
                name: e.name().to_owned(),
            }),
    );
    choices
}

/// Workbench tabs, + and the environment selector (F07, F08-AC9, F14-AC1).
#[must_use]
pub fn tab_strip(state: &App) -> Element<'_, Message> {
    let active = state.tab().id;
    let tabs = row(state
        .tabs()
        .iter()
        .map(|tab| workbench_tab(tab, tab.id == active)))
    .spacing(1);
    let new_tab = button(text("+").size(16))
        .on_press(app(AppMessage::NewTab))
        .padding([4, 12])
        .style(theme::flat(false));
    let choices = env_choices(state);
    let selected = choices
        .iter()
        .find(|c| c.path.as_deref() == state.environment())
        .cloned();
    let environment = pick_list(choices, selected, |c: EnvChoice| {
        app(AppMessage::SelectEnvironment(c.path))
    })
    .text_size(SMALL)
    .padding([4, 10])
    .width(190);
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

/// The small label before a tab's title: its method, or what it holds.
#[must_use]
pub fn tab_badge(tab: &Tab) -> (String, iced::Color) {
    match &tab.doc {
        Doc::Request => {
            let method = tab.draft.method.as_str();
            (short_method(method).to_owned(), theme::method_color(method))
        }
        Doc::Environment(_) => ("ENV".to_owned(), theme::INFO),
        Doc::Container(c) if c.kind == ItemKind::Collection => ("COL".to_owned(), theme::ACCENT),
        Doc::Container(_) => ("DIR".to_owned(), theme::ACCENT),
        Doc::Example(_) => ("EX".to_owned(), theme::MUTED),
        Doc::Runner(_) => ("RUN".to_owned(), theme::ACCENT),
    }
}

fn workbench_tab(tab: &Tab, active: bool) -> Element<'_, Message> {
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
    let (badge, color) = tab_badge(tab);
    let label = row![
        text(badge).size(10).font(bold()).color(color),
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

/// Bottom bar: sidebar toggle, Console, errors, Cookies, Runner, version.
#[must_use]
pub fn footer<'a>(state: &'a App, save_error: Option<&'a str>) -> Element<'a, Message> {
    let toggle = button(
        text(if state.sidebar_open() {
            "Hide sidebar"
        } else {
            "Show sidebar"
        })
        .size(SMALL),
    )
    .on_press(app(AppMessage::ToggleSidebar))
    .padding([3, 8])
    .style(theme::flat(false));
    let mut status = row![].spacing(6).align_y(Alignment::Center);
    if let Some(notice) = state.notice() {
        status = status.push(text(notice).size(SMALL).style(theme::muted));
        if state.status().is_none() {
            status = status.push(
                button(text("×").size(12))
                    .on_press(app(AppMessage::DismissStatus))
                    .padding([0, 6])
                    .style(theme::flat(false)),
            );
        }
    }
    if let Some(error) = state.status() {
        status = status
            .push(text(error).size(SMALL).style(theme::error))
            .push(
                button(text("×").size(12))
                    .on_press(app(AppMessage::DismissStatus))
                    .padding([0, 6])
                    .style(theme::flat(false)),
            );
    }
    if let Some(error) = save_error {
        status = status.push(text(error).size(SMALL).style(theme::error));
    }
    let console = button(text("Console").size(SMALL))
        .on_press(app(AppMessage::ToggleConsole))
        .padding([3, 8])
        .style(theme::flat(state.console().open));
    let cookies = button(text("Cookies").size(SMALL))
        .on_press(app(AppMessage::OpenCookies))
        .padding([3, 8])
        .style(theme::flat(false));
    container(
        row![
            toggle,
            console,
            space().width(Fill),
            status,
            space().width(12),
            cookies,
            button(text("Runner").size(SMALL))
                .on_press_maybe(
                    state
                        .runner_target()
                        .is_some()
                        .then(|| app(AppMessage::OpenRunnerHere)),
                )
                .padding([3, 8])
                .style(theme::flat(false)),
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
