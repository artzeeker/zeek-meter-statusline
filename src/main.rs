//! zeek-meter-statusline: a status line for Claude Code.
//!
//! Two modes:
//!  - default (no subcommand): reads the session JSON Claude Code pipes to
//!    stdin, prints one rendered status line.
//!  - `init ...`: one-shot setup helpers the installer (`install.sh`) calls
//!    into, so the installer itself never needs `jq` to do JSON work — see
//!    `settings.rs` and `terminal.rs` for why that matters.

mod bar;
mod config;
mod git;
mod input;
mod render;
mod settings;
mod terminal;

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("zeek-meter-statusline {VERSION}");
        return ExitCode::SUCCESS;
    }

    if args.first().map(String::as_str) == Some("init") {
        return run_init(&args[1..]);
    }

    run_statusline(&args)
}

fn run_statusline(args: &[String]) -> ExitCode {
    let mut cli_nerd_font: Option<bool> = None;
    for a in args {
        match a.as_str() {
            "--nerd-font" => cli_nerd_font = Some(true),
            "--no-nerd-font" => cli_nerd_font = Some(false),
            _ => {}
        }
    }

    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    let data = input::Input::parse(&raw);

    let git_info = data
        .resolved_cwd()
        .map(|c| Path::new(&c).to_path_buf())
        .or_else(|| std::env::current_dir().ok())
        .and_then(|cwd| git::git_info(&cwd, data.session_id.as_deref()));

    let nerd = config::resolve_nerd_font(cli_nerd_font);
    println!("{}", render::render_now(&data, git_info.as_ref(), nerd));
    ExitCode::SUCCESS
}

fn run_init(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--merge-settings") => match settings::merge_settings() {
            Ok(path) => {
                println!("Updated {}", path.display());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Error updating settings.json: {e}");
                ExitCode::FAILURE
            }
        },
        Some("--detect-terminals") => {
            for d in terminal::detect_all() {
                let path = d
                    .config_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                println!("{}|{}|{}|{}", d.name, path, d.needs_edit, d.note);
            }
            ExitCode::SUCCESS
        }
        Some("--configure-vscode") => {
            let apply = args.iter().any(|a| a == "--apply");
            match terminal::configure_vscode(apply) {
                Ok(Some(path)) => {
                    if apply {
                        println!("Updated {}", path.display());
                    } else {
                        println!("Would update {}", path.display());
                    }
                    ExitCode::SUCCESS
                }
                Ok(None) => {
                    println!("No change needed.");
                    ExitCode::from(2)
                }
                Err(e) => {
                    eprintln!("Error updating VS Code settings.json: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "usage: zeek-meter-statusline init <--merge-settings|--detect-terminals|--configure-vscode [--apply]>"
            );
            ExitCode::FAILURE
        }
    }
}
