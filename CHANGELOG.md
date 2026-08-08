# Changelog

## v1.0.1

- **Fix: Nerd Font icons never rendered on Windows (tofu boxes), even after a reboot.** Both installers registered the per-user font in `HKCU\...\CurrentVersion\Fonts` with a bare filename; Windows resolves that relative to the machine-wide `%WINDIR%\Fonts` dir, so the font was copied but never actually enumerable. Now registered with the full path, matching how Windows registers its own per-user fonts. `install.ps1` also activates newly-registered fonts in the current session immediately (`AddFontResource` + `WM_FONTCHANGE` broadcast) instead of requiring a sign-out.
- **Fix: font-already-installed detection never matched**, on any platform, because it checked for a space in `symbols nerd font` against filenames that don't have one — every install re-downloaded the ~2.85MB font pack. Detection is now a real usability check (`InstalledFontCollection` on Windows; a corrected filename pattern on macOS/Linux) instead of a guess.
- **Fix: the installer reported success without verifying it.** `$fontInstalledOk`/`font_installed_ok` used to mean "the download and copy didn't throw"; it now means "the font is confirmed enumerable," so the "Glyph test" message and the `nerd_font: false` fallback-config write are both trustworthy.
- **Re-running the installer now repairs a previously-broken install** instead of skipping re-registration because the font file was already on disk.

## v1.0.0

Full rewrite to a native Rust binary.

- **Native Rust binary.** No runtime dependency at all. Actual computation is sub-millisecond; end-to-end latency is now dominated by OS process-spawn overhead (~30ms measured on Windows via Git Bash) plus a `git status` subprocess.
- **Git branch without a subprocess.** Reads `.git/HEAD` directly (handles detached HEAD and linked worktrees) instead of spawning `git branch --show-current`. The dirty-flag check still needs `git status`, but is now cached per-directory for ~2 seconds instead of running on every refresh.
- **Nerd Font icons on by default**, with an installer that offers to fetch the (~2.85MB, symbols-only) font pack automatically — no admin rights needed on any platform. Disable with `CLAUDE_STATUSLINE_NERDFONT=0` or a config file.
- **Terminal config detection.** The installer detects VS Code's integrated terminal and offers to add the Nerd Font as an explicit fallback (the one common terminal that needs this — most others pick up an installed font automatically). Always asks first, always backs up before editing.
- **Interactive installer** with `--yes`/`--no-font`/`--no-terminal-config`/`--version` flags and checksum-verified downloads.
- **Versioned releases** for Windows (x86_64), macOS (x86_64 + aarch64), and Linux (x86_64 + aarch64, musl), each with a `SHA256SUMS` entry.

Rendering logic — bar math, pace-marker placement, color thresholds, and mood-face thresholds — is covered by unit tests in the crate.
