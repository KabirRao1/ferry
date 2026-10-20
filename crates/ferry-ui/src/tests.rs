//! The response's Test Results tab (F15-AC10).

use ferry_app::{Message as AppMessage, Received, Tab, TestFilter, TestStatus};
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Fill};

use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::theme;

/// Shown when the response has no tests.
pub const NO_TESTS: &str = "No tests for this request";

fn badge(status: TestStatus) -> Element<'static, Message> {
    let (label, style): (&str, fn(&iced::Theme) -> iced::widget::text::Style) = match status {
        TestStatus::Passed => ("PASS", theme::success),
        TestStatus::Failed => ("FAIL", theme::error),
        TestStatus::Skipped => ("SKIP", theme::muted),
    };
    text(label)
        .size(SMALL)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..iced::Font::DEFAULT
        })
        .style(style)
        .width(40)
        .into()
}

/// Filters, any post-response script error, then the tests.
pub fn view<'a>(tab: &'a Tab, received: &'a Received) -> Element<'a, Message> {
    let filters = row(TestFilter::ALL.map(|filter| {
        button(text(filter.to_string()).size(TEXT))
            .on_press(Message::App(AppMessage::TestFilterSelected(filter)))
            .padding([3, 10])
            .style(theme::sub_tab(filter == tab.test_filter))
            .into()
    }))
    .spacing(4);
    let mut page = column![filters].spacing(10);
    if let Some(error) = &received.script_error {
        page = page.push(text(error.as_str()).size(TEXT).style(theme::error));
    }
    if received.tests.is_empty() {
        page = page.push(text(NO_TESTS).size(TEXT).style(theme::muted));
        return page.into();
    }
    let mut list = column![].spacing(6);
    for test in received
        .tests
        .iter()
        .filter(|t| tab.test_filter.keeps(t.status))
    {
        let mut line = column![
            row![badge(test.status), text(test.name.as_str()).size(TEXT)]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(2);
        if let Some(error) = &test.error {
            line = line.push(
                container(text(error.as_str()).size(SMALL).style(theme::muted)).padding([0, 48]),
            );
        }
        list = list.push(line);
    }
    page.push(scrollable(list).height(Fill)).into()
}
