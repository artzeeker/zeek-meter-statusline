//! `zeek-meter-statusline uninstall`: reverses everything the installer did,
//! in the same order the installer applied it — settings.json, then VS
//! Code's font fallback, then this tool's own config file, then (opt-in)
//! the Nerd Font pack, then the binary itself last, since a running
//! executable can't delete itself outright on Windows.

use std::io::{self, Write};

pub struct Options {
    pub yes: bool,
    pub dry_run: bool,
    pub keep_config: bool,
    pub keep_vscode: bool,
    pub remove_font: bool,
}

impl Options {
    pub fn parse(args: &[String]) -> Options {
        Options {
            yes: args.iter().any(|a| a == "--yes" || a == "-y"),
            dry_run: args.iter().any(|a| a == "--dry-run"),
            keep_config: args.iter().any(|a| a == "--keep-config"),
            keep_vscode: args.iter().any(|a| a == "--keep-vscode"),
            remove_font: args.iter().any(|a| a == "--remove-font"),
        }
    }
}

/// Prompts `question`, defaulting to **No** on a bare Enter — used for the
/// two destructive-but-optional steps (deleting the config file, removing
/// the shared Nerd Font pack) where the *gate* to even reach this prompt
/// (not `--keep-config`; `--remove-font`) already signals intent, but a
/// plain interactive run still asks before doing something a little scary.
///
/// `--yes` or `--dry-run` always answers "yes" here: `--yes` means skip
/// every prompt and do the standard full uninstall (config file removed
/// unless `--keep-config`, font removed if `--remove-font` was given), and
/// `--dry-run` needs to know what *would* happen to report it accurately.
fn confirm(question: &str, opts: &Options) -> bool {
    if opts.yes || opts.dry_run {
        return true;
    }
    print!("{question} [y/N] ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
    let line = line.trim().to_lowercase();
    line == "y" || line == "yes"
}

pub fn run(args: &[String]) -> i32 {
    let opts = Options::parse(args);
    if opts.dry_run {
        println!("Dry run — no changes will be made.\n");
    }

    let mut had_error = false;

    step_settings(&opts, &mut had_error);
    if !opts.keep_vscode {
        step_vscode(&opts, &mut had_error);
    }
    if !opts.keep_config {
        step_config_file(&opts, &mut had_error);
    }
    if opts.remove_font {
        step_font(&opts, &mut had_error);
    }
    step_binary(&opts, &mut had_error);

    if had_error {
        1
    } else {
        0
    }
}

fn step_settings(opts: &Options, had_error: &mut bool) {
    if opts.dry_run {
        match crate::settings::status_line_owned() {
            Ok((path, true)) => println!("Would remove statusLine entry from {}", path.display()),
            Ok((path, false)) if path.exists() => println!(
                "statusLine in {} doesn't point at this binary — would leave it alone",
                path.display()
            ),
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading settings.json: {e}");
                *had_error = true;
            }
        }
        return;
    }

    match crate::settings::unmerge_settings() {
        Ok((path, true)) => println!("Removed statusLine entry from {}", path.display()),
        Ok((path, false)) if path.exists() => println!(
            "statusLine in {} doesn't point at this binary; leaving it alone.",
            path.display()
        ),
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error updating settings.json: {e}");
            *had_error = true;
        }
    }
}

fn step_vscode(opts: &Options, had_error: &mut bool) {
    match crate::terminal::unconfigure_vscode(!opts.dry_run) {
        Ok(Some(path)) if opts.dry_run => {
            println!(
                "Would remove the Nerd Font fallback from {}",
                path.display()
            )
        }
        Ok(Some(path)) => println!("Removed the Nerd Font fallback from {}", path.display()),
        Ok(None) => {}
        Err(e) => {
            eprintln!("Error updating VS Code settings.json: {e}");
            *had_error = true;
        }
    }
}

fn step_config_file(opts: &Options, had_error: &mut bool) {
    let Some(path) = crate::config::config_path() else {
        return;
    };
    if !path.exists() {
        return;
    }
    if !confirm("Remove your saved settings too?", opts) {
        return;
    }
    if opts.dry_run {
        println!("Would remove {}", path.display());
        return;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => println!("Removed {}", path.display()),
        Err(e) => {
            eprintln!("Error removing {}: {e}", path.display());
            *had_error = true;
        }
    }
}

fn step_font(opts: &Options, had_error: &mut bool) {
    if opts.dry_run {
        let files = crate::font::font_files_present();
        if !files.is_empty() {
            println!("Would remove {} Nerd Font file(s):", files.len());
            for f in &files {
                println!("  {f}");
            }
            println!(
                "(other tools — starship, powerlevel10k, eza, lsd — commonly depend on this font)"
            );
        }
        return;
    }

    if !confirm(
        "Remove the Nerd Font symbols pack too? Other tools (starship, powerlevel10k, eza, lsd) may depend on it.",
        opts,
    ) {
        return;
    }
    match crate::font::remove_font() {
        Ok(files) if !files.is_empty() => println!("Removed {} Nerd Font file(s).", files.len()),
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error removing Nerd Font files: {e}");
            *had_error = true;
        }
    }
}

fn step_binary(opts: &Options, had_error: &mut bool) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    if opts.dry_run {
        println!("Would remove {}", exe.display());
        return;
    }

    if cfg!(target_os = "windows") {
        remove_self_windows(&exe, had_error);
    } else if let Err(e) = std::fs::remove_file(&exe) {
        eprintln!("Error removing {}: {e}", exe.display());
        *had_error = true;
    } else {
        println!("Removed {}", exe.display());
    }
}

/// Windows can't delete a running `.exe`. Renames it out of the way (which
/// *is* allowed while running), then spawns a detached helper that waits a
/// couple seconds for this process to exit and deletes the renamed file. If
/// even the rename fails, the binary is left in place and the user is told.
///
/// The helper is `powershell.exe -Command "Start-Sleep ...; Remove-Item
/// ..."` rather than `cmd /C timeout & del`: this binary may be invoked from
/// a bash-family shell (Git Bash, MSYS, WSL-adjacent tooling) whose `PATH`
/// puts a GNU coreutils `timeout` ahead of the real `%SystemRoot%\System32
/// \timeout.exe`, and GNU `timeout` rejects `/t 2` outright. `powershell.exe`
/// itself isn't shadowed by any of those toolchains, so this sidesteps the
/// ambiguity entirely. Stdio is nulled so the detached helper never leaks
/// output into (or blocks on) whatever launched this process.
fn remove_self_windows(exe: &std::path::Path, had_error: &mut bool) {
    let old = exe.with_extension("exe.old");
    if let Err(e) = std::fs::rename(exe, &old) {
        eprintln!(
            "Could not remove {} automatically ({e}); delete it by hand once Claude Code is closed.",
            exe.display()
        );
        *had_error = true;
        return;
    }
    println!("Removed {}", exe.display());

    let old_literal = old.to_string_lossy().replace('\'', "''");
    let ps_command = format!(
        "Start-Sleep -Seconds 2; Remove-Item -LiteralPath '{old_literal}' -Force -ErrorAction SilentlyContinue"
    );
    let spawned = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &ps_command,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if spawned.is_err() {
        println!(
            "(the renamed file at {} may be left behind — safe to delete by hand)",
            old.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags() {
        let args: Vec<String> = ["--yes", "--dry-run", "--keep-config", "--remove-font"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let opts = Options::parse(&args);
        assert!(opts.yes);
        assert!(opts.dry_run);
        assert!(opts.keep_config);
        assert!(opts.remove_font);
        assert!(!opts.keep_vscode);
    }

    #[test]
    fn defaults_are_all_off() {
        let opts = Options::parse(&[]);
        assert!(!opts.yes);
        assert!(!opts.dry_run);
        assert!(!opts.keep_config);
        assert!(!opts.keep_vscode);
        assert!(!opts.remove_font);
    }

    #[test]
    fn dry_run_config_step_touches_nothing() {
        // Point CLAUDE_STATUSLINE_CLAUDE_DIR at a scratch dir with a config
        // file present, run the config-file step in dry-run mode, and
        // confirm the file still exists afterward untouched.
        let dir = std::env::temp_dir().join(format!(
            "zeek-meter-statusline-uninstall-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CLAUDE_STATUSLINE_CLAUDE_DIR", &dir);

        let cfg_path = dir.join("zeek-meter-statusline.json");
        std::fs::write(&cfg_path, r#"{"nerd_font": false}"#).unwrap();

        let opts = Options {
            yes: true,
            dry_run: true,
            keep_config: false,
            keep_vscode: true,
            remove_font: false,
        };
        let mut had_error = false;
        step_config_file(&opts, &mut had_error);

        assert!(!had_error);
        assert!(cfg_path.exists());
        let contents = std::fs::read_to_string(&cfg_path).unwrap();
        assert_eq!(contents, r#"{"nerd_font": false}"#);

        std::env::remove_var("CLAUDE_STATUSLINE_CLAUDE_DIR");
        std::fs::remove_dir_all(&dir).ok();
    }
}
