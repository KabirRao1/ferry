//! Visualize (F22-AC5, ADR-0046).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;
use std::time::Duration;

use ferry_app::visualize::{CSP, render};
use ferry_app::{
    App, BodyFormat, Bytes, Edit, Effect, Finished, Header, Message, Phase, Received, Response,
    ScriptPhase, ScriptReport, Timing, Visualizer, received,
};

fn visualizer(template: &str, data: &str) -> Visualizer {
    Visualizer {
        template: template.into(),
        data: data.into(),
    }
}

fn page(template: &str, data: &str) -> String {
    String::from_utf8(render(&visualizer(template, data)).unwrap().to_vec()).unwrap()
}

#[test]
fn f22_ac5_templates_render_with_their_data() {
    let html = page(
        "<table>{{#each rows}}<tr><td>{{@index}}</td><td>{{name}}</td></tr>{{/each}}</table>{{#if empty}}none{{/if}}",
        r#"{"rows": [{"name": "Ada"}, {"name": "<b>Bob</b>"}], "empty": false}"#,
    );
    assert!(
        html.ends_with(
            "<table><tr><td>0</td><td>Ada</td></tr><tr><td>1</td><td>&lt;b&gt;Bob&lt;/b&gt;</td></tr></table>"
        ),
        "{html}"
    );
    // Triple braces keep markup as it is.
    let raw = page("{{{markup}}}", r#"{"markup": "<i>x</i>"}"#);
    assert!(raw.ends_with("<i>x</i>"), "{raw}");
    // Missing values render empty.
    assert!(page("[{{nope}}]", "{}").ends_with("[]"));
}

#[test]
fn f22_ac5_the_page_has_the_policy_and_get_data() {
    let html = page(
        "<p>hi</p>",
        r#"{"a": "</script><script>alert(1)</script>"}"#,
    );
    assert!(html.contains(&format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">"
    )));
    assert!(html.contains("var pm = { getData: function (callback) { callback(null, "));
    // The data cannot close the script early.
    assert!(!html.contains("</script><script>alert"), "{html}");
    assert!(html.contains(r#"<\/script>"#));
    assert_eq!(
        CSP,
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:"
    );
}

#[test]
fn f22_ac5_bad_templates_say_why() {
    let error = render(&visualizer("{{#each rows}}", "{}")).unwrap_err();
    assert!(error.starts_with("Visualizer template error: "), "{error}");
    let error = render(&visualizer("x", "not json")).unwrap_err();
    assert!(
        error.starts_with("Visualizer template error: the data is not JSON"),
        "{error}"
    );
}

// --- in the app ------------------------------------------------------------------

fn response() -> Received {
    received(Response {
        status: 200,
        reason: "OK".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "application/json".into(),
        }],
        body: Bytes::from_static(br#"{"n": 1}"#),
        elapsed: Duration::from_millis(5),
        timing: Timing::default(),
        url: "https://api.test".into(),
        sent: Vec::new(),
        set_cookies: Vec::new(),
    })
    .unwrap()
}

fn app_with(visualizer: Option<Visualizer>) -> App {
    let mut app = App::default();
    app.update(Message::Edit(Edit::Url("https://api.test".into())));
    app.update(Message::Edit(Edit::Script(
        ScriptPhase::PostResponse,
        "pm.visualizer.set('x');".into(),
    )));
    app.update(Message::SendPressed);
    let tab = app.tab().id;
    app.update(Message::ScriptedFinished {
        tab,
        id: 1,
        result: Arc::new(Finished::Received(Box::new(response()))),
        report: Arc::new(ScriptReport {
            visualizer,
            ..ScriptReport::default()
        }),
    });
    app
}

fn shown(app: &App) -> &Received {
    match &app.tab().phase {
        Phase::Received(r) => r,
        other => panic!("{other:?}"),
    }
}

#[test]
fn f22_ac5_visualize_is_a_format_only_with_a_visualizer() {
    let app = app_with(None);
    assert!(!shown(&app).formats().contains(&BodyFormat::Visualize));
    let mut app = app_with(Some(visualizer("<p>{{n}}</p>", r#"{"n": 1}"#)));
    assert_eq!(
        shown(&app).formats(),
        [
            BodyFormat::Pretty,
            BodyFormat::Raw,
            BodyFormat::Hex,
            BodyFormat::Visualize
        ]
    );
    assert_eq!(BodyFormat::Visualize.to_string(), "Visualize");
    assert_eq!(app.preview(), None, "Pretty is still the default");
    app.update(Message::BodyFormatSelected(BodyFormat::Visualize));
    let target = app.preview().unwrap();
    assert!(target.scripts);
    assert_eq!(target.mime, "text/html");
    assert!(String::from_utf8_lossy(&target.body).ends_with("<p>1</p>"));
    // Search does not apply to a rendered page.
    assert!(!shown(&app).searchable());
    // Open in Browser opens the page as HTML.
    match app.update(Message::OpenInBrowser) {
        Some(Effect::OpenInBrowser { body, extension }) => {
            assert_eq!(extension, "html");
            assert_eq!(body, target.body);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn f22_ac5_a_broken_template_shows_its_error() {
    let mut app = app_with(Some(visualizer("{{#if}}", "{}")));
    app.update(Message::BodyFormatSelected(BodyFormat::Visualize));
    assert!(
        matches!(&shown(&app).visualized, Some(Err(e)) if e.starts_with("Visualizer template error"))
    );
    assert_eq!(app.preview(), None);
    assert!(app.update(Message::OpenInBrowser).is_none());
}
