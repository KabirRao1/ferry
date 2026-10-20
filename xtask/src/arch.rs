//! F00-AC2: enforce the crate dependency direction `ui -> app -> {core, http, store}`.

use serde_json::Value;

use crate::util::{self, Result};

/// Each workspace crate and the workspace crates it may depend on (any dependency kind).
pub const ALLOWED: &[(&str, &[&str])] = &[
    ("ferry-core", &[]),
    ("ferry-http", &["ferry-core"]),
    ("ferry-store", &["ferry-core"]),
    ("ferry-script", &["ferry-core"]),
    (
        "ferry-app",
        &["ferry-core", "ferry-http", "ferry-store", "ferry-script"],
    ),
    ("ferry-ui", &["ferry-app"]),
    ("xtask", &[]),
];

pub fn run() -> Result {
    let json = util::output(
        "cargo metadata",
        util::cargo().args(["metadata", "--format-version", "1", "--no-deps"]),
    )?;
    let metadata: Value =
        serde_json::from_str(&json).map_err(|e| format!("cargo metadata: {e}"))?;
    let violations = check(&metadata)?;
    if violations.is_empty() {
        println!("arch: ok ({} crates)", ALLOWED.len());
        Ok(())
    } else {
        for v in &violations {
            eprintln!("arch: {v}");
        }
        Err(format!(
            "{} dependency-direction violation(s)",
            violations.len()
        ))
    }
}

/// Violations of [`ALLOWED`] in `cargo metadata --no-deps` output.
pub fn check(metadata: &Value) -> Result<Vec<String>> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata: missing `packages`")?;
    let members: Vec<&str> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
    let mut violations = Vec::new();
    for package in packages {
        let name = package["name"].as_str().unwrap_or("<unnamed>");
        let Some((_, allowed)) = ALLOWED.iter().find(|(n, _)| *n == name) else {
            violations.push(format!(
                "`{name}` is not in the architecture table (xtask/src/arch.rs)"
            ));
            continue;
        };
        let deps = package["dependencies"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        for dep in deps {
            let dep_name = dep["name"].as_str().unwrap_or("");
            if members.contains(&dep_name) && !allowed.contains(&dep_name) {
                let kind = dep["kind"].as_str().unwrap_or("normal");
                violations.push(format!("`{name}` must not depend on `{dep_name}` ({kind})"));
            }
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pkg(name: &str, deps: &[(&str, Option<&str>)]) -> Value {
        json!({
            "name": name,
            "dependencies": deps.iter().map(|(d, kind)| json!({"name": d, "kind": kind})).collect::<Vec<_>>(),
        })
    }

    fn meta(pkgs: Vec<Value>) -> Value {
        json!({ "packages": pkgs })
    }

    #[test]
    fn f00_ac2_allowed_graph_passes() {
        let m = meta(vec![
            pkg("ferry-core", &[("memchr", None)]),
            pkg("ferry-app", &[("ferry-core", None), ("ferry-http", None)]),
            pkg("ferry-http", &[("ferry-core", None)]),
            pkg("ferry-ui", &[("ferry-app", None), ("iced", None)]),
        ]);
        assert_eq!(check(&m).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn f00_ac2_core_depending_on_workspace_crate_fails() {
        let m = meta(vec![
            pkg("ferry-core", &[]),
            pkg("ferry-store", &[]),
            pkg("ferry-core", &[("ferry-store", None)]),
        ]);
        assert_eq!(
            check(&m).unwrap(),
            vec!["`ferry-core` must not depend on `ferry-store` (normal)"]
        );
    }

    #[test]
    fn f00_ac2_ui_skipping_app_fails_even_for_dev_deps() {
        let m = meta(vec![
            pkg("ferry-http", &[]),
            pkg("ferry-ui", &[("ferry-http", Some("dev"))]),
        ]);
        assert_eq!(
            check(&m).unwrap(),
            vec!["`ferry-ui` must not depend on `ferry-http` (dev)"]
        );
    }

    #[test]
    fn f00_ac2_unknown_crate_fails() {
        let m = meta(vec![pkg("ferry-sync", &[])]);
        assert_eq!(
            check(&m).unwrap(),
            vec!["`ferry-sync` is not in the architecture table (xtask/src/arch.rs)"]
        );
    }

    #[test]
    fn f00_ac2_malformed_metadata_is_an_error() {
        assert!(check(&json!({})).is_err());
    }
}
