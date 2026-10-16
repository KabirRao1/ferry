//! The response pane: Body, Cookies, Headers and Test Results tabs, with the
//! status, time (and its breakdown) and size on the right (F06, F14-AC3).

use ferry_app::{BodyFormat, Message as AppMessage, Phase, Received, ResponseTab, Tab, format};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{
    button, center, column, container, row, rule, scrollable, space, text, tooltip,
};
use iced::{Alignment, Fill, Font};

use crate::request::underline;
use crate::screen::{SMALL, TEXT, coming};
use crate::shell::Message;
use crate::{Element, body_view, theme};

/// Hint shown before the first send.
pub const IDLE_HINT: &str = if cfg!(target_os = "macos") {
    "Enter the URL and click Send to get a response (⌘+Enter)"
} else {
    "Enter the URL and click Send to get a response (Ctrl+Enter)"
};
pub const SENDING: &str = "Sending request…";
pub const CANCELLED: &str = "Request cancelled";
pub const EMPTY_BODY: &str = "Empty body";
pub const FAILED_TITLE: &str = "Could not send request";

fn app(m: AppMessage) -> Message {
    Message::App(m)
}

/// The response pane for `tab`.
#[must_use]
pub fn view(tab: &Tab) -> Element<'_, Message> {
    let header_label = text("Response").size(TEXT).style(theme::muted);
    let content: Element<'_, Message> = match &tab.phase {
        Phase::Idle => column![
            container(header_label).padding([8, 16]),
            center(text(IDLE_HINT).size(TEXT).style(theme::muted)),
        ]
        .into(),
        Phase::Sending { .. } => column![
            container(header_label).padding([8, 16]),
            center(
                column![
                    text(SENDING).size(TEXT),
                    button(text("Cancel").size(TEXT))
                        .on_press(app(AppMessage::CancelPressed))
                        .padding([6, 14])
                        .style(theme::secondary),
                ]
                .spacing(10)
                .align_x(Alignment::Center)
            ),
        ]
        .into(),
        Phase::Cancelled => column![
            container(header_label).padding([8, 16]),
            center(text(CANCELLED).size(TEXT).style(theme::muted)),
        ]
        .into(),
        Phase::Failed(message) => column![
            container(header_label).padding([8, 16]),
            center(
                column![
                    text(FAILED_TITLE).size(14),
                    text(message.as_str())
                        .size(TEXT)
                        .style(theme::error)
                        .center(),
                ]
                .spacing(6)
                .max_width(560)
                .align_x(Alignment::Center)
            ),
        ]
        .into(),
        Phase::Received(received) => self::received(received, tab.response_tab),
    };
    container(content)
        .width(Fill)
        .height(Fill)
        .style(theme::app_background)
        .into()
}

/// Response tab labels (F14-AC3).
#[must_use]
pub fn tab_label(received: &Received, which: ResponseTab) -> String {
    match which {
        ResponseTab::Body => "Body".into(),
        ResponseTab::Cookies => "Cookies".into(),
        ResponseTab::Headers => format!("Headers ({})", received.response.headers.len()),
        ResponseTab::TestResults => "Test Results".into(),
    }
}

fn received(received: &Received, selected: ResponseTab) -> Element<'_, Message> {
    let r = &received.response;
    let tabs = row(ResponseTab::ALL.into_iter().map(|which| {
        column![
            button(text(tab_label(received, which)).size(TEXT))
                .on_press(app(AppMessage::ResponseTabSelected(which)))
                .padding([4, 10])
                .style(theme::sub_tab(which == selected)),
            underline(which == selected),
        ]
        .width(iced::Length::Shrink)
        .into()
    }))
    .spacing(2);
    let time = tooltip(
        text(format::duration(r.elapsed))
            .size(TEXT)
            .style(theme::accent),
        container(timing(r)).padding(10).style(theme::dialog),
        tooltip::Position::Bottom,
    );
    let summary = row![
        text(r.status_line())
            .size(TEXT)
            .color(theme::status_color(r.status)),
        text("·").style(theme::faint),
        time,
        text("·").style(theme::faint),
        text(format::size(r.body.len() as u64))
            .size(TEXT)
            .style(theme::accent),
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let header = row![tabs, space().width(Fill), summary]
        .align_y(Alignment::Center)
        .padding([4, 16]);
    let content: Element<'_, Message> = match selected {
        ResponseTab::Body => body(received),
        ResponseTab::Headers => headers(received),
        ResponseTab::Cookies => coming(
            "Cookies arrive in Phase 3",
            "A cookie jar that keeps cookies between requests, with the cookies each response set.",
        ),
        ResponseTab::TestResults => coming(
            "Test results arrive in Phase 4",
            "Write tests in post-response scripts with pm.test and see which pass here.",
        ),
    };
    column![header, container(content).padding([4, 16]).height(Fill)].into()
}

/// The time breakdown shown on hover, as `(label, value)` lines (F06-AC2).
/// A reused connection has no DNS or Connect line.
#[must_use]
pub fn timing_lines(r: &ferry_app::Response) -> Vec<(&'static str, String)> {
    let t = r.timing;
    let mut lines = Vec::new();
    if t.dns.is_none() && t.connect.is_none() {
        lines.push(("Connection reused", String::new()));
    }
    if let Some(dns) = t.dns {
        lines.push(("DNS Lookup", format::duration(dns)));
    }
    if let Some(connect) = t.connect {
        lines.push(("Connect (TCP + TLS)", format::duration(connect)));
    }
    lines.push(("Waiting (TTFB)", format::duration(t.waiting)));
    lines.push(("Download", format::duration(t.download)));
    lines.push(("Total", format::duration(r.elapsed)));
    lines
}

fn timing(r: &ferry_app::Response) -> Element<'_, Message> {
    let mut rows = column![text("Response time").size(SMALL).style(theme::muted)].spacing(4);
    for (label, value) in timing_lines(r) {
        if label == "Total" {
            rows = rows.push(rule::horizontal(1));
        }
        rows = rows.push(row![
            text(label).size(SMALL).width(150),
            text(value).size(SMALL),
        ]);
    }
    rows.width(240).into()
}

fn body(received: &Received) -> Element<'_, Message> {
    let formats = row(received.formats().into_iter().map(|f| {
        button(text(f.to_string()).size(SMALL))
            .on_press(app(AppMessage::BodyFormatSelected(f)))
            .padding([2, 10])
            .style(theme::flat(f == received.format))
            .into()
    }))
    .spacing(2);
    let content: Element<'_, Message> =
        if received.response.body.is_empty() && received.format != BodyFormat::Hex {
            center(text(EMPTY_BODY).size(TEXT).style(theme::muted)).into()
        } else {
            body_view::view(received.view()).map(|m| app(AppMessage::Body(m)))
        };
    column![formats, content].spacing(6).into()
}

fn headers(received: &Received) -> Element<'_, Message> {
    let rows = received.response.headers.iter().map(|h| {
        row![
            mono(h.name.as_str()).width(260),
            mono(h.value.as_str()).style(theme::muted).width(Fill),
        ]
        .spacing(12)
        .padding([3, 0])
        .into()
    });
    let header = row![
        text("Key").size(SMALL).style(theme::muted).width(260),
        text("Value").size(SMALL).style(theme::muted),
    ]
    .spacing(12);
    column![header, scrollable(column(rows).width(Fill)).height(Fill)]
        .spacing(4)
        .into()
}

fn mono<'a>(content: &'a str) -> iced::widget::Text<'a> {
    text(content)
        .font(Font::MONOSPACE)
        .size(TEXT)
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::WordOrGlyph)
}
