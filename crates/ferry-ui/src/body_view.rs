//! Virtualized response body view (F06-AC9, F06-AC10), with search
//! highlights (F06-AC8).
//!
//! Only the lines that fit the viewport are turned into widgets, so render
//! cost is independent of body size.

use ferry_app::Find;
use ferry_app::body_view::{BodyView, Mark, Message};
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::widget::{
    Row, column, container, mouse_area, responsive, row, sensor, text, vertical_slider,
};
use iced::{Fill, Font, Pixels, mouse};

use crate::{Element, theme};

/// Height of one display line, in logical pixels.
pub const LINE_HEIGHT: f32 = 18.0;
const TEXT_SIZE: f32 = 13.0;
/// Display lines moved per wheel notch.
const WHEEL_LINES: f32 = 3.0;

/// Render `state` filling the available space, highlighting the matches of
/// `find` (F06-AC8). Reports the viewport's size in display lines.
pub fn view<'a>(state: &'a BodyView, find: Option<&'a Find>) -> Element<'a, Message> {
    let content = responsive(move |size| {
        let rows = rows_for_height(size.height);
        let gutter_digits = digits(state.line_count());
        let lines = match find.filter(|f| !f.matches.is_empty()) {
            Some(find) => column(
                state
                    .visible_marked(rows, &find.matches, find.query.len(), find.current())
                    .into_iter()
                    .map(|(n, pieces)| {
                        let pieces = pieces.into_iter().map(|(piece, mark)| match mark {
                            Mark::Plain => mono(piece).into(),
                            Mark::Match => container(mono(piece)).style(theme::find_match).into(),
                            Mark::Current => container(mono(piece).color(theme::BACKGROUND))
                                .style(theme::find_current)
                                .into(),
                        });
                        row![
                            mono(format!("{:>gutter_digits$}", n + 1)),
                            Row::with_children(pieces),
                        ]
                        .spacing(12)
                        .into()
                    }),
            ),
            None => column(state.visible(rows).into_iter().map(|(n, line)| {
                row![mono(format!("{:>gutter_digits$}", n + 1)), mono(line),]
                    .spacing(12)
                    .into()
            })),
        };
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
    });
    sensor(content)
        .on_show(|size| Message::Resized(rows_for_height(size.height)))
        .on_resize(|size| Message::Resized(rows_for_height(size.height)))
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
