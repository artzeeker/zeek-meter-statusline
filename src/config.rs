//! Resolves the Nerd Font toggle. Precedence: CLI flag > env var > config
//! file > default.
//!
//! v1 flips the default from off to **on** — the whole point of the auto
//! font-install flow (see `README.md`) is that Nerd Font glyphs should just
//! work out of the box. `CLAUDE_STATUSLINE_NERDFONT` is kept as the env var
//! name for continuity with earlier releases, but its meaning inverts: `=0`
//! now *disables* nerd-font rendering (previously `=1` was required to
//! enable it).
//!
//! The config file (`~/.claude/zeek-meter-statusline.json`, `{"nerd_font":
//! bool}`) exists so the installer can persist a "no, I don't have a Nerd
//! Font" choice without requiring the user to edit their shell profile —
//! important on Windows, where setting a persistent env var is friction.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    nerd_font: Option<bool>,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn claude_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude"))
}

fn config_path() -> Option<PathBuf> {
    claude_dir().map(|d| d.join("zeek-meter-statusline.json"))
}

fn load_file_nerd_font() -> Option<bool> {
    let path = config_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let cfg: FileConfig = serde_json::from_str(&raw).ok()?;
    cfg.nerd_font
}

/// `cli_flag` should be `Some(true)`/`Some(false)` if `--nerd-font`/
/// `--no-nerd-font` was passed on argv, `None` otherwise.
pub fn resolve_nerd_font(cli_flag: Option<bool>) -> bool {
    if let Some(v) = cli_flag {
        return v;
    }
    if let Ok(v) = std::env::var("CLAUDE_STATUSLINE_NERDFONT") {
        return v != "0";
    }
    if let Some(v) = load_file_nerd_font() {
        return v;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_flag_wins_over_everything() {
        assert!(!resolve_nerd_font(Some(false)));
        assert!(resolve_nerd_font(Some(true)));
    }
}
