//! The Scripts tab of a request, collection or folder (F15-AC1).

use ferry_app::{Message as AppMessage, ScriptPhase, Scripts, TabId};
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Element, Fill, Font, Length};

use crate::editors::Editors;
use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::theme;

/// What each phase's editor is for.
pub const PRE_REQUEST_HINT: &str = "Runs before the request is sent. Use pm.environment, pm.variables and pm.request to prepare it.";
pub const POST_RESPONSE_HINT: &str =
    "Runs after the response arrives. Use pm.test and pm.expect to check pm.response.";

fn label(phase: ScriptPhase) -> &'static str {
    match phase {
        ScriptPhase::PreRequest => "Pre-request",
        ScriptPhase::PostResponse => "Post-response",
    }
}

/// The phase list on the left and the selected phase's editor.
pub fn view<'a>(
    id: TabId,
    scripts: &'a Scripts,
    selected: ScriptPhase,
    editors: &'a Editors,
) -> Element<'a, Message> {
    let list = column(
        [ScriptPhase::PreRequest, ScriptPhase::PostResponse].map(|phase| {
            let mark = if scripts.get(phase).trim().is_empty() {
                ""
            } else {
                " ●"
            };
            button(text(format!("{}{mark}", label(phase))).size(TEXT))
                .on_press(Message::App(AppMessage::ScriptPhaseSelected(phase)))
                .padding([4, 10])
                .width(Fill)
                .style(theme::sub_tab(phase == selected))
                .into()
        }),
    )
    .spacing(2)
    .width(Length::Fixed(130.0));
    let hint = match selected {
        ScriptPhase::PreRequest => PRE_REQUEST_HINT,
        ScriptPhase::PostResponse => POST_RESPONSE_HINT,
    };
    let editor: Element<'a, Message> = match editors.script(id, selected) {
        Some(content) => text_editor(content)
            .on_action(move |action| Message::ScriptAction(selected, action))
            .placeholder("// JavaScript")
            .font(Font::MONOSPACE)
            .size(TEXT)
            .height(Fill)
            .padding(8)
            .into(),
        None => container(text("")).into(),
    };
    row![
        list,
        column![text(hint).size(SMALL).style(theme::muted), editor]
            .spacing(6)
            .width(Fill)
            .height(Fill),
    ]
    .spacing(12)
    .height(Fill)
    .into()
}
