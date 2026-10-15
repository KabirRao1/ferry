//! F00-AC1: UI behavior is testable headless with `iced_test`.
//! F06-AC9 / F06-AC10: the virtualized body view renders only visible lines.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::num::NonZeroU32;
use std::sync::Arc;

use ferry_app::body_view::{BodyView, Message};
use ferry_ui::body_view::{self, LINE_HEIGHT};
use ferry_ui::shell::Shell;
use iced::{Settings, Size, mouse};
use iced_test::{Simulator, simulator};

fn body(text: &str) -> BodyView {
    BodyView::new(Arc::from(text.as_bytes()), NonZeroU32::new(4096).unwrap()).unwrap()
}

fn numbered(n: usize) -> String {
    (0..n).map(|i| format!("line-{i}\n")).collect()
}

#[test]
fn f00_ac1_shell_renders_empty_state_headless() {
    let shell = Shell::boot();
    let mut ui = simulator(shell.view());
    assert!(ui.find("Ferry").is_ok());
    assert!(
        ui.find("No request open. Press Ctrl+T to start one.")
            .is_ok()
    );
}

#[test]
fn f06_ac10_only_visible_lines_become_widgets() {
    let state = body(&numbered(100_000));
    let height = 10.0 * LINE_HEIGHT;
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, height),
        body_view::view(&state),
    );
    assert!(ui.find("line-0").is_ok());
    assert!(ui.find("line-9").is_ok());
    assert!(
        ui.find("line-10").is_err(),
        "line past the viewport was built"
    );
}

#[test]
fn f06_ac10_scrolled_view_starts_at_top_line() {
    let mut state = body(&numbered(1000));
    state.update(Message::ScrolledTo(500));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, 5.0 * LINE_HEIGHT),
        body_view::view(&state),
    );
    assert!(ui.find("line-500").is_ok());
    assert!(ui.find("line-499").is_err());
}

#[test]
fn f06_ac10_mouse_wheel_scrolls_by_lines() {
    let state = body(&numbered(1000));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(600.0, 200.0),
        body_view::view(&state),
    );
    ui.point_at(iced::Point::new(100.0, 100.0));
    let _ = ui.simulate([iced::Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    })]);
    assert_eq!(
        ui.into_messages().collect::<Vec<_>>(),
        vec![Message::ScrolledBy(3)]
    );
}

#[test]
fn f06_ac10_delta_conversion() {
    use body_view::lines_for_delta;
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
        -3
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Pixels { x: 0.0, y: -1.0 }),
        1
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Pixels { x: 0.0, y: 0.0 }),
        0
    );
    assert_eq!(
        lines_for_delta(mouse::ScrollDelta::Lines {
            x: 0.0,
            y: f32::NAN
        }),
        0
    );
    assert_eq!(body_view::rows_for_height(0.0), 0);
    assert_eq!(body_view::rows_for_height(LINE_HEIGHT * 2.5), 3);
    assert_eq!(body_view::rows_for_height(f32::INFINITY), 0);
}

/// Pixel snapshots are platform-specific (font rasterization differs), so they
/// are recorded and enforced on Windows only. See DECISIONS.md ADR-0006.
#[cfg(windows)]
#[test]
fn f00_ac1_shell_snapshot_matches() {
    let shell = Shell::boot();
    let mut ui = Simulator::with_size(Settings::default(), Size::new(640.0, 400.0), shell.view());
    let snap = ui.snapshot(&iced::Theme::Dark).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/shell_empty").unwrap(),
        "shell snapshot changed"
    );
}

#[cfg(windows)]
#[test]
fn f06_ac10_body_view_snapshot_matches() {
    let mut state = body(&numbered(1000));
    state.update(Message::ScrolledTo(42));
    let mut ui = Simulator::with_size(
        Settings::default(),
        Size::new(480.0, 180.0),
        body_view::view(&state),
    );
    let snap = ui.snapshot(&iced::Theme::Dark).unwrap();
    assert!(
        snap.matches_image("tests/snapshots/body_view_scrolled")
            .unwrap(),
        "body view snapshot changed"
    );
}
