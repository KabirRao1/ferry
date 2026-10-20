//! Environment and globals files, with their git-ignored local layer
//! (F08-AC10, ADR-0011, ADR-0040).

use serde::{Deserialize, Serialize};

use crate::collection::{FormatError, toml_error};
use crate::variables::{Origin, Variable};

// CONTRACT
/// The contents of an environment or of `globals.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VariablesFile {
    /// The environment's name; `None` for globals.
    pub name: Option<String>,
    pub variables: Vec<Variable>,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize)]
struct SharedRepr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    variables: Vec<VarRepr>,
}

#[derive(Serialize, Deserialize)]
struct VarRepr {
    key: String,
    #[serde(default)]
    value: String,
    #[serde(default = "yes", skip_serializing_if = "crate::is_true")]
    enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    secret: bool,
}

#[derive(Serialize, Deserialize)]
struct LocalRepr {
    #[serde(default)]
    variables: Vec<LocalVar>,
}

#[derive(Serialize, Deserialize)]
struct LocalVar {
    key: String,
    #[serde(default)]
    value: String,
}

// CONTRACT
/// Parse a shared file and, if it exists, its local file. Local values
/// replace shared ones by key (recorded as `Origin::Overridden`), and keys
/// only in the local file are appended (`Origin::LocalOnly`).
///
/// # Errors
/// [`FormatError`] if either file is not valid.
pub fn parse(shared: &str, local: Option<&str>) -> Result<VariablesFile, FormatError> {
    let shared: SharedRepr = toml::from_str(shared).map_err(|e| toml_error(shared, &e))?;
    let local = match local {
        Some(text) => parse_local(text)?,
        None => Vec::new(),
    };
    let mut variables: Vec<Variable> = shared
        .variables
        .into_iter()
        .map(|v| Variable {
            key: v.key,
            value: v.value,
            enabled: v.enabled,
            secret: v.secret,
            origin: Origin::Shared,
        })
        .collect();
    apply_local(&mut variables, local);
    Ok(VariablesFile {
        name: shared.name,
        variables,
    })
}

fn parse_local(text: &str) -> Result<Vec<LocalVar>, FormatError> {
    Ok(toml::from_str::<LocalRepr>(text)
        .map_err(|e| toml_error(text, &e))?
        .variables)
}

/// Local values replace shared ones by key (`Origin::Overridden`); keys
/// only in the local file are appended (`Origin::LocalOnly`).
fn apply_local(variables: &mut Vec<Variable>, local: Vec<LocalVar>) {
    for l in local {
        match variables
            .iter_mut()
            .find(|v| v.key == l.key && v.origin == Origin::Shared)
        {
            Some(v) => {
                let shared = std::mem::replace(&mut v.value, l.value);
                v.origin = Origin::Overridden { shared };
            }
            None => variables.push(Variable {
                key: l.key,
                value: l.value,
                origin: Origin::LocalOnly,
                ..Variable::default()
            }),
        }
    }
}

/// Merge a local file's text into variables read from a shared file.
pub(crate) fn merge_local(variables: &mut Vec<Variable>, local: &str) -> Result<(), FormatError> {
    apply_local(variables, parse_local(local)?);
    Ok(())
}

/// The value the shared file keeps for `v`, or `None` if `v` is only in
/// the local file.
fn shared_value(v: &Variable) -> Option<String> {
    match (&v.origin, v.secret) {
        (Origin::LocalOnly, _) => None,
        (_, true) => Some(String::new()),
        (Origin::Overridden { shared }, false) => Some(shared.clone()),
        (Origin::Shared, false) => Some(v.value.clone()),
    }
}

/// The variables as the shared file stores them.
pub(crate) fn shared_variables(variables: &[Variable]) -> Vec<Variable> {
    variables
        .iter()
        .filter_map(|v| {
            shared_value(v).map(|value| Variable {
                value,
                origin: Origin::Shared,
                ..v.clone()
            })
        })
        .collect()
}

/// The local file's text for `variables`, if any value belongs there.
pub(crate) fn local_text(variables: &[Variable]) -> Option<String> {
    let local: Vec<LocalVar> = variables
        .iter()
        .filter(|v| v.secret || v.origin != Origin::Shared)
        .map(|v| LocalVar {
            key: v.key.clone(),
            value: v.value.clone(),
        })
        .collect();
    (!local.is_empty()).then(|| to_toml(&LocalRepr { variables: local }))
}

// CONTRACT
/// Set `key` as a script does (F15-AC5, ADR-0045): the first variable with
/// that key gets `value` in the local layer, or a local-only variable is
/// added. The shared file's value is kept for [`unset_local`].
pub fn set_local(variables: &mut Vec<Variable>, key: &str, value: &str) {
    match variables.iter_mut().find(|v| v.key == key) {
        Some(v) => {
            if v.origin == Origin::Shared {
                let shared = std::mem::replace(&mut v.value, value.to_owned());
                v.origin = Origin::Overridden { shared };
            } else {
                value.clone_into(&mut v.value);
            }
        }
        None => variables.push(Variable {
            key: key.to_owned(),
            value: value.to_owned(),
            origin: Origin::LocalOnly,
            ..Variable::default()
        }),
    }
}

// CONTRACT
/// Remove `key`'s local value as a script's `unset` does (F15-AC5): an
/// overridden variable gets its shared value back and a local-only one
/// goes away.
pub fn unset_local(variables: &mut Vec<Variable>, key: &str) {
    let Some(i) = variables.iter().position(|v| v.key == key) else {
        return;
    };
    match std::mem::take(&mut variables[i].origin) {
        Origin::Shared => {}
        Origin::Overridden { shared } => variables[i].value = shared,
        Origin::LocalOnly => {
            variables.remove(i);
        }
    }
}

// CONTRACT
/// The shared file's text and, when any value belongs in it, the local
/// file's text. Secret values and values that came from the local file go
/// to the local file; the shared file keeps a secret's key with an empty
/// value, an overridden variable's shared value, and nothing of a local-only
/// variable.
#[must_use]
pub fn write(file: &VariablesFile) -> (String, Option<String>) {
    let shared = shared_variables(&file.variables)
        .into_iter()
        .map(|v| VarRepr {
            key: v.key,
            value: v.value,
            enabled: v.enabled,
            secret: v.secret,
        })
        .collect();
    let shared_text = to_toml(&SharedRepr {
        name: file.name.clone(),
        variables: shared,
    });
    (shared_text, local_text(&file.variables))
}

/// Serialize a value whose fields are all TOML-representable.
pub(crate) fn to_toml<T: Serialize>(value: &T) -> String {
    // Strings, booleans, integers, tables and arrays of tables always
    // serialize; an error would mean a non-TOML type slipped into a repr.
    toml::to_string(value).unwrap_or_default()
}

// CONTRACT
/// The local file's path for a shared file's path: `dev.toml` becomes
/// `dev.local.toml`.
#[must_use]
pub fn local_path(shared: &str) -> String {
    match shared.strip_suffix(".toml") {
        Some(stem) => format!("{stem}.local.toml"),
        None => format!("{shared}.local.toml"),
    }
}
