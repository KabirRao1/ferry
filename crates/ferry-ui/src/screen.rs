//! The main window, laid out like Postman (F14): header, left sidebar,
//! workbench (tab strip, request builder, response pane), footer.

use ferry_app::{App, Doc, SidebarSection};
use iced::widget::{
    center, column, container, mouse_area, opaque, responsive, row, space, stack, text,
};
use iced::{Alignment, Fill, FillPortion, Shrink, mouse};

use crate::editors::Editors;
use crate::shell::Message;
use crate::{Element, chrome, dialogs, docs, request, response, sidebar, theme};

/// UI text size.
pub const TEXT: f32 = 13.0;
/// Smaller text: labels, footer, hints.
pub const SMALL: f32 = 12.0;

/// The whole window.
#[must_use]
pub fn view<'a>(
    app: &'a App,
    editors: &'a Editors,
    resizing: bool,
    save_error: Option<&'a str>,
) -> Element<'a, Message> {
    let mut body = row![];
    if app.sidebar_open() {
        body = body.push(sidebar::view(app));
        body = body.push(vline());
    }
    body = body.push(workbench(app, editors, resizing));
    let main = column![
        chrome::header(app),
        hline(),
        body.height(Fill),
        hline(),
        chrome::footer(app, save_error),
    ];
    let main = container(main)
        .width(Fill)
        .height(Fill)
        .style(theme::app_background);
    match dialogs::view(app) {
        Some(dialog) => stack![main, opaque(dialog)].into(),
        None => main.into(),
    }
}

fn workbench<'a>(app: &'a App, editors: &'a Editors, resizing: bool) -> Element<'a, Message> {
    let tab = app.tab();
    if tab.doc != Doc::Request {
        return column![
            chrome::tab_strip(app),
            hline(),
            docs::view(app, tab, editors)
        ]
        .height(Fill)
        .into();
    }
    let split = app.split();
    responsive(move |size| {
        // Portions out of 1000 keep the split precise; truncation is fine.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let top = (split * 1000.0) as u16;
        let request = container(request::view(app, app.tab(), editors))
            .height(FillPortion(top.max(1)))
            .width(Fill);
        let response = container(response::view(app.tab()))
            .height(FillPortion(1000u16.saturating_sub(top).max(1)))
            .width(Fill);
        let divider = mouse_area(container(space().height(4).width(Fill)).style(if resizing {
            theme::divider_active
        } else {
            theme::divider
        }))
        .on_press(Message::ResizeStart)
        .interaction(mouse::Interaction::ResizingVertically);
        let tabs = chrome::tab_strip(app);
        let content = column![tabs, hline(), request, divider, response].height(Fill);
        let tabs_height = chrome::TAB_STRIP_HEIGHT + 1.0;
        if resizing {
            // Only while dragging the divider: track the pointer anywhere in
            // the workbench, and stop on release.
            let height = size.height;
            mouse_area(content)
                .on_release(Message::ResizeEnd)
                .on_move(move |p| {
                    let usable = (height - tabs_height).max(1.0);
                    Message::ResizeMoved((p.y - tabs_height) / usable)
                })
                .interaction(mouse::Interaction::ResizingVertically)
                .into()
        } else {
            content.into()
        }
    })
    .into()
}

/// A 1 px horizontal line.
#[must_use]
pub fn hline<'a>() -> Element<'a, Message> {
    container(space().height(1).width(Fill))
        .style(theme::divider)
        .into()
}

/// A 1 px vertical line.
#[must_use]
pub fn vline<'a>() -> Element<'a, Message> {
    container(space().width(1).height(Fill))
        .style(theme::divider)
        .into()
}

/// Muted explanatory text for features from later phases (F14-AC5).
#[must_use]
pub fn coming<'a>(title: &'a str, detail: &'a str) -> Element<'a, Message> {
    center(
        column![
            text(title).size(14),
            text(detail).size(TEXT).style(theme::muted).center(),
        ]
        .spacing(6)
        .max_width(420)
        .align_x(Alignment::Center),
    )
    .into()
}

/// Section labels used by the sidebar.
#[must_use]
pub fn section_label(section: SidebarSection) -> &'static str {
    match section {
        SidebarSection::Collections => "Collections",
        SidebarSection::Environments => "Environments",
        SidebarSection::History => "History",
    }
}

/// A container that never grows (for toolbars).
pub fn bar<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content).height(Shrink).width(Fill).into()
}
