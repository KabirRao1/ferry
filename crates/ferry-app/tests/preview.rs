//! Preview: which bodies offer it, when the web view should show, and Open
//! in Browser (F22-AC1, F22-AC3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;

use bytes::Bytes;
use ferry_app::{
    App, BodyFormat, Effect, Finished, Header, Message, Phase, Received, Response, ResponseTab,
    TabId, extension, previewable, received,
};

fn response(content_type: &str, body: &'static [u8]) -> Response {
    Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![Header {
            name: "Content-Type".into(),
            value: content_type.into(),
        }],
        body: Bytes::from_static(body),
        url: "https://api.test/".into(),
        ..Response::default()
    }
}

/// Send from the active tab and receive `response`; returns the send id.
fn receive(app: &mut App, response: Response) -> (TabId, u64) {
    app.update(Message::Edit(ferry_app::Edit::Url(
        "https://api.test/".into(),
    )));
    let Some(Effect::Send { tab, id, .. }) = app.update(Message::SendPressed) else {
        panic!("no send");
    };
    app.update(Message::Finished {
        tab,
        id,
        result: Arc::new(Finished::Received(Box::new(received(response).unwrap()))),
    });
    (tab, id)
}

fn shown(app: &App) -> &Received {
    match &app.tab().phase {
        Phase::Received(r) => r,
        other => panic!("{other:?}"),
    }
}

const HTML: &[u8] = b"<!doctype html><h1>Hi</h1>";

#[test]
fn f22_ac1_previewable_types() {
    for (header, mime) in [
        ("text/html; charset=utf-8", "text/html"),
        ("TEXT/HTML", "text/html"),
        ("application/xhtml+xml", "application/xhtml+xml"),
        ("image/png", "image/png"),
        ("image/svg+xml", "image/svg+xml"),
        ("audio/mpeg", "audio/mpeg"),
        ("video/mp4", "video/mp4"),
        ("application/pdf", "application/pdf"),
        (" application/pdf ; x=1", "application/pdf"),
    ] {
        assert_eq!(previewable(header).as_deref(), Some(mime), "{header}");
    }
    for header in [
        "application/json",
        "text/plain",
        "text/htmlx",
        "image/",
        "imagery/png",
        "application/octet-stream",
        "",
    ] {
        assert_eq!(previewable(header), None, "{header}");
    }
}

#[test]
fn f22_ac3_extensions_come_from_a_fixed_list() {
    assert_eq!(extension("text/html"), "html");
    assert_eq!(extension("application/xhtml+xml"), "xhtml");
    assert_eq!(extension("application/pdf"), "pdf");
    assert_eq!(extension("image/jpeg"), "jpg");
    assert_eq!(extension("image/svg+xml"), "svg");
    assert_eq!(extension("audio/x-wav"), "wav");
    assert_eq!(extension("video/quicktime"), "mov");
    assert_eq!(extension("image/x-unknown"), "bin");
    assert_eq!(extension("../../evil"), "bin");
}

#[test]
fn f22_ac1_previewable_bodies_offer_preview() {
    let mut app = App::default();
    receive(&mut app, response("text/html", HTML));
    assert_eq!(
        shown(&app).formats(),
        vec![BodyFormat::Raw, BodyFormat::Hex, BodyFormat::Preview]
    );
    assert_eq!(
        shown(&app).format,
        BodyFormat::Raw,
        "Preview is not the default"
    );
    assert_eq!(BodyFormat::Preview.to_string(), "Preview");

    let mut app = App::default();
    receive(&mut app, response("application/json", b"{}"));
    assert!(!shown(&app).formats().contains(&BodyFormat::Preview));
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    assert_eq!(
        shown(&app).format,
        BodyFormat::Pretty,
        "not offered, not selectable"
    );
}

#[test]
fn f22_ac1_the_preview_follows_what_is_shown() {
    let mut app = App::default();
    assert_eq!(app.preview(), None, "no response");
    let (tab, id) = receive(&mut app, response("text/html", HTML));
    assert_eq!(app.preview(), None, "Raw is shown");
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    let preview = app.preview().unwrap();
    assert_eq!(preview.tab, tab);
    assert_eq!(preview.send, id);
    assert_eq!(preview.mime, "text/html");
    assert_eq!(preview.body.as_ref(), HTML);

    // Leaving it: another response tab, another workbench tab, a dialog.
    app.update(Message::ResponseTabSelected(ResponseTab::Headers));
    assert_eq!(app.preview(), None);
    app.update(Message::ResponseTabSelected(ResponseTab::Body));
    assert!(app.preview().is_some());
    app.update(Message::NewTab);
    assert_eq!(app.preview(), None);
    app.update(Message::ActivateTab(tab));
    assert!(app.preview().is_some());
    app.update(Message::OpenCookies);
    assert_eq!(app.preview(), None, "the web view would cover the dialog");
    app.update(Message::DialogCancel);
    assert!(app.preview().is_some());
    app.update(Message::BodyFormatSelected(BodyFormat::Hex));
    assert_eq!(app.preview(), None);
}

#[test]
fn f22_ac1_a_new_response_is_a_new_preview() {
    let mut app = App::default();
    receive(&mut app, response("text/html", HTML));
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    let first = app.preview().unwrap();
    receive(&mut app, response("text/html", b"<p>second</p>"));
    assert_eq!(app.preview(), None, "a new response starts in Raw");
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    let second = app.preview().unwrap();
    assert_ne!(first.send, second.send);
    assert_eq!(second.body.as_ref(), b"<p>second</p>");
}

#[test]
fn f22_ac1_preview_is_not_searchable() {
    let mut app = App::default();
    receive(&mut app, response("text/html", HTML));
    app.update(Message::FindOpen);
    assert!(shown(&app).find.is_some());
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    assert!(!shown(&app).searchable());
    assert!(
        shown(&app).find.is_none(),
        "switching to Preview closes the find bar"
    );
    app.update(Message::FindOpen);
    assert!(shown(&app).find.is_none());
}

#[test]
fn f22_ac3_without_a_web_view_nothing_is_previewed() {
    let mut app = App::default().without_preview();
    assert!(!app.preview_available());
    receive(&mut app, response("image/png", b"\x89PNG"));
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    assert_eq!(
        shown(&app).format,
        BodyFormat::Preview,
        "still offered, with Open in Browser"
    );
    assert_eq!(app.preview(), None);

    let mut app = App::default();
    receive(&mut app, response("image/png", b"\x89PNG"));
    app.update(Message::BodyFormatSelected(BodyFormat::Preview));
    assert!(app.preview().is_some());
    app.update(Message::PreviewFailed("WebView2 is not installed".into()));
    assert_eq!(app.preview(), None);
    assert!(!app.preview_available());
    assert_eq!(app.preview_error(), Some("WebView2 is not installed"));
}

#[test]
fn f22_ac3_open_in_browser_hands_over_the_body() {
    let mut app = App::default();
    assert!(app.update(Message::OpenInBrowser).is_none(), "no response");
    receive(&mut app, response("application/pdf", b"%PDF-1.7"));
    match app.update(Message::OpenInBrowser) {
        Some(Effect::OpenInBrowser { body, extension }) => {
            assert_eq!(body.as_ref(), b"%PDF-1.7");
            assert_eq!(extension, "pdf");
        }
        other => panic!("{other:?}"),
    }
    let mut app = App::default();
    receive(&mut app, response("application/json", b"{}"));
    assert!(
        app.update(Message::OpenInBrowser).is_none(),
        "not previewable"
    );
}
