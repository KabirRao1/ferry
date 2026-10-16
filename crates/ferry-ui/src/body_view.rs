//! Virtualized response body view (F06-AC9, F06-AC10).
//!
//! Only the lines that fit the viewport are turned into widgets, so render
//! cost is independent of body size.

use ferry_app::body_view::{BodyView, Message};
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::widget::{column, container, mouse_area, responsive, row, text, vertical_slider};
use iced::{Fill, Font, Pixels, mouse};

use crate::Element;

/// Height of one display line, in logical pixels.
pub const LINE_HEIGHT: f32 = 18.0;
const TEXT_SIZE: f32 = 13.0;
/// Display lines moved per wheel notch.
const WHEEL_LINES: f32 = 3.0;

/// Render `state` filling the available space.
pub fn view(state: &BodyView) -> Element<'_, Message> {
    responsive(move |size| {
        let rows = rows_for_height(size.height);
        let gutter_digits = digits(state.line_count());
        let lines = column(state.visible(rows).into_iter().map(|(n, line)| {
            row![mono(format!("{:>gutter_digits$}", n + 1)), mono(line),]
                .spacing(12)
                .into()
        }));
        let body = mouse_area(container(lines).width(Fill).height(Fill).clip(true))
            .on_scroll(|delta| Message::ScrolledBy(lines_for_delta(delta)));
        if state.top() == 0 && state.line_count() <= rows {
            // Everything fits: no scrollbar.
            return body.into();
        }
        let last = u32::try_from(state.line_count().saturating_sub(1)).unwrap_or(u32::MAX);
        let top = u32::try_from(state.top()).unwrap_or(u32::MAX);
        // Sliders grow upward; invert so the top of the track is line 0.
        let scrollbar = vertical_slider(0..=last, last - top.min(last), move |v| {
            Message::ScrolledTo((last - v) as usize)
        })
        .width(10);
        row![body, scrollbar].into()
    })
    .into()
}

fn mono<'a>(content: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(content)
        .font(Font::MONOSPACE)
        .size(TEXT_SIZE)
        .line_height(LineHeight::Absolute(Pixels(LINE_HEIGHT)))
        .shaping(Shaping::Basic)
        .wrapping(Wrapping::None)
}

/// Display lines that fit (fully or partly) in `height` pixels.
#[must_use]
pub fn rows_for_height(height: f32) -> usize {
    if height.is_finite() && height > 0.0 {
        // Bounded by any realistic window height; truncation is intended.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rows = (height / LINE_HEIGHT).ceil() as usize;
        rows
    } else {
        0
    }
}

/// Convert a wheel or touchpad delta into display lines (positive scrolls down).
#[must_use]
pub fn lines_for_delta(delta: mouse::ScrollDelta) -> isize {
    let lines = match delta {
        mouse::ScrollDelta::Lines { y, .. } => -y * WHEEL_LINES,
        mouse::ScrollDelta::Pixels { y, .. } => -y / LINE_HEIGHT,
    };
    if lines.is_finite() {
        // Rounded away from zero so small touchpad deltas still move; clamped by `as`.
        #[allow(clippy::cast_possible_truncation)]
        let rounded = if lines < 0.0 {
            lines.floor()
        } else {
            lines.ceil()
        } as isize;
        rounded
    } else {
        0
    }
}

fn digits(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |d| d as usize + 1)
}
