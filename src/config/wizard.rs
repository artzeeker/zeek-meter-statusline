//! The `config` subcommand: an interactive setup wizard plus `--show`,
//! `--set KEY=VALUE`, and `--preview` for scripting/testing without a live
//! Claude Code session.
//!
//! All writes go through `config::write_raw_map`, which round-trips the
//! config file as a raw JSON map — so `--set` never clobbers a key this
//! binary doesn't know about yet, matching the "preserve unknown keys"
//! contract `settings::merge_settings` already uses for `settings.json`.

use serde_json::{Map, Value};
use std::io::{self, BufRead, Write};

use crate::config::{self, CliOverrides, Config, Layout, DEFAULT_SEGMENTS, KNOWN_SEGMENTS};
use crate::theme::THEME_NAMES;

pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--show") {
        return show();
    }
    if let Some(kv) = args.iter().find_map(|a| a.strip_prefix("--set=")) {
        return set(kv);
    }
    if let Some(pos) = args.iter().position(|a| a == "--set") {
        return match args.get(pos + 1) {
            Some(kv) => set(kv),
            None => {
                eprintln!("usage: zeek-meter-statusline config --set KEY=VALUE");
                1
            }
        };
    }
    if args.iter().any(|a| a == "--preview") {
        return preview_cmd(args);
    }
    interactive()
}

// ---------------------------------------------------------------------------
// Interactive wizard
// ---------------------------------------------------------------------------

fn interactive() -> i32 {
    println!("zeek-meter-statusline config wizard \u{2014} press Enter to accept the default in [brackets].\n");

    let theme = ask_choice("Pick a theme:", &THEME_NAMES, "neon");
    println!(
        "\n{}\n",
        render_preview_cfg(&preview_config(
            &theme,
            Layout::OneLine,
            true,
            true,
            &default_segments()
        ))
    );

    let layout_name = ask_choice("Layout:", &["one-line", "two-line"], "one-line");
    let layout = Layout::parse(&layout_name).unwrap_or(Layout::OneLine);

    let nerd_font = ask_yes_no("Enable Nerd Font icons?", true);

    let extra_known: Vec<&str> = KNOWN_SEGMENTS
        .iter()
        .filter(|s| !DEFAULT_SEGMENTS.contains(s))
        .copied()
        .collect();
    println!("\nExtra segments available: {}", extra_known.join(", "));
    let extra_raw = ask_line("Add any? (comma-separated, blank for none) > ");
    let mut segments = default_segments();
    for name in extra_raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if KNOWN_SEGMENTS.contains(&name) {
            if !segments.contains(&name.to_string()) {
                segments.push(name.to_string());
            }
        } else {
            println!("  (skipping unknown segment '{name}')");
        }
    }

    let pet_animate = ask_yes_no("Animate the pet?", true);

    let mut map = Map::new();
    map.insert("theme".into(), Value::String(theme.clone()));
    map.insert("layout".into(), Value::String(layout.as_str().to_string()));
    map.insert("nerd_font".into(), Value::Bool(nerd_font));
    map.insert(
        "segments".into(),
        Value::Array(segments.iter().map(|s| Value::String(s.clone())).collect()),
    );
    let mut pet_map = Map::new();
    pet_map.insert("enabled".into(), Value::Bool(true));
    pet_map.insert("animate".into(), Value::Bool(pet_animate));
    map.insert("pet".into(), Value::Object(pet_map));

    match config::write_raw_map(&map) {
        Ok(path) => {
            println!("\nSaved {}", path.display());
            println!(
                "\n{}",
                render_preview_cfg(&preview_config(
                    &theme,
                    layout,
                    nerd_font,
                    pet_animate,
                    &segments
                ))
            );
            0
        }
        Err(e) => {
            eprintln!("Error writing config: {e}");
            1
        }
    }
}

fn default_segments() -> Vec<String> {
    DEFAULT_SEGMENTS.iter().map(|s| s.to_string()).collect()
}

fn ask_line(prompt_text: &str) -> String {
    print!("{prompt_text}");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().lock().read_line(&mut line);
    line.trim().to_string()
}

fn ask_choice(question: &str, options: &[&str], default: &str) -> String {
    println!("{question}");
    for (i, o) in options.iter().enumerate() {
        let marker = if *o == default { " (default)" } else { "" };
        println!("  {}. {o}{marker}", i + 1);
    }
    let raw = ask_line(&format!("> [{default}] "));
    if raw.is_empty() {
        return default.to_string();
    }
    if let Ok(n) = raw.parse::<usize>() {
        if n >= 1 && n <= options.len() {
            return options[n - 1].to_string();
        }
    }
    if options.contains(&raw.as_str()) {
        return raw;
    }
    println!("  (unrecognized, using default '{default}')");
    default.to_string()
}

fn ask_yes_no(question: &str, default_yes: bool) -> bool {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    let raw = ask_line(&format!("{question} {suffix} ")).to_lowercase();
    if raw.is_empty() {
        return default_yes;
    }
    raw == "y" || raw == "yes"
}

// ---------------------------------------------------------------------------
// --show
// ---------------------------------------------------------------------------

fn show() -> i32 {
    let cfg = Config::load(&CliOverrides::default());
    let file_map = config::load_raw_map();

    println!("Resolved configuration:");
    print_field(
        "theme",
        &cfg.theme,
        "CLAUDE_STATUSLINE_THEME",
        file_map.contains_key("theme"),
    );
    print_field(
        "layout",
        cfg.layout.as_str(),
        "CLAUDE_STATUSLINE_LAYOUT",
        file_map.contains_key("layout"),
    );
    print_field(
        "nerd_font",
        &cfg.nerd_font.to_string(),
        "CLAUDE_STATUSLINE_NERDFONT",
        file_map.contains_key("nerd_font"),
    );
    print_field(
        "color",
        &cfg.color_override
            .map(|_| "override".to_string())
            .unwrap_or_else(|| "auto".to_string()),
        "CLAUDE_STATUSLINE_COLOR",
        file_map.contains_key("color"),
    );
    print_field(
        "bar_width",
        &cfg.bar_width.to_string(),
        "",
        file_map.contains_key("bar_width"),
    );
    print_field(
        "percent_decimals",
        &cfg.percent_decimals.to_string(),
        "",
        file_map.contains_key("percent_decimals"),
    );
    print_field(
        "separator",
        cfg.separator.as_str(),
        "",
        file_map.contains_key("separator"),
    );
    println!(
        "  segments = {}   ({})",
        cfg.segments.join(","),
        if file_map.contains_key("segments") {
            "file"
        } else {
            "default"
        }
    );
    println!("  pet.enabled = {}", cfg.pet_enabled);
    println!("  pet.animate = {}", cfg.pet_animate);

    if let Some(p) = config::config_path() {
        println!("\nConfig file: {}", p.display());
    }
    0
}

fn print_field(name: &str, value: &str, env_name: &str, file_has: bool) {
    let source = if !env_name.is_empty() && std::env::var(env_name).is_ok() {
        "env"
    } else if file_has {
        "file"
    } else {
        "default"
    };
    println!("  {name} = {value}   ({source})");
}

// ---------------------------------------------------------------------------
// --set KEY=VALUE
// ---------------------------------------------------------------------------

fn set(kv: &str) -> i32 {
    let Some((key, value)) = kv.split_once('=') else {
        eprintln!("expected KEY=VALUE, got '{kv}'");
        return 1;
    };
    let mut map = config::load_raw_map();
    if let Err(e) = apply_set(&mut map, key.trim(), value.trim()) {
        eprintln!("{e}");
        return 1;
    }
    match config::write_raw_map(&map) {
        Ok(path) => {
            println!("Updated {}", path.display());
            0
        }
        Err(e) => {
            eprintln!("Error writing config: {e}");
            1
        }
    }
}

fn apply_set(map: &mut Map<String, Value>, key: &str, value: &str) -> Result<(), String> {
    match key {
        "theme" => {
            validate_one_of(value, &THEME_NAMES)?;
            map.insert("theme".into(), Value::String(value.to_string()));
        }
        "color" => {
            validate_one_of(value, &["auto", "truecolor", "256", "16", "none"])?;
            map.insert("color".into(), Value::String(value.to_string()));
        }
        "layout" => {
            validate_one_of(value, &["one-line", "two-line"])?;
            map.insert("layout".into(), Value::String(value.to_string()));
        }
        "separator" => {
            validate_one_of(value, &["pipe", "dot", "none"])?;
            map.insert("separator".into(), Value::String(value.to_string()));
        }
        "nerd_font" => {
            map.insert("nerd_font".into(), Value::Bool(parse_bool(value)?));
        }
        "bar_width" => {
            let n: usize = value
                .parse()
                .map_err(|_| format!("bar_width must be a positive integer, got '{value}'"))?;
            if n == 0 {
                return Err("bar_width must be greater than 0".to_string());
            }
            map.insert("bar_width".into(), Value::Number(n.into()));
        }
        "percent_decimals" => {
            let n: u8 = value
                .parse()
                .map_err(|_| format!("percent_decimals must be 0-255, got '{value}'"))?;
            map.insert("percent_decimals".into(), Value::Number(n.into()));
        }
        "segments" => {
            let names: Vec<&str> = value
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            for n in &names {
                if !KNOWN_SEGMENTS.contains(n) {
                    return Err(format!(
                        "unknown segment '{n}'; known: {}",
                        KNOWN_SEGMENTS.join(", ")
                    ));
                }
            }
            map.insert(
                "segments".into(),
                Value::Array(names.iter().map(|s| Value::String(s.to_string())).collect()),
            );
        }
        "pet.enabled" | "pet.animate" => {
            let b = parse_bool(value)?;
            let field = key.split_once('.').unwrap().1;
            let pet_entry = map
                .entry("pet".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(pet_map) = pet_entry {
                pet_map.insert(field.to_string(), Value::Bool(b));
            } else {
                let mut pet_map = Map::new();
                pet_map.insert(field.to_string(), Value::Bool(b));
                *pet_entry = Value::Object(pet_map);
            }
        }
        other => return Err(format!("unknown config key '{other}'")),
    }
    Ok(())
}

fn validate_one_of(value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("'{value}' is not one of: {}", allowed.join(", ")))
    }
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" | "1" | "yes" | "y" => Ok(true),
        "false" | "0" | "no" | "n" => Ok(false),
        other => Err(format!("expected true/false, got '{other}'")),
    }
}

// ---------------------------------------------------------------------------
// --preview
// ---------------------------------------------------------------------------

fn preview_cmd(args: &[String]) -> i32 {
    let mut cli = CliOverrides::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--theme" => {
                if let Some(v) = args.get(i + 1) {
                    cli.theme = Some(v.clone());
                    i += 1;
                }
            }
            "--layout" => {
                if let Some(v) = args.get(i + 1) {
                    cli.layout = Layout::parse(v);
                    i += 1;
                }
            }
            "--nerd-font" => cli.nerd_font = Some(true),
            "--no-nerd-font" => cli.nerd_font = Some(false),
            _ => {}
        }
        i += 1;
    }
    let cfg = Config::load(&cli);
    println!("{}", render_preview_cfg(&cfg));
    0
}

/// Builds a `Config` for preview purposes from explicit choices rather than
/// the on-disk file, so the wizard can show a live sample before writing
/// anything.
fn preview_config(
    theme: &str,
    layout: Layout,
    nerd_font: bool,
    pet_animate: bool,
    segments: &[String],
) -> Config {
    Config {
        theme: theme.to_string(),
        layout,
        nerd_font,
        pet_animate,
        segments: segments.to_vec(),
        ..Config::default()
    }
}

/// Fixed fake session data at a pinned clock, so previews render
/// deterministically without a live Claude Code session.
fn fixture_input(now: f64) -> crate::input::Input {
    let five_resets = now + 8_000.0; // partway through the 5h window
    let seven_resets = now + 300_000.0; // partway through the 7d window
    let raw = format!(
        r#"{{
            "model": {{"display_name": "Opus"}},
            "workspace": {{"current_dir": "."}},
            "context_window": {{"used_percentage": 42, "total_input_tokens": 84000, "context_window_size": 200000}},
            "cost": {{"total_cost_usd": 0.42, "total_duration_ms": 720000, "total_lines_added": 156, "total_lines_removed": 23}},
            "effort": {{"level": "high"}},
            "thinking": {{"enabled": true}},
            "rate_limits": {{
                "five_hour": {{"used_percentage": 60, "resets_at": {five_resets}}},
                "seven_day": {{"used_percentage": 20, "resets_at": {seven_resets}}}
            }}
        }}"#
    );
    crate::input::Input::parse(&raw)
}

fn render_preview_cfg(cfg: &Config) -> String {
    let now = 1_000_000.0;
    let input = fixture_input(now);
    crate::render::render(&input, None, cfg, now).join("\n")
}
