//! The response pane: Body, Cookies, Headers and Test Results tabs, with the
//! status, time (and its breakdown) and size on the right (F06, F14-AC3).

use ferry_app::{
    App, BodyFormat, FILTER_TOO_LARGE, FilterState, Message as AppMessage, PREVIEW_UNAVAILABLE,
    Phase, Received, ResponseTab, Tab, format,
};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{
    button, center, column, container, row, rule, scrollable, sensor, space, text, text_input,
    tooltip,
};
use iced::{Alignment, Fill, FillPortion, Font};

use crate::request::underline;
use crate::screen::{SMALL, TEXT};
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
pub const NO_COOKIES: &str = "No cookies were set by this response";
pub const SAVE_AS_EXAMPLE: &str = "Save as Example";
/// Why Save as Example is disabled (F32-AC1).
pub const SAVE_FIRST: &str = "Save the request to a collection first";
/// Id of the find bar's field, for focus (F06-AC8).
pub const FIND_INPUT: &str = "find-input";
/// Placeholder of the filter field (F22-AC4).
pub const FILTER_PLACEHOLDER: &str = "Filter with JSONPath, e.g. $.items[*].name";

fn app(m: AppMessage) -> Message {
    Message::App(m)
}

/// The response pane for `tab`.
#[must_use]
pub fn view<'a>(state: &'a App, tab: &'a Tab) -> Element<'a, Message> {
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
        Phase::Received(received) => self::received(state, tab, received),
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
        ResponseTab::Cookies => format!("Cookies ({})", received.cookies.len()),
        ResponseTab::Headers => format!("Headers ({})", received.response.headers.len()),
        ResponseTab::TestResults if received.tests.is_empty() => "Test Results".into(),
        ResponseTab::TestResults => {
            let passed = received
                .tests
                .iter()
                .filter(|t| t.status == ferry_app::TestStatus::Passed)
                .count();
            format!("Test Results ({passed}/{})", received.tests.len())
        }
    }
}

fn received<'a>(state: &'a App, tab: &'a Tab, received: &'a Received) -> Element<'a, Message> {
    let selected = tab.response_tab;
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
        ResponseTab::Body => body(state, tab, received),
        ResponseTab::Headers => headers(received),
        ResponseTab::Cookies => cookies(received),
        ResponseTab::TestResults => crate::tests::view(tab, received),
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

fn body<'a>(state: &'a App, tab: &'a Tab, received: &'a Received) -> Element<'a, Message> {
    let formats = row(received.formats().into_iter().map(|f| {
        button(text(f.to_string()).size(SMALL))
            .on_press(app(AppMessage::BodyFormatSelected(f)))
            .padding([2, 10])
            .style(theme::flat(f == received.format))
            .into()
    }))
    .spacing(2);
    let mut toolbar = row![formats].spacing(10).align_y(Alignment::Center);
    let pretty = received.format == BodyFormat::Pretty && received.pretty.is_some();
    if pretty && received.filter_available() {
        toolbar = toolbar.push(
            text_input(FILTER_PLACEHOLDER, &received.filter.expression)
                .on_input(|s| app(AppMessage::FilterChanged(s)))
                .size(SMALL)
                .padding([3, 8])
                .width(300)
                .style(theme::input),
        );
    } else if pretty {
        toolbar = toolbar.push(text(FILTER_TOO_LARGE).size(SMALL).style(theme::faint));
    }
    toolbar = toolbar.push(space().width(Fill));
    if received.searchable() {
        toolbar = toolbar.push(tooltip(
            button(text("Search").size(SMALL))
                .on_press(app(AppMessage::FindOpen))
                .padding([2, 10])
                .style(theme::flat(received.find.is_some())),
            container(text(FIND_SHORTCUT).size(SMALL))
                .padding(6)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        ));
    }
    if received.preview_type().is_some() {
        toolbar = toolbar.push(
            button(text(OPEN_IN_BROWSER).size(SMALL))
                .on_press(app(AppMessage::OpenInBrowser))
                .padding([2, 10])
                .style(theme::secondary),
        );
    }
    toolbar = toolbar.push(save_as_example(tab));
    let mut parts = column![toolbar].spacing(6);
    if let Some(find) = &received.find {
        parts = parts.push(find_bar(find));
    }
    if let Some(note) = &received.note {
        parts = parts.push(text(note.as_str()).size(SMALL).style(theme::muted));
    }
    if received.format == BodyFormat::Pretty {
        match &received.filter.state {
            FilterState::Invalid(error) => {
                parts = parts.push(text(error.as_str()).size(SMALL).style(theme::error));
            }
            FilterState::Running => {
                parts = parts.push(text("Filtering…").size(SMALL).style(theme::faint));
            }
            FilterState::Off | FilterState::Shown(_) => {}
        }
    }
    let content: Element<'_, Message> = if received.format == BodyFormat::Preview {
        preview(state)
    } else if received.response.body.is_empty() && received.format != BodyFormat::Hex {
        center(text(EMPTY_BODY).size(TEXT).style(theme::muted)).into()
    } else {
        body_view::view(received.view(), received.find.as_ref()).map(|m| app(AppMessage::Body(m)))
    };
    parts.push(content).into()
}

/// The area the web view covers, or why there is none (F22-AC1, F22-AC3).
fn preview(state: &App) -> Element<'_, Message> {
    if state.preview_available() {
        // Empty: the native web view is laid over it. It reports layout
        // changes so the web view follows.
        return sensor(
            container(space())
                .id(iced::widget::Id::new(crate::preview::PREVIEW_AREA))
                .width(Fill)
                .height(Fill),
        )
        .on_show(|_| Message::PreviewMoved)
        .on_resize(|_| Message::PreviewMoved)
        .into();
    }
    let mut lines = column![text(PREVIEW_UNAVAILABLE).size(14)]
        .spacing(8)
        .align_x(Alignment::Center);
    if let Some(error) = state.preview_error() {
        lines = lines.push(text(error).size(SMALL).style(theme::muted));
    }
    lines = lines.push(
        button(text(OPEN_IN_BROWSER).size(TEXT))
            .on_press(app(AppMessage::OpenInBrowser))
            .padding([6, 14])
            .style(theme::primary),
    );
    center(lines).into()
}

/// The button that opens the body with the system's application (F22-AC3).
pub const OPEN_IN_BROWSER: &str = "Open in Browser";

/// The shortcut hint on the Search button.
pub const FIND_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "Find in body (⌘+F)"
} else {
    "Find in body (Ctrl+F)"
};

/// `Save as Example`, disabled with a hint in an unlinked tab (F32-AC1).
fn save_as_example(tab: &Tab) -> Element<'_, Message> {
    let button = button(text(SAVE_AS_EXAMPLE).size(SMALL))
        .padding([2, 10])
        .style(theme::secondary);
    if tab.link.is_some() {
        button.on_press(app(AppMessage::SaveAsExample)).into()
    } else {
        tooltip(
            button,
            container(text(SAVE_FIRST).size(SMALL))
                .padding(6)
                .style(theme::dialog),
            tooltip::Position::Bottom,
        )
        .into()
    }
}

/// The find bar: field, `<i> of <n>`, previous, next and close (F06-AC8).
fn find_bar(find: &ferry_app::Find) -> Element<'_, Message> {
    let step = |label: &'static str, message: AppMessage| {
        let b = button(text(label).size(SMALL))
            .padding([2, 8])
            .style(theme::flat(false));
        if find.matches.is_empty() {
            b
        } else {
            b.on_press(app(message))
        }
    };
    row![
        text_input("Find in body", &find.query)
            .id(iced::widget::Id::new(FIND_INPUT))
            .on_input(|s| app(AppMessage::FindQuery(s)))
            .on_submit(Message::FindSubmit)
            .size(SMALL)
            .padding([3, 8])
            .width(260)
            .style(theme::input),
        text(find.label()).size(SMALL).style(theme::muted).width(90),
        step("↑", AppMessage::FindPrevious),
        step("↓", AppMessage::FindNext),
        button(text("×").size(TEXT))
            .on_press(app(AppMessage::FindClose))
            .padding([0, 8])
            .style(theme::flat(false)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// The cookies the final response set (F17-AC4).
fn cookies(received: &Received) -> Element<'_, Message> {
    if received.cookies.is_empty() {
        return center(text(NO_COOKIES).size(TEXT).style(theme::muted)).into();
    }
    // Shares of the width, so the table always fits the pane.
    const WIDTHS: [u16; 7] = [3, 4, 3, 2, 4, 2, 2];
    let cell = |s: String, i: usize| mono_owned(s).width(FillPortion(WIDTHS[i]));
    let head = |s: &'static str, i: usize| {
        text(s)
            .size(SMALL)
            .style(theme::muted)
            .width(FillPortion(WIDTHS[i]))
    };
    let header = row![
        head("Name", 0),
        head("Value", 1),
        head("Domain", 2),
        head("Path", 3),
        head("Expires", 4),
        head("HttpOnly", 5),
        head("Secure", 6),
    ]
    .spacing(12);
    let yes_no = |b: bool| if b { "true" } else { "false" }.to_owned();
    let rows = received.cookies.iter().map(|c| {
        row![
            cell(c.name.clone(), 0),
            cell(c.value.clone(), 1),
            cell(c.domain.clone(), 2),
            cell(c.path.clone(), 3),
            cell(crate::dialogs::expires_label(c.expires), 4),
            cell(yes_no(c.http_only), 5),
            cell(yes_no(c.secure), 6),
        ]
        .spacing(12)
        .padding([3, 0])
        .into()
    });
    column![header, scrollable(column(rows).width(Fill)).height(Fill)]
        .spacing(4)
        .into()
}

fn mono_owned<'a>(content: String) -> iced::widget::Text<'a> {
    text(content)
        .font(Font::MONOSPACE)
        .size(TEXT)
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::WordOrGlyph)
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
