use std::path::PathBuf;
use std::process::Command;

pub type Result<T = ()> = std::result::Result<T, String>;

/// Workspace root (parent of the `xtask` directory).
pub fn root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map_or(manifest.clone(), PathBuf::from)
}

pub fn cargo() -> Command {
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cmd.current_dir(root());
    cmd
}

/// Run `cmd`, streaming output; error if it exits non-zero.
pub fn run(name: &str, cmd: &mut Command) -> Result {
    let status = cmd
        .status()
        .map_err(|e| format!("{name}: could not start: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name}: failed ({status})"))
    }
}

/// Run `cmd` and capture stdout as UTF-8.
pub fn output(name: &str, cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .map_err(|e| format!("{name}: could not start: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{name}: failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("{name}: non-UTF-8 output: {e}"))
}
