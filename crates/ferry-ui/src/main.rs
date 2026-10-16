//! The `ferry` desktop binary.

use ferry_app::{Client, Config};
use ferry_ui::shell::Shell;

fn main() -> iced::Result {
    let client = match Client::new(Config::default()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ferry: could not start the HTTP client: {error}");
            std::process::exit(1);
        }
    };
    iced::application(
        move || Shell::new(client.clone()),
        Shell::update,
        Shell::view,
    )
    .title("Ferry")
    .theme(Shell::theme)
    .subscription(Shell::subscription)
    .window_size((1100.0, 720.0))
    .run()
}
