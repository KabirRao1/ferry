//! The `ferry` desktop binary.

use ferry_ui::shell::Shell;

fn main() -> iced::Result {
    iced::application(Shell::boot, Shell::update, Shell::view)
        .title("Ferry")
        .subscription(Shell::subscription)
        .run()
}
