//! The Code panel beside the request (F19-AC1).

use ferry_app::{App, Language, Message as AppMessage};
use iced::widget::{button, column, container, pick_list, row, scrollable, space, text};
use iced::{Alignment, Element, Fill, Font, Length};

use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::theme;

/// The panel's width.
pub const CODE_WIDTH: f32 = 440.0;

fn app(m: AppMessage) -> Message {
    Message::App(m)
}

/// Language, Copy and close above the code, or why there is none.
pub fn view(state: &App) -> Element<'_, Message> {
    let header = row![
        text("Code").size(14),
        space().width(Fill),
        pick_list(Language::ALL, Some(state.code_language()), |l| {
            app(AppMessage::CodeLanguage(l))
        })
        .text_size(TEXT)
        .padding([4, 8]),
        button(text("Copy").size(TEXT))
            .on_press_maybe(state.code().is_ok().then(|| app(AppMessage::CopyCode)))
            .padding([4, 10])
            .style(theme::secondary),
        button(text("×").size(14))
            .on_press(app(AppMessage::ToggleCode))
            .padding([2, 8])
            .style(theme::flat(false)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let body: Element<'_, Message> = match state.code() {
        Ok(code) => scrollable(
            container(text(code).size(SMALL).font(Font::MONOSPACE))
                .padding(10)
                .width(Fill),
        )
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        })
        .height(Fill)
        .into(),
        Err(error) => container(text(error).size(TEXT).style(theme::muted))
            .padding(10)
            .into(),
    };
    container(column![header, container(body).style(theme::dialog).height(Fill)].spacing(8))
        .padding([10, 12])
        .width(Length::Fixed(CODE_WIDTH))
        .height(Fill)
        .style(theme::surface)
        .into()
}
