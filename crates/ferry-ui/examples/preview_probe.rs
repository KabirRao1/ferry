//! Opens Ferry with an HTML response shown in Preview, for checking the web
//! view by eye (F22-AC1, F22-AC2): the page should render, its script must
//! not run, and its link and `window.open` must go nowhere.
//!
//! `cargo run -p ferry-ui --example preview_probe [html|svg|redirect|leave|pdf <file>]`
//!
//! `leave` switches to Raw after 3 s: the web view must be gone (F22-AC1).

use std::sync::Arc;
use std::time::Duration;

use ferry_app::{
    App, BodyFormat, Bytes, Client, Config, Edit, Effect, Finished, Header, Message, Response,
    received,
};
use ferry_ui::screen::TEXT;
use ferry_ui::shell::{self, Shell};
use iced::{Subscription, Task};

struct Probe {
    shell: Shell,
    leave: bool,
}

#[derive(Debug, Clone)]
enum ProbeMessage {
    Shell(shell::Message),
    Leave,
}

fn update(probe: &mut Probe, message: ProbeMessage) -> Task<ProbeMessage> {
    let message = match message {
        ProbeMessage::Shell(m) => m,
        ProbeMessage::Leave => {
            probe.leave = false;
            shell::Message::App(Message::BodyFormatSelected(BodyFormat::Raw))
        }
    };
    probe.shell.update(message).map(ProbeMessage::Shell)
}

fn view(probe: &Probe) -> iced::Element<'_, ProbeMessage> {
    probe.shell.view().map(ProbeMessage::Shell)
}

fn subscription(probe: &Probe) -> Subscription<ProbeMessage> {
    let leave = if probe.leave {
        iced::time::every(Duration::from_secs(3)).map(|_| ProbeMessage::Leave)
    } else {
        Subscription::none()
    };
    Subscription::batch([probe.shell.subscription().map(ProbeMessage::Shell), leave])
}

const HTML: &str = r##"<!doctype html>
<html><head><title>Preview probe</title>
<style>body{font-family:sans-serif;margin:24px} .ok{color:#0a7}</style></head>
<body>
<h1>Ferry preview</h1>
<p class="ok">This HTML is rendered by the system web view.</p>
<p id="js">Scripts are off: this line stays.</p>
<script>document.getElementById('js').textContent = 'SCRIPT RAN';</script>
<p><a href="https://example.com/">A link that must not navigate</a></p>
<svg width="120" height="60"><rect width="120" height="60" rx="8" fill="#2dd4bf"/></svg>
</body></html>"##;

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200"><circle cx="100" cy="100" r="80" fill="#2dd4bf"/></svg>"##;

const REDIRECT: &str = r#"<!doctype html><meta http-equiv="refresh" content="0;url=https://example.com/">
<body style="font-family:sans-serif;margin:24px"><h1>Still here</h1><p>The redirect was refused.</p></body>"#;

fn main() -> iced::Result {
    let kind = std::env::args().nth(1).unwrap_or_else(|| "html".into());
    let (mime, body): (&str, Bytes) = match kind.as_str() {
        "svg" => ("image/svg+xml", Bytes::from_static(SVG.as_bytes())),
        // Navigates away without scripts; the web view must refuse.
        "redirect" => ("text/html", Bytes::from_static(REDIRECT.as_bytes())),
        "pdf" => (
            "application/pdf",
            Bytes::from(
                std::fs::read(std::env::args().nth(2).unwrap_or_default()).unwrap_or_default(),
            ),
        ),
        _ => (
            "text/html; charset=utf-8",
            Bytes::from_static(HTML.as_bytes()),
        ),
    };
    let leave = kind == "leave";
    let client = match Client::new(Config::default()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("could not start the HTTP client: {error}");
            std::process::exit(1);
        }
    };
    iced::application(
        move || {
            let mut app = App::default();
            app.update(Message::Edit(Edit::Url("https://preview.test/".into())));
            if let Some(Effect::Send { tab, id, .. }) = app.update(Message::SendPressed) {
                let response = Response {
                    status: 200,
                    reason: "OK".into(),
                    headers: vec![Header {
                        name: "content-type".into(),
                        value: mime.into(),
                    }],
                    body: body.clone(),
                    elapsed: Duration::from_millis(5),
                    url: "https://preview.test/".into(),
                    ..Response::default()
                };
                let result = match received(response) {
                    Ok(r) => Finished::Received(Box::new(r)),
                    Err(error) => Finished::Failed(error),
                };
                app.update(Message::Finished {
                    tab,
                    id,
                    result: Arc::new(result),
                });
            }
            app.update(Message::BodyFormatSelected(BodyFormat::Preview));
            Probe {
                shell: Shell::new(app, client.clone(), None),
                leave,
            }
        },
        update,
        view,
    )
    .title("Ferry preview probe")
    .theme(|probe: &Probe| probe.shell.theme())
    .subscription(subscription)
    .settings(iced::Settings {
        default_text_size: TEXT.into(),
        ..iced::Settings::default()
    })
    .window_size((1280.0, 800.0))
    .run()
}
