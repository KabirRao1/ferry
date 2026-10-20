//! Visualize: a Handlebars template from `pm.visualizer.set`, rendered into
//! a page whose inline scripts may run but which cannot reach the network
//! (F22-AC5, ADR-0046).

use bytes::Bytes;
use ferry_script::Visualizer;

// CONTRACT
/// The page's Content Security Policy: inline scripts and styles, images
/// and fonts from `data:` URLs, and nothing else.
pub const CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:";

// CONTRACT
/// The start of the message for a template that does not render.
pub const TEMPLATE_ERROR: &str = "Visualizer template error";

// CONTRACT
/// The page for `visualizer`: the policy, `pm.getData`, then the rendered
/// template.
///
/// # Errors
/// `Visualizer template error: <reason>` when the template does not parse
/// or render, or the data is not JSON.
pub fn render(visualizer: &Visualizer) -> Result<Bytes, String> {
    let data: serde_json::Value = serde_json::from_str(&visualizer.data)
        .map_err(|e| format!("{TEMPLATE_ERROR}: the data is not JSON: {e}"))?;
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.set_strict_mode(false);
    let body = handlebars
        .render_template(&visualizer.template, &data)
        .map_err(|e| format!("{TEMPLATE_ERROR}: {}", reason(&e)))?;
    // `</` cannot end the script early once escaped.
    let json = data.to_string().replace("</", "<\\/");
    let page = format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\n<script>var pm = {{ getData: function (callback) {{ callback(null, {json}); }} }};</script>\n{body}"
    );
    Ok(Bytes::from(page))
}

/// A render error's reason without the crate's prefixes.
fn reason(error: &handlebars::RenderError) -> String {
    let text = error.to_string();
    text.lines().next().unwrap_or_default().trim().to_owned()
}
