//! Real-window frame-time probe for F06-AC10.
//!
//! Opens a window showing a 50 MB body and scrolls on every frame (mostly
//! wheel-sized steps, with a jump every 10 frames). Run with
//! `ICED_PRESENT_MODE=immediate` so vsync does not cap the frame interval.
//! Prints one JSON line of frame intervals, then exits.

use std::num::NonZeroU32;
use std::time::Instant;

use ferry_app::Bytes;
use ferry_app::body_view::{BodyView, Message as Body};
use iced::{Subscription, Task};

const WARMUP: usize = 30;
const FRAMES: usize = 330;

struct Probe {
    view: Option<BodyView>,
    frames: Vec<Instant>,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Frame(Instant),
    Body(Body),
}

fn boot() -> Probe {
    let line = r#"    {"id": 1234567, "name": "ferry", "tags": ["alpha", "beta"], "active": true, "score": 98.6},"#;
    let mut body = String::with_capacity(50 * 1024 * 1024 + line.len());
    while body.len() < 50 * 1024 * 1024 {
        body.push_str(line);
        body.push('\n');
    }
    let view =
        NonZeroU32::new(4096).and_then(|w| BodyView::new(Bytes::from(body.into_bytes()), w).ok());
    Probe {
        view,
        frames: Vec::with_capacity(FRAMES),
    }
}

fn update(state: &mut Probe, message: Message) -> Task<Message> {
    let Some(view) = state.view.as_mut() else {
        eprintln!("failed to index body");
        return iced::exit();
    };
    match message {
        Message::Body(m) => view.update(m),
        Message::Frame(at) => {
            state.frames.push(at);
            let n = state.frames.len();
            if n.is_multiple_of(10) {
                view.update(Body::ScrolledTo(n * view.line_count() / FRAMES));
            } else {
                view.update(Body::ScrolledBy(3));
            }
            if n >= FRAMES {
                report(&state.frames[WARMUP..]);
                return iced::exit();
            }
        }
    }
    Task::none()
}

fn report(frames: &[Instant]) {
    let mut gaps: Vec<f64> = frames
        .windows(2)
        .map(|w| (w[1] - w[0]).as_secs_f64() * 1e3)
        .collect();
    gaps.sort_by(f64::total_cmp);
    let at = |q: f64| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = ((gaps.len().saturating_sub(1)) as f64 * q).round() as usize;
        gaps.get(i).copied().unwrap_or(f64::NAN)
    };
    println!(
        "{{\"frames\":{},\"frame_p50_ms\":{:.2},\"frame_p95_ms\":{:.2},\"frame_max_ms\":{:.2}}}",
        gaps.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

fn view(state: &Probe) -> iced::Element<'_, Message> {
    match &state.view {
        Some(v) => ferry_ui::body_view::view(v).map(Message::Body),
        None => iced::widget::text("failed to index body").into(),
    }
}

fn subscription(_: &Probe) -> Subscription<Message> {
    iced::window::frames().map(Message::Frame)
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("ferry scroll probe")
        .window_size((1280.0, 800.0))
        .subscription(subscription)
        .run()
}
