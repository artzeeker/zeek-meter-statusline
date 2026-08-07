//! Detects terminal configs that might need a nudge to render Nerd Font
//! glyphs, and edits the one that actually needs it.
//!
//! **Windows Terminal** (and most terminals using platform text shaping —
//! DirectWrite on Windows, CoreText on macOS) automatically falls back to any
//! installed font that has a glyph the primary font lacks. Since
//! `NerdFontsSymbolsOnly` is exactly that — icon glyphs, no letters — once
//! it's installed, Windows Terminal picks it up with no config change. So
//! this module deliberately does **not** touch `font.face`: setting it would
//! either be a no-op (fallback already covers it) or, if misapplied, replace
//! the user's primary font with one that has no alphanumeric glyphs at all.
//!
//! **VS Code**'s integrated terminal does not do this automatically — its
//! `terminal.integrated.fontFamily` setting must explicitly list the Nerd
//! Font as a fallback, or icons render as tofu. That edit is real and
//! useful, so this module performs it (with backup, and only when asked).

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SYMBOLS_FONT_NAME: &str = "Symbols Nerd Font Mono";

pub struct Detection {
    pub name: &'static str,
    pub config_path: Option<PathBuf>,
    pub needs_edit: bool,
    pub note: String,
}

pub fn detect_all() -> Vec<Detection> {
    vec![detect_windows_terminal(), detect_vscode()]
}

fn detect_windows_terminal() -> Detection {
    let path = windows_terminal_settings_path();
    let exists = path.as_ref().is_some_and(|p| p.exists());
    Detection {
        name: "Windows Terminal",
        config_path: path,
        needs_edit: false,
        note: if exists {
            "uses automatic font fallback (DirectWrite) — no config change needed once the font is installed".into()
        } else {
            "not detected".into()
        },
    }
}

fn windows_terminal_settings_path() -> Option<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")?;
    let packages_dir = PathBuf::from(local_app_data).join("Packages");
    let entries = std::fs::read_dir(&packages_dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("Microsoft.WindowsTerminal") {
            let candidate = entry.path().join("LocalState").join("settings.json");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn detect_vscode() -> Detection {
    let path = vscode_settings_path();
    let exists = path.as_ref().is_some_and(|p| p.exists());
    let needs_edit = exists && !already_has_nerd_font(path.as_ref().unwrap());
    Detection {
        name: "VS Code",
        config_path: path,
        needs_edit,
        note: if !exists {
            "not detected".into()
        } else if needs_edit {
            format!("terminal.integrated.fontFamily would gain '{SYMBOLS_FONT_NAME}' as a fallback")
        } else {
            "already configured".into()
        },
    }
}

fn vscode_settings_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    } else {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    }
}

fn already_has_nerd_font(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|raw| raw.contains("Nerd Font"))
}

fn default_primary_font() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "monospace"
    }
}

/// Appends the Nerd Font as a fallback to VS Code's terminal font family.
/// Backs up the file first. Returns `Ok(None)` if no change was needed.
pub fn configure_vscode(apply: bool) -> std::io::Result<Option<PathBuf>> {
    let Some(path) = vscode_settings_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    if already_has_nerd_font(&path) {
        return Ok(None);
    }
    if !apply {
        return Ok(Some(path));
    }

    let raw = std::fs::read_to_string(&path)?;
    let mut settings: Map<String, Value> = match serde_json::from_str(&raw) {
        Ok(Value::Object(m)) => m,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "VS Code settings.json is not a JSON object",
            ))
        }
    };

    let new_value = match settings.get("terminal.integrated.fontFamily") {
        Some(Value::String(existing)) if !existing.trim().is_empty() => {
            format!("{existing}, '{SYMBOLS_FONT_NAME}'")
        }
        _ => format!("{}, '{SYMBOLS_FONT_NAME}'", default_primary_font()),
    };

    let backup = backup_path(&path);
    std::fs::copy(&path, &backup)?;

    settings.insert(
        "terminal.integrated.fontFamily".into(),
        Value::String(new_value),
    );
    let pretty = serde_json::to_string_pretty(&Value::Object(settings))?;
    std::fs::write(&path, format!("{pretty}\n"))?;

    Ok(Some(path))
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

    #[test]
    fn already_has_nerd_font_detects_substring() {
        let dir = std::env::temp_dir().join(format!("zms-terminal-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"terminal.integrated.fontFamily": "Consolas, 'Symbols Nerd Font Mono'"}"#,
        )
        .unwrap();
        assert!(already_has_nerd_font(&path));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_not_flagged_as_needing_font() {
        let dir = std::env::temp_dir().join(format!("zms-terminal-test2-{}", std::process::id()));
        let path = dir.join("does-not-exist.json");
        assert!(!already_has_nerd_font(&path));
    }
}
