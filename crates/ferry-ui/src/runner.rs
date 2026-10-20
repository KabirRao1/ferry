//! The Runner tab (F16-AC1..AC6): settings and requests on the left,
//! results on the right.

use ferry_app::runner::{RunStatus, Runner};
use ferry_app::{Message as AppMessage, RunnerDoc, RunnerEdit, Tab, TestStatus, format};
use iced::widget::{
    Column, button, checkbox, column, container, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Element, Fill, Length};

use crate::screen::{SMALL, TEXT};
use crate::shell::Message;
use crate::theme;

/// Shown before the first run.
pub const NO_RUN: &str = "Choose the requests and press Run. Results appear here as they finish.";
/// Shown when the collection or folder holds no requests.
pub const NO_REQUESTS: &str = "There are no requests to run here.";

fn app(m: AppMessage) -> Message {
    Message::App(m)
}

fn edit(e: RunnerEdit) -> Message {
    app(AppMessage::RunnerEdit(e))
}

fn field<'a>(
    label: &'a str,
    value: &'a str,
    on_input: fn(String) -> RunnerEdit,
    error: Option<&'a str>,
    enabled: bool,
) -> Element<'a, Message> {
    let mut input = text_input("", value).size(TEXT).padding(6).width(120);
    if enabled {
        input = input.on_input(move |v| edit(on_input(v)));
    }
    let mut col = column![
        row![text(label).size(TEXT).width(Length::Fixed(110.0)), input]
            .spacing(8)
            .align_y(Alignment::Center)
    ]
    .spacing(2);
    if let Some(error) = error {
        col = col.push(container(text(error).size(SMALL).style(theme::error)).padding([0, 118]));
    }
    col.into()
}

fn settings(doc: &RunnerDoc) -> Element<'_, Message> {
    let enabled = !doc.running();
    let mut list = column![].spacing(4);
    if doc.items.is_empty() {
        list = list.push(text(NO_REQUESTS).size(TEXT).style(theme::muted));
    }
    for (i, item) in doc.items.iter().enumerate() {
        let mut check = checkbox(item.checked).size(14);
        if enabled {
            check = check.on_toggle(move |on| edit(RunnerEdit::Item(i, on)));
        }
        list = list.push(
            row![
                check,
                text(item.method.as_str())
                    .size(SMALL)
                    .color(theme::method_color(&item.method))
                    .width(54),
                text(item.name.as_str()).size(TEXT),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    let data: Element<'_, Message> = match (&doc.data_file, &doc.data) {
        (Some(path), data) => {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let mut col = column![
                row![
                    text(name).size(TEXT),
                    button(text("Clear").size(SMALL))
                        .on_press_maybe(enabled.then(|| edit(RunnerEdit::ClearData)))
                        .padding([2, 8])
                        .style(theme::flat(false)),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            ];
            match data {
                Some(Ok(rows)) => {
                    col = col.push(
                        text(format!("{} rows, one per iteration", rows.len()))
                            .size(SMALL)
                            .style(theme::muted),
                    );
                }
                Some(Err(error)) => {
                    col = col.push(text(error.as_str()).size(SMALL).style(theme::error));
                }
                None => {}
            }
            col.into()
        }
        (None, _) => button(text("Select File").size(TEXT))
            .on_press_maybe(enabled.then(|| app(AppMessage::RunnerPickData)))
            .padding([4, 10])
            .style(theme::flat(false))
            .into(),
    };
    let mut bail = checkbox(doc.bail)
        .label("Stop on first failure")
        .size(14)
        .text_size(TEXT);
    if enabled {
        bail = bail.on_toggle(|on| edit(RunnerEdit::Bail(on)));
    }
    let action = if doc.running() {
        button(text("Stop").size(TEXT))
            .on_press(app(AppMessage::RunStop))
            .padding([6, 18])
            .style(theme::flat(false))
    } else {
        button(text("Run").size(TEXT))
            .on_press_maybe(doc.can_run().then(|| app(AppMessage::RunStart)))
            .padding([6, 18])
            .style(theme::primary)
    };
    column![
        field(
            "Iterations",
            &doc.iterations,
            RunnerEdit::Iterations,
            doc.iterations_error(),
            enabled,
        ),
        field(
            "Delay (ms)",
            &doc.delay,
            RunnerEdit::Delay,
            doc.delay_error(),
            enabled,
        ),
        row![text("Data").size(TEXT).width(Length::Fixed(110.0)), data].spacing(8),
        bail,
        action,
        text("Requests").size(SMALL).style(theme::muted),
        scrollable(list).height(Fill),
    ]
    .spacing(12)
    .width(Length::Fixed(360.0))
    .height(Fill)
    .into()
}

fn summary_line(run: &Runner, now: std::time::SystemTime) -> Element<'_, Message> {
    let s = run.summary(now);
    let state = match run.status {
        RunStatus::Running => "Running",
        RunStatus::Finished => "Finished",
        RunStatus::Stopped => "Stopped",
    };
    row![
        text(state).size(TEXT),
        text(format!(
            "Iterations {}/{} · Requests {} · Passed {} · Failed {} · {}",
            s.iterations,
            run.iterations(),
            s.requests,
            s.passed,
            s.failed,
            format::duration(s.duration)
        ))
        .size(TEXT)
        .style(theme::muted),
    ]
    .spacing(12)
    .into()
}

fn results<'a>(doc: &'a RunnerDoc, run: &'a Runner) -> Element<'a, Message> {
    let mut list = Column::new().spacing(2);
    for (i, r) in run.results.iter().enumerate() {
        let status: Element<'_, Message> = match r.status {
            Some(code) => text(code.to_string())
                .size(TEXT)
                .color(theme::status_color(code))
                .width(44)
                .into(),
            None => text("Error")
                .size(TEXT)
                .style(theme::error)
                .width(44)
                .into(),
        };
        let tests = text(format!("✓ {}  ✗ {}", r.passed(), r.failed_tests()))
            .size(SMALL)
            .style(if r.failed() {
                theme::error
            } else {
                theme::muted
            });
        list = list.push(
            button(
                row![
                    text(format!("#{}", r.iteration + 1))
                        .size(SMALL)
                        .style(theme::faint)
                        .width(34),
                    text(r.method.as_str())
                        .size(SMALL)
                        .color(theme::method_color(&r.method))
                        .width(54),
                    text(r.name.as_str()).size(TEXT).width(Fill),
                    status,
                    text(format::duration(r.time))
                        .size(SMALL)
                        .style(theme::muted)
                        .width(64),
                    tests,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(app(AppMessage::RunSelect(i)))
            .padding([4, 8])
            .width(Fill)
            .style(theme::flat(doc.selected == Some(i))),
        );
    }
    scrollable(list).height(Length::FillPortion(3)).into()
}

fn details<'a>(run: &'a Runner, index: usize) -> Element<'a, Message> {
    let Some(r) = run.results.get(index) else {
        return space().into();
    };
    let mut col = column![
        text(format!("{} {}", r.method, r.url))
            .size(SMALL)
            .font(iced::Font::MONOSPACE)
    ]
    .spacing(4);
    if let Some(error) = &r.error {
        col = col.push(text(error.as_str()).size(SMALL).style(theme::error));
    }
    if r.tests.is_empty() {
        col = col.push(text("No tests").size(SMALL).style(theme::muted));
    }
    for t in &r.tests {
        let (label, style): (&str, fn(&iced::Theme) -> iced::widget::text::Style) = match t.status {
            TestStatus::Passed => ("PASS", theme::success),
            TestStatus::Failed => ("FAIL", theme::error),
            TestStatus::Skipped => ("SKIP", theme::muted),
        };
        let mut line = row![
            text(label).size(SMALL).style(style).width(40),
            text(t.name.as_str()).size(SMALL)
        ]
        .spacing(6);
        if let Some(error) = &t.error {
            line = line.push(text(format!("— {error}")).size(SMALL).style(theme::muted));
        }
        col = col.push(line);
    }
    container(scrollable(col).width(Fill))
        .width(Fill)
        .height(Length::FillPortion(2))
        .padding(8)
        .style(theme::dialog)
        .into()
}

/// The Runner tab for `tab`.
pub fn view<'a>(
    tab: &'a Tab,
    doc: &'a RunnerDoc,
    now: std::time::SystemTime,
) -> Element<'a, Message> {
    let _ = tab;
    let right: Element<'_, Message> = match &doc.run {
        None => container(text(NO_RUN).size(TEXT).style(theme::muted))
            .padding(8)
            .into(),
        Some(run) => {
            let mut col = column![summary_line(run, now)].spacing(8);
            for note in &run.notes {
                col = col.push(text(note.as_str()).size(SMALL).style(theme::warning));
            }
            col = col.push(results(doc, run));
            if let Some(i) = doc.selected {
                col = col.push(details(run, i));
            }
            if run.status != RunStatus::Running {
                let mut export = row![
                    button(text("Export Results").size(TEXT))
                        .on_press(app(AppMessage::RunExport))
                        .padding([4, 10])
                        .style(theme::flat(false))
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                if let Some(notice) = &doc.notice {
                    export = export.push(text(notice.as_str()).size(SMALL).style(theme::muted));
                }
                col = col.push(export);
            }
            col.height(Fill).into()
        }
    };
    column![
        row![
            text("RUNNER").size(SMALL).style(theme::muted),
            text(doc.name.as_str()).size(16),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![settings(doc), container(right).width(Fill).height(Fill)].spacing(24),
    ]
    .spacing(14)
    .padding([12, 16])
    .height(Fill)
    .into()
}
