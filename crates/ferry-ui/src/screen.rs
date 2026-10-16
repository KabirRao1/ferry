//! The request/response screen: a pure function of [`App`] state.

use ferry_app::{App, Message, Method, Phase, Received, ResponseTab, format};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{
    button, center, column, container, pick_list, row, rule, scrollable, space, text, text_input,
};
use iced::{Alignment, Color, Fill, Font, Theme, color};

use crate::{Element, body_view};

/// Id of the URL field, for focus and tests.
pub const URL_INPUT: &str = "url";
/// Placeholder shown in the empty URL field.
pub const URL_PLACEHOLDER: &str = "https://api.example.com/users";
/// Hint shown before the first send.
pub const IDLE_HINT: &str = if cfg!(target_os = "macos") {
    "Enter a URL and press Send (⌘+Enter)"
} else {
    "Enter a URL and press Send (Ctrl+Enter)"
};
pub const SENDING: &str = "Sending…";
pub const CANCELLED: &str = "Request cancelled";
pub const EMPTY_BODY: &str = "Empty body";

/// The whole screen: request bar above the response panel.
#[must_use]
pub fn view(app: &App) -> Element<'_, Message> {
    column![
        request_bar(app),
        rule::horizontal(1),
        container(response_panel(app)).padding(12).height(Fill),
    ]
    .into()
}

fn request_bar(app: &App) -> Element<'_, Message> {
    let method = pick_list(
        Method::STANDARD,
        Some(app.method().clone()),
        Message::MethodSelected,
    )
    .width(110);
    let url = text_input(URL_PLACEHOLDER, app.url())
        .id(iced::widget::Id::new(URL_INPUT))
        .on_input(Message::UrlChanged)
        .on_submit(Message::SendPressed)
        .padding(8)
        .width(Fill);
    let action = if app.is_sending() {
        button(text("Cancel").align_x(Alignment::Center))
            .on_press(Message::CancelPressed)
            .style(button::secondary)
    } else {
        button(text("Send").align_x(Alignment::Center))
            .on_press(Message::SendPressed)
            .style(button::primary)
    };
    row![method, url, action.padding([8, 18])]
        .spacing(8)
        .padding(12)
        .align_y(Alignment::Center)
        .into()
}

fn response_panel(app: &App) -> Element<'_, Message> {
    match app.phase() {
        Phase::Idle => center(text(IDLE_HINT).style(text::secondary)).into(),
        Phase::Sending { .. } => center(text(SENDING)).into(),
        Phase::Cancelled => center(text(CANCELLED).style(text::secondary)).into(),
        Phase::Failed(message) => center(text(message.as_str()).color(ERROR)).into(),
        Phase::Received(received) => response(received, app.tab()),
    }
}

fn response(received: &Received, tab: ResponseTab) -> Element<'_, Message> {
    let r = &received.response;
    let summary = row![
        text(r.status_line()).color(status_color(r.status)),
        text("·").style(text::secondary),
        text(format::duration(r.elapsed)),
        text("·").style(text::secondary),
        text(format::size(r.body.len() as u64)),
        space().width(Fill),
        tab_button("Body".to_owned(), ResponseTab::Body, tab),
        tab_button(
            format!("Headers ({})", r.headers.len()),
            ResponseTab::Headers,
            tab
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let content: Element<'_, Message> = match tab {
        ResponseTab::Body if r.body.is_empty() => {
            center(text(EMPTY_BODY).style(text::secondary)).into()
        }
        ResponseTab::Body => body_view::view(&received.body).map(Message::Body),
        ResponseTab::Headers => headers(received),
    };
    column![summary, content].spacing(12).into()
}

fn tab_button(label: String, tab: ResponseTab, current: ResponseTab) -> Element<'static, Message> {
    let style = if tab == current {
        button::primary
    } else {
        button::text
    };
    button(text(label))
        .on_press(Message::TabSelected(tab))
        .style(style)
        .into()
}

fn headers(received: &Received) -> Element<'_, Message> {
    let rows = received.response.headers.iter().map(|h| {
        row![
            mono(h.name.as_str()).style(text::primary),
            mono(h.value.as_str()),
        ]
        .spacing(12)
        .into()
    });
    scrollable(column(rows).spacing(4).width(Fill)).into()
}

fn mono<'a>(content: &'a str) -> iced::widget::Text<'a> {
    text(content)
        .font(Font::MONOSPACE)
        .size(13)
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::WordOrGlyph)
}

/// Error text. Bright enough for WCAG AA contrast on the dark background
/// (iced's own dark-theme success/danger colors are too dim for text).
const ERROR: Color = color!(0xf8_71_71);

/// Status colors by class: 2xx green, 3xx blue, 4xx amber, 5xx red.
#[must_use]
pub fn status_color(status: u16) -> Color {
    match status {
        200..=299 => color!(0x4a_de_80),
        300..=399 => color!(0x60_a5_fa),
        400..=499 => color!(0xfb_bf_24),
        500..=599 => ERROR,
        _ => Theme::Dark.palette().text,
    }
}
