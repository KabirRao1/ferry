//! Frame-time spike for F06-AC10: a 50 MB body scrolled through a headless
//! simulator. Prints one JSON line consumed by `cargo xtask budgets`.
//!
//! Each frame = view() + layout (`build_ms`) and draw + CPU raster (`draw_ms`).
//! tiny-skia rasterizes on the CPU at 2x scale, so this overstates GPU cost.

use std::num::NonZeroU32;
use std::time::Instant;

use ferry_app::Bytes;
use ferry_app::body_view::{BodyView, Message};
use ferry_ui::body_view;
use iced::{Settings, Size};
use iced_test::Simulator;

const FRAMES: usize = 120;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let line = r#"    {"id": 1234567, "name": "ferry", "tags": ["alpha", "beta"], "active": true, "score": 98.6},"#;
    let mut body = String::with_capacity(50 * 1024 * 1024 + line.len());
    while body.len() < 50 * 1024 * 1024 {
        body.push_str(line);
        body.push('\n');
    }
    let bytes = Bytes::from(body.into_bytes());

    let t = Instant::now();
    let wrap = NonZeroU32::new(4096).ok_or("wrap")?;
    let mut state = BodyView::new(bytes.clone(), wrap)?;
    let index_ms = t.elapsed().as_secs_f64() * 1e3;

    let size = Size::new(1280.0, 800.0);
    let lines = state.line_count();
    let mut build = Vec::with_capacity(FRAMES);
    let mut draw = Vec::with_capacity(FRAMES);
    for i in 0..FRAMES {
        // Mix wheel steps with large jumps across the whole body.
        if i.is_multiple_of(10) {
            state.update(Message::ScrolledTo(i * lines / FRAMES));
        } else {
            state.update(Message::ScrolledBy(3));
        }
        let t = Instant::now();
        let mut ui = Simulator::with_size(Settings::default(), size, body_view::view(&state, None));
        build.push(t.elapsed().as_secs_f64() * 1e3);
        let t = Instant::now();
        let _ = ui.snapshot(&iced::Theme::Dark)?;
        draw.push(t.elapsed().as_secs_f64() * 1e3);
    }
    // Same-size empty frame: the fixed cost of CPU raster + readback.
    let mut empty = Vec::with_capacity(FRAMES);
    for _ in 0..FRAMES {
        let t = Instant::now();
        let mut ui = Simulator::with_size(
            Settings::default(),
            size,
            iced::Element::<Message>::from(iced::widget::space()),
        );
        let _ = ui.snapshot(&iced::Theme::Dark)?;
        empty.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let empty_p50 = p(&empty, 0.50);
    // First frame includes renderer and font initialization; report it separately.
    let first = build[0] + draw[0];
    let frame: Vec<f64> = build
        .iter()
        .zip(&draw)
        .skip(1)
        .map(|(b, d)| b + d)
        .collect();
    println!(
        "{{\"body_bytes\":{},\"lines\":{lines},\"index_ms\":{index_ms:.2},\"first_frame_ms\":{first:.2},\"build_p95_ms\":{:.2},\"draw_p95_ms\":{:.2},\"frame_p50_ms\":{:.2},\"frame_p95_ms\":{:.2},\"frame_max_ms\":{:.2},\"empty_frame_p50_ms\":{empty_p50:.2},\"body_overhead_p95_ms\":{:.2}}}",
        bytes.len(),
        p(&build[1..], 0.95),
        p(&draw[1..], 0.95),
        p(&frame, 0.50),
        p(&frame, 0.95),
        p(&frame, 1.0),
        p(&frame, 0.95) - empty_p50,
    );
    Ok(())
}

fn p(samples: &[f64], q: f64) -> f64 {
    let mut v = samples.to_vec();
    v.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((v.len().saturating_sub(1)) as f64 * q).round() as usize;
    v.get(i).copied().unwrap_or(f64::NAN)
}
