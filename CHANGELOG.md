# Changelog

## v2.0.0

Configurable, uninstallable, and considerably more colorful.

- **Uninstall.** `zeek-meter-statusline uninstall` (also reachable via `uninstall.sh`/`uninstall.ps1`, mirroring the installers) reverses everything the installer did: the `statusLine` entry in `settings.json` (only if it still points at this binary), the VS Code font fallback, this tool's own config file, and — opt-in via `--remove-font` — the Nerd Font pack itself. `--dry-run` reports what would happen without changing anything.
- **`config` subcommand.** An interactive wizard (theme, layout, Nerd Font, extra segments, pet animation) that writes `~/.claude/zeek-meter-statusline.json`, plus `config --show` (resolved settings and where each came from), `config --set KEY=VALUE`, and `config --preview` for a live sample line with no session needed.
- **Truecolor themes.** Five presets (`neon`, `warm`, `mono`, `dracula`, `nord`), each a full RGB ramp that auto-downgrades to 256-color, 16-color, or plain text based on `COLORTERM`/`TERM`/`NO_COLOR` (or an explicit `CLAUDE_STATUSLINE_COLOR` override). Bars are tinted cool-to-hot along their length in Nerd Font mode; ASCII mode keeps v1's flat single-color bars.
- **Sub-cell bar precision.** Nerd Font bars now fill in eighth-of-a-cell steps (80 steps at the default width, vs. 10) instead of rounding to the nearest whole cell. The pace marker (`|`) is unchanged — still one cell, still overwrites whatever's under it — just recolored to the theme's accent so it stays visible against the gradient.
- **A more alive pet.** Seven states (fresh, calm, actively-working, worried, stressed, overheated, celebrating-a-reset) instead of four static faces, each with a short animation cycle that advances while Claude Code is actively redrawing the status line and holds still when idle. Kaomoji/box-drawing only, no emoji, so the line's width never jitters.
- **Eight new opt-in segments** reading the rest of what Claude Code already sends: `cost`, `duration`, `lines` (from `cost.*`), `effort`, `mode` (thinking/fast-mode/output-style/agent/vim), `pr`, `worktree`, `repo`, `reset` (time left in each rate-limit window), and `tokens` (raw token counts, correctly handling 1M-context models). Off by default — the out-of-the-box line is unchanged; add them via `config` or the config file's `segments` array.
- **Optional two-line layout** (`"layout": "two-line"`): identity/extras on the first row, meters and the pet on the second. Either layout drops low-priority segments first if the row would overflow `$COLUMNS`.

Everything above is additive to the default experience: with no config file, v2 still shows the same segments in the same order as v1 (`model | git | ctx | 5h | 7d | pet`) — just with a default truecolor theme, finer bars, and a livelier pet in place of v1's fixed 4-color palette and static faces.

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
