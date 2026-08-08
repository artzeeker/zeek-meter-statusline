//! Resolves every user-facing option: nerd-font toggle, theme, color depth
//! override, layout, bar width, percent precision, separator style, which
//! segments render (and in what order), and the pet's enable/animate flags.
//!
//! Precedence for every key is the same: **CLI flag > env var > config file >
//! default.** `resolve_nerd_font` is the original (v1) implementation of that
//! chain for the one setting v1 had; it's kept byte-for-byte so the
//! `CLAUDE_STATUSLINE_NERDFONT` env var and `{"nerd_font": bool}` config
//! files written by existing installs keep working unchanged. `Config::load`
//! generalizes the same chain to every other key.
//!
//! The config file (`~/.claude/zeek-meter-statusline.json`) has every field
//! optional and ignores unknown keys, matching the "never fail on unexpected
//! input" contract `input.rs` uses for the session JSON.

pub mod wizard;

use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::theme::ColorDepth;

pub const DEFAULT_SEGMENTS: [&str; 6] = ["model", "git", "ctx", "5h", "7d", "pet"];
pub const KNOWN_SEGMENTS: [&str; 16] = [
    "model", "git", "ctx", "5h", "7d", "pet", "cost", "duration", "lines", "effort", "mode", "pr",
    "worktree", "repo", "reset", "tokens",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    OneLine,
    TwoLine,
}

impl Layout {
    fn parse(s: &str) -> Option<Layout> {
        match s {
            "one-line" => Some(Layout::OneLine),
            "two-line" => Some(Layout::TwoLine),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Layout::OneLine => "one-line",
            Layout::TwoLine => "two-line",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Pipe,
    Dot,
    None,
}

impl Separator {
    fn parse(s: &str) -> Option<Separator> {
        match s {
            "pipe" => Some(Separator::Pipe),
            "dot" => Some(Separator::Dot),
            "none" => Some(Separator::None),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Separator::Pipe => "pipe",
            Separator::Dot => "dot",
            Separator::None => "none",
        }
    }
    pub fn glyph(&self) -> &'static str {
        match self {
            Separator::Pipe => "|",
            Separator::Dot => "\u{2022}",
            Separator::None => "",
        }
    }
}

fn parse_color_depth(s: &str) -> Option<ColorDepth> {
    match s {
        "truecolor" => Some(ColorDepth::TrueColor),
        "256" => Some(ColorDepth::Ansi256),
        "16" => Some(ColorDepth::Ansi16),
        "none" => Some(ColorDepth::None),
        _ => None, // "auto" or unrecognized: don't override, let detection run
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub theme: String,
    pub color_override: Option<ColorDepth>,
    pub layout: Layout,
    pub nerd_font: bool,
    pub bar_width: usize,
    pub percent_decimals: u8,
    pub separator: Separator,
    pub segments: Vec<String>,
    pub pet_enabled: bool,
    pub pet_animate: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            theme: "neon".to_string(),
            color_override: None,
            layout: Layout::OneLine,
            nerd_font: true,
            bar_width: crate::bar::BAR_WIDTH,
            percent_decimals: 0,
            separator: Separator::Pipe,
            segments: DEFAULT_SEGMENTS.iter().map(|s| s.to_string()).collect(),
            pet_enabled: true,
            pet_animate: true,
        }
    }
}

/// CLI-argv overrides recognized by the main (non-`init`/`config`/`uninstall`)
/// invocation. Only `nerd_font` is wired up as a real flag today (matching
/// v1); the others exist so `config --preview --theme X --layout Y` can share
/// this same layering code without a separate code path.
#[derive(Debug, Default, Clone)]
pub struct CliOverrides {
    pub nerd_font: Option<bool>,
    pub theme: Option<String>,
    pub layout: Option<Layout>,
    pub color: Option<ColorDepth>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    nerd_font: Option<bool>,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    bar_width: Option<usize>,
    #[serde(default)]
    percent_decimals: Option<u8>,
    #[serde(default)]
    separator: Option<String>,
    #[serde(default)]
    segments: Option<Vec<String>>,
    #[serde(default)]
    pet: Option<PetFileConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct PetFileConfig {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    animate: Option<bool>,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `CLAUDE_STATUSLINE_CLAUDE_DIR` overrides the `.claude` directory itself,
/// not just the binary's install location — both installers already set it
/// when relocating the binary (for tests, or a non-default Claude home), and
/// every other file this binary touches (`settings.json`, this tool's own
/// config file, the VS Code detector) needs to follow the same override or
/// an installer-side relocation silently only half-applies.
pub fn claude_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_STATUSLINE_CLAUDE_DIR") {
        return Some(PathBuf::from(dir));
    }
    home_dir().map(|h| h.join(".claude"))
}

pub fn config_path() -> Option<PathBuf> {
    claude_dir().map(|d| d.join("zeek-meter-statusline.json"))
}

fn load_file_config() -> FileConfig {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// `cli_flag` should be `Some(true)`/`Some(false)` if `--nerd-font`/
/// `--no-nerd-font` was passed on argv, `None` otherwise. Kept exactly as v1
/// implemented it: CLI > `CLAUDE_STATUSLINE_NERDFONT` env (`=0` disables) >
/// config file > default-on.
pub fn resolve_nerd_font(cli_flag: Option<bool>) -> bool {
    if let Some(v) = cli_flag {
        return v;
    }
    if let Ok(v) = std::env::var("CLAUDE_STATUSLINE_NERDFONT") {
        return v != "0";
    }
    if let Some(v) = load_file_config().nerd_font {
        return v;
    }
    true
}

impl Config {
    /// Layers CLI overrides, env vars, the config file, and defaults, in
    /// that precedence order, for every setting.
    pub fn load(cli: &CliOverrides) -> Config {
        let file = load_file_config();
        let defaults = Config::default();

        let nerd_font = resolve_nerd_font(cli.nerd_font);

        let theme = cli
            .theme
            .clone()
            .or_else(|| std::env::var("CLAUDE_STATUSLINE_THEME").ok())
            .or(file.theme)
            .unwrap_or(defaults.theme);

        let color_override = cli.color.or_else(|| {
            std::env::var("CLAUDE_STATUSLINE_COLOR")
                .ok()
                .and_then(|v| parse_color_depth(&v))
                .or_else(|| file.color.as_deref().and_then(parse_color_depth))
        });

        let layout = cli
            .layout
            .or_else(|| {
                std::env::var("CLAUDE_STATUSLINE_LAYOUT")
                    .ok()
                    .and_then(|v| Layout::parse(&v))
            })
            .or_else(|| file.layout.as_deref().and_then(Layout::parse))
            .unwrap_or(defaults.layout);

        let bar_width = file
            .bar_width
            .filter(|w| *w > 0)
            .unwrap_or(defaults.bar_width);
        let percent_decimals = file.percent_decimals.unwrap_or(defaults.percent_decimals);

        let separator = file
            .separator
            .as_deref()
            .and_then(Separator::parse)
            .unwrap_or(defaults.separator);

        let segments = file
            .segments
            .map(|s| {
                s.into_iter()
                    .filter(|name| KNOWN_SEGMENTS.contains(&name.as_str()))
                    .collect::<Vec<_>>()
            })
            .filter(|s| !s.is_empty())
            .unwrap_or(defaults.segments);

        let pet_enabled = file
            .pet
            .as_ref()
            .and_then(|p| p.enabled)
            .unwrap_or(defaults.pet_enabled);
        let pet_animate = file
            .pet
            .as_ref()
            .and_then(|p| p.animate)
            .unwrap_or(defaults.pet_animate);

        Config {
            theme,
            color_override,
            layout,
            nerd_font,
            bar_width,
            percent_decimals,
            separator,
            segments,
            pet_enabled,
            pet_animate,
        }
    }
}

/// Loads the config file as a raw JSON map (preserving unrecognized keys),
/// for `config --set` to mutate in place without clobbering fields this
/// binary doesn't know about yet.
pub fn load_raw_map() -> Map<String, Value> {
    let Some(path) = config_path() else {
        return Map::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Map::new();
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

pub fn write_raw_map(map: &Map<String, Value>) -> std::io::Result<PathBuf> {
    let path = config_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine home directory (HOME/USERPROFILE unset)",
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pretty = serde_json::to_string_pretty(&Value::Object(map.clone()))?;
    std::fs::write(&path, format!("{pretty}\n"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_flag_wins_over_everything() {
        assert!(!resolve_nerd_font(Some(false)));
        assert!(resolve_nerd_font(Some(true)));
    }

    #[test]
    fn default_config_matches_v1_behavior() {
        let cfg = Config::default();
        assert_eq!(cfg.theme, "neon");
        assert_eq!(cfg.bar_width, 10);
        assert!(cfg.nerd_font);
        assert_eq!(cfg.segments, vec!["model", "git", "ctx", "5h", "7d", "pet"]);
    }

    #[test]
    fn layout_parses_known_values_only() {
        assert_eq!(Layout::parse("one-line"), Some(Layout::OneLine));
        assert_eq!(Layout::parse("two-line"), Some(Layout::TwoLine));
        assert_eq!(Layout::parse("garbage"), None);
    }

    #[test]
    fn separator_glyphs() {
        assert_eq!(Separator::Pipe.glyph(), "|");
        assert_eq!(Separator::None.glyph(), "");
    }

    #[test]
    fn cli_overrides_take_precedence_in_load() {
        let cli = CliOverrides {
            nerd_font: Some(false),
            theme: Some("nord".to_string()),
            layout: Some(Layout::TwoLine),
            color: Some(ColorDepth::None),
        };
        let cfg = Config::load(&cli);
        assert!(!cfg.nerd_font);
        assert_eq!(cfg.theme, "nord");
        assert_eq!(cfg.layout, Layout::TwoLine);
        assert_eq!(cfg.color_override, Some(ColorDepth::None));
    }
}
