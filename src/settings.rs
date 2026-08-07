//! Merges a `statusLine` entry into Claude Code's `~/.claude/settings.json`.
//!
//! This is invoked by the installer as `zeek-meter-statusline init
//! --merge-settings` rather than done in bash, so the installer never needs
//! `jq` or `node` — the same reason the *old* Node-based installer could shell
//! out to `node -e` for this, but a bash-only installer for a Rust binary
//! can't assume any particular scripting runtime is present. The
//! already-downloaded binary is the one dependency we can always rely on.
//!
//! Existing settings keys are preserved; only `statusLine` is added or
//! overwritten. This also happens to be how migration off the old Node
//! script works: if `settings.json` already has `statusLine.command: "node
//! ~/.claude/statusline.js"`, it's unconditionally replaced with the new
//! binary's command.

use serde_json::{Map, Value};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::claude_dir;

pub fn settings_path() -> Option<PathBuf> {
    claude_dir().map(|d| d.join("settings.json"))
}

/// The command string written into `statusLine.command`. Uses `~` (expanded
/// by the shell Claude Code invokes the command through) rather than an
/// absolute path, so this same settings.json snippet is portable across
/// machines. The platform-correct executable suffix (`.exe` on Windows,
/// none elsewhere) is baked in at compile time via `env::consts::EXE_SUFFIX`,
/// matching whichever release asset this binary itself is.
fn status_line_command() -> String {
    format!(
        "~/.claude/zeek-meter-statusline{}",
        std::env::consts::EXE_SUFFIX
    )
}

pub fn merge_settings() -> io::Result<PathBuf> {
    let path = settings_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine home directory (HOME/USERPROFILE unset)",
        )
    })?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut settings = load_existing(&path)?;

    let mut status_line = Map::new();
    status_line.insert("type".into(), Value::String("command".into()));
    status_line.insert("command".into(), Value::String(status_line_command()));
    settings.insert("statusLine".into(), Value::Object(status_line));

    let pretty = serde_json::to_string_pretty(&Value::Object(settings))?;
    std::fs::write(&path, format!("{pretty}\n"))?;
    Ok(path)
}

fn load_existing(path: &Path) -> io::Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(m)) => Ok(m),
        _ => {
            let backup = backup_path(path);
            std::fs::copy(path, &backup)?;
            eprintln!(
                "Warning: existing settings.json was invalid JSON. Backed it up to {}",
                backup.display()
            );
            Ok(Map::new())
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json")
        .to_string();
    name.push_str(&format!(".bak-{millis}"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zeek-meter-statusline-settings-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn preserves_existing_keys() {
        let dir = scratch_dir("preserve");
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"model":"opusplan","theme":"dark"}"#).unwrap();

        let mut settings = load_existing(&path).unwrap();
        settings.insert("statusLine".into(), Value::String("placeholder".into()));

        assert_eq!(settings.get("model").unwrap(), "opusplan");
        assert_eq!(settings.get("theme").unwrap(), "dark");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_json_is_backed_up_not_lost() {
        let dir = scratch_dir("invalid");
        let path = dir.join("settings.json");
        fs::write(&path, "{not valid json").unwrap();

        let settings = load_existing(&path).unwrap();
        assert!(settings.is_empty());

        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
            .collect();
        assert_eq!(backups.len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_yields_empty_map() {
        let dir = scratch_dir("missing");
        let path = dir.join("settings.json");
        let settings = load_existing(&path).unwrap();
        assert!(settings.is_empty());
        fs::remove_dir_all(&dir).ok();
    }
}
