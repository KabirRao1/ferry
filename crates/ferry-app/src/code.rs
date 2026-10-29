//! The Code panel: the active request as code (F19-AC1).

use ferry_core::auth::AuthKind;
use ferry_core::codegen::{Language, snippet};
use ferry_core::variables::Resolver;

use crate::ambient::FixedAmbient;
use crate::app::{App, vars_for};
use crate::doc::Doc;

// CONTRACT
/// Shown in the Code panel for tabs that are not requests.
pub const NOT_A_REQUEST: &str = "Code is available for requests.";

impl App {
    // CONTRACT
    /// Whether the Code panel is open.
    #[must_use]
    pub fn code_open(&self) -> bool {
        self.code_open
    }

    // CONTRACT
    /// The Code panel's language, remembered with the session.
    #[must_use]
    pub fn code_language(&self) -> Language {
        self.code_language
    }

    // CONTRACT
    /// The active request as code: inherited auth filled in and variables
    /// resolved as for sending, unresolved ones left as `{{name}}`.
    ///
    /// # Errors
    /// Why the request cannot be built, or [`NOT_A_REQUEST`].
    pub fn code(&self) -> Result<String, String> {
        let tab = self.tab();
        if tab.doc != Doc::Request {
            return Err(NOT_A_REQUEST.to_owned());
        }
        let mut draft = tab.draft.clone();
        if draft.auth.kind == AuthKind::Inherit {
            draft.auth = self
                .inherited_auth(tab)
                .map(|(auth, _)| auth)
                .unwrap_or_default();
        }
        let vars = vars_for(
            &self.workspace,
            self.environment.as_deref(),
            tab.link.as_deref(),
        );
        // A fixed clock and seed: the panel shows the same dynamic values
        // on every redraw.
        let mut ambient = FixedAmbient::with_seed(self.ambient.now(), 0, 0);
        let mut resolver = Resolver::new(&vars, &mut ambient);
        let resolved = draft.resolve(&mut resolver);
        let built = resolved.build().map_err(|e| e.to_string())?;
        Ok(self.code_language.generate(&snippet(&built, &resolved)))
    }
}
