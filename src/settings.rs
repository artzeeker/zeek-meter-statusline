//! Merges (and unmerges) a `statusLine` entry in Claude Code's
//! `~/.claude/settings.json`.
//!
//! This is invoked by the installer as `zeek-meter-statusline init
//! --merge-settings` rather than done in bash, so the installer never needs
//! `jq` — the already-downloaded binary is the one dependency we can always
//! rely on. `uninstall` calls `unmerge_settings` the same way.
//!
//! Existing settings keys are preserved; only `statusLine` is added, removed,
//! or overwritten: whatever the previous `statusLine.command` was, `merge`
//! unconditionally replaces it with this binary's command, and `unmerge`
//! only removes it if it still points at this binary — never at whatever
//! other status line the user may have switched to since installing.

use serde_json::{Map, Value};
use std::io;
use std::path::{Path, PathBuf};

use crate::config::claude_dir;
use crate::fsutil;

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

/// Substring used to recognize "this is our statusLine entry" regardless of
/// exact path form (`~/...`, an absolute path, `.exe` or not).
const BINARY_MARKER: &str = "zeek-meter-statusline";

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

    write_settings(&path, &settings)?;
    Ok(path)
}

fn is_owned_by_us(settings: &Map<String, Value>) -> bool {
    matches!(
        settings.get("statusLine"),
        Some(Value::Object(sl)) if matches!(sl.get("command"), Some(Value::String(cmd)) if cmd.contains(BINARY_MARKER))
    )
}

/// Read-only check: does `settings.json` exist, and if so, does its
/// `statusLine` point at this binary? Used by `uninstall --dry-run` to
/// report what *would* happen without writing anything.
pub fn status_line_owned() -> io::Result<(PathBuf, bool)> {
    let path = settings_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine home directory (HOME/USERPROFILE unset)",
        )
    })?;
    if !path.exists() {
        return Ok((path, false));
    }
    let settings = load_existing(&path)?;
    Ok((path, is_owned_by_us(&settings)))
}

/// The inverse of `merge_settings`. Removes the `statusLine` key only if its
/// `.command` mentions this binary; if the user has since pointed
/// `statusLine` at something else, leaves it untouched and returns
/// `Ok((path, false))` so the caller can warn instead of silently doing
/// nothing. Every other settings key is preserved either way. Backs the file
/// up before writing, same as `merge_settings`'s caller-side contract for
/// invalid JSON.
pub fn unmerge_settings() -> io::Result<(PathBuf, bool)> {
    let path = settings_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot determine home directory (HOME/USERPROFILE unset)",
        )
    })?;

    if !path.exists() {
        return Ok((path, false));
    }

    let mut settings = load_existing(&path)?;

    if !is_owned_by_us(&settings) {
        return Ok((path, false));
    }

    fsutil::backup(&path)?;
    settings.remove("statusLine");
    write_settings(&path, &settings)?;
    Ok((path, true))
}

fn write_settings(path: &Path, settings: &Map<String, Value>) -> io::Result<()> {
    let pretty = serde_json::to_string_pretty(&Value::Object(settings.clone()))?;
    std::fs::write(path, format!("{pretty}\n"))
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
            let backup = fsutil::backup_path(path);
            std::fs::copy(path, &backup)?;
            eprintln!(
                "Warning: existing settings.json was invalid JSON. Backed it up to {}",
                backup.display()
            );
            Ok(Map::new())
        }
    }
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

    #[test]
    fn unmerge_removes_our_statusline_and_keeps_siblings() {
        let dir = scratch_dir("unmerge-owned");
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"model":"opusplan","statusLine":{"type":"command","command":"~/.claude/zeek-meter-statusline"}}"#,
        )
        .unwrap();

        let mut settings = load_existing(&path).unwrap();
        assert!(is_owned_by_us(&settings));
        settings.remove("statusLine");
        assert_eq!(settings.get("model").unwrap(), "opusplan");
        assert!(!settings.contains_key("statusLine"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unmerge_leaves_foreign_statusline_untouched() {
        let dir = scratch_dir("unmerge-foreign");
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"statusLine":{"type":"command","command":"~/.claude/some-other-tool"}}"#,
        )
        .unwrap();

        let settings = load_existing(&path).unwrap();
        assert!(!is_owned_by_us(&settings));
        fs::remove_dir_all(&dir).ok();
    }
}
