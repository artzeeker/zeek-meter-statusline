//! Nerd Font removal for `uninstall --remove-font` — the inverse of the
//! per-platform font-install steps in `install.sh`/`install.ps1`. Platform
//! detection and destination directories match those scripts exactly so a
//! font this project installed is the one this module finds and removes.

use std::path::PathBuf;

/// Where `NerdFontsSymbolsOnly` files land per platform, matching
/// `install_font_windows`/`_macos`/`_linux` in `install.sh`.
pub fn font_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(|d| {
            PathBuf::from(d)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts")
        })
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|d| PathBuf::from(d).join("Library").join("Fonts"))
    } else {
        std::env::var_os("HOME")
            .map(|d| PathBuf::from(d).join(".local").join("share").join("fonts"))
    }
}

fn is_symbols_font_file(name: &str) -> bool {
    name.to_lowercase().contains("symbols nerd font")
}

/// Returns the file names that would be removed, without touching anything
/// — used by `uninstall --dry-run`.
pub fn font_files_present() -> Vec<String> {
    let Some(dir) = font_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            is_symbols_font_file(&name).then_some(name)
        })
        .collect()
}

/// Removes the installed `NerdFontsSymbolsOnly` files (and, on Windows,
/// their `HKCU` font-registry entries; on Linux, refreshes the font cache
/// afterward). Returns the file names actually removed.
pub fn remove_font() -> std::io::Result<Vec<String>> {
    let Some(dir) = font_dir() else {
        return Ok(Vec::new());
    };
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut removed = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_symbols_font_file(&name) {
            continue;
        }
        if std::fs::remove_file(entry.path()).is_ok() {
            if cfg!(target_os = "windows") {
                unregister_windows_font(&name);
            }
            removed.push(name);
        }
    }

    if cfg!(target_os = "linux") && !removed.is_empty() {
        let _ = std::process::Command::new("fc-cache").arg("-f").output();
    }

    Ok(removed)
}

/// `reg.exe` is shelled out to rather than adding a `winreg` dependency —
/// same trade-off `install.ps1`/`install.sh` already make for the
/// registration side of this.
fn unregister_windows_font(file_name: &str) {
    let display_name = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let value_name = format!("{display_name} (TrueType)");
    let _ = std::process::Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
            "/v",
            &value_name,
            "/f",
        ])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_font_file_matching_is_case_insensitive() {
        // Matches the same "(?i)symbols nerd font" convention both
        // installers already use to detect an existing install.
        assert!(is_symbols_font_file("Symbols Nerd Font Mono.ttf"));
        assert!(is_symbols_font_file("SYMBOLS NERD FONT MONO REGULAR.TTF"));
        assert!(!is_symbols_font_file("Consolas.ttf"));
    }
}
