//! Baseline for budgets: an iced window with no content, instrumented with
//! the same startup probe as `ferry`. Built with the same iced features.

use ferry_ui::probe::Probe;

#[derive(Debug, Clone, Copy)]
struct Frame;

fn update(probe: &mut Probe, _: Frame) {
    probe.frame();
}

fn view(_: &Probe) -> iced::Element<'_, Frame> {
    iced::widget::space().into()
}

fn subscription(probe: &Probe) -> iced::Subscription<Frame> {
    probe.subscription(|| Frame)
}

fn main() -> iced::Result {
    iced::application(Probe::from_env, update, view)
        .title("empty")
        .subscription(subscription)
        .run()
}
