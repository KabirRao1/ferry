//! The console panel under the workbench: every request as sent and how it
//! ended (F18-AC1..AC3).

use ferry_app::console::{ConsoleEntry, ConsoleState};
use ferry_app::{App, Header, Message as AppMessage, format};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{
    Column, button, checkbox, column, container, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Fill, Font};

use crate::screen::{self, SMALL, TEXT};
use crate::shell::Message;
use crate::{Element, theme};

/// Height of the open panel, in logical pixels.
pub const CONSOLE_HEIGHT: f32 = 240.0;
pub const EMPTY: &str = "Requests you send are logged here";
pub const NO_MATCH: &str = "No logged requests match";
pub const ERRORS_ONLY: &str = "Errors only";

fn app(message: AppMessage) -> Message {
    Message::App(message)
}

/// The panel, when open.
#[must_use]
pub fn view(state: &App) -> Element<'_, Message> {
    let console = state.console();
    let toolbar = row![
        text("Console").size(TEXT).font(Font {
            weight: iced::font::Weight::Bold,
            ..Font::DEFAULT
        }),
        text_input("Filter by URL or method", &console.filter)
            .on_input(|s| app(AppMessage::ConsoleFilter(s)))
            .size(SMALL)
            .padding([3, 8])
            .width(260)
            .style(theme::input),
        checkbox(console.errors_only)
            .label(ERRORS_ONLY)
            .on_toggle(|on| app(AppMessage::ConsoleErrorsOnly(on)))
            .size(14)
            .text_size(SMALL),
        space().width(Fill),
        button(text("Clear").size(SMALL))
            .on_press(app(AppMessage::ConsoleClear))
            .padding([3, 8])
            .style(theme::secondary),
        button(text("×").size(TEXT))
            .on_press(app(AppMessage::ToggleConsole))
            .padding([1, 8])
            .style(theme::flat(false)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding([4, 12]);
    let visible = console.visible();
    let content: Element<'_, Message> = if visible.is_empty() {
        let hint = if console.entries.is_empty() {
            EMPTY
        } else {
            NO_MATCH
        };
        container(text(hint).size(TEXT).style(theme::muted))
            .padding([8, 16])
            .into()
    } else {
        // Newest last, as a log reads.
        let rows = visible
            .into_iter()
            .map(|entry| entry_view(entry, console.expanded.contains(&entry.id)));
        scrollable(Column::with_children(rows).padding([0, 8]))
            .anchor_bottom()
            .height(Fill)
            .into()
    };
    container(column![toolbar, screen::hline(), content])
        .height(CONSOLE_HEIGHT)
        .width(Fill)
        .style(theme::surface)
        .into()
}

fn entry_view(entry: &ConsoleEntry, expanded: bool) -> Element<'_, Message> {
    let summary = text(entry.summary())
        .size(SMALL)
        .font(Font::MONOSPACE)
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::None);
    let summary = if entry.is_error() {
        summary.style(theme::error)
    } else {
        summary
    };
    let line = button(
        row![
            text(if expanded { "▾" } else { "▸" })
                .size(SMALL)
                .style(theme::faint)
                .width(14),
            container(summary).width(Fill).clip(true),
        ]
        .align_y(Alignment::Center),
    )
    .on_press(app(AppMessage::ConsoleExpand(entry.id)))
    .padding([3, 6])
    .width(Fill)
    .style(theme::flat(false));
    if !expanded {
        return line.into();
    }
    let mut details = column![section("Request Headers"), headers(&entry.request_headers),]
        .spacing(4)
        .padding([4, 26]);
    if !entry.request_body.is_empty() {
        details = details
            .push(section("Request Body"))
            .push(mono(&entry.request_body));
    }
    match &entry.state {
        ConsoleState::Pending => {
            details = details.push(text("Sending…").size(SMALL).style(theme::muted));
        }
        ConsoleState::Done {
            status,
            reason,
            elapsed,
            size,
            headers: response_headers,
            body,
        } => {
            details = details
                .push(section("Response"))
                .push(
                    text(format!(
                        "{status} {reason} · {} · {}",
                        format::duration(*elapsed),
                        format::size(*size)
                    ))
                    .size(SMALL)
                    .color(theme::status_color(*status)),
                )
                .push(section("Response Headers"))
                .push(headers(response_headers));
            if !body.is_empty() {
                details = details.push(section("Response Body")).push(mono(body));
            }
        }
        ConsoleState::Failed(error) => {
            details = details.push(text(error.as_str()).size(SMALL).style(theme::error));
        }
        ConsoleState::Cancelled => {
            details = details.push(
                text(crate::response::CANCELLED)
                    .size(SMALL)
                    .style(theme::muted),
            );
        }
    }
    column![line, details].into()
}

fn section(label: &str) -> Element<'_, Message> {
    text(label).size(SMALL).style(theme::muted).into()
}

fn headers(headers: &[Header]) -> Element<'_, Message> {
    if headers.is_empty() {
        return text("None").size(SMALL).style(theme::faint).into();
    }
    column(headers.iter().map(|h| {
        row![mono(&h.name).width(220), mono(&h.value).style(theme::muted)]
            .spacing(12)
            .into()
    }))
    .into()
}

fn mono(content: &str) -> iced::widget::Text<'_> {
    text(content)
        .size(SMALL)
        .font(Font::MONOSPACE)
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::WordOrGlyph)
}
