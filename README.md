# zeek-meter-statusline

A [Claude Code](https://claude.com/claude-code) status line, compiled to a single native binary: model name, git branch (with a dirty marker), a context-window usage bar, and pace-aware bars for the 5-hour and 7-day rate-limit windows — plus a small pet that reacts to how hot your usage is running. Truecolor themes, eight opt-in extra segments, an interactive config wizard, and a real uninstall path.

```
Sonnet 5 | main* | ctx [####------] 42% | 5h [#####|----] 60% | 7d [##---|----] 20% | :)
```

With Nerd Font icons enabled (the default — see [Nerd Font glyphs](#nerd-font-glyphs) below), bars fill in eighth-of-a-cell steps and are tinted along the active theme:

```
 Sonnet 5 |  main* |  [▓▓▓▓▎-----] 42% |  [▓▓▓▓▓|----] 60% |  [▓▓--|-----] 20% | (·ᴗ·)
```

**No runtime dependency at all** — the binary is statically compiled per platform; the installer only needs `curl` (already on every supported OS).

## Install

macOS, Linux, Git Bash, or WSL:

```bash
curl -fsSL https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.sh | bash
```

Windows (PowerShell — don't use the `bash` command above there; PowerShell's `curl` is an alias for `Invoke-WebRequest` and doesn't understand `-fsSL`):

```powershell
irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.ps1 | iex
```

The installer asks a couple of questions (skip with `--yes` / `-Yes`, or answer them individually via flags below):

- Install a small Nerd Font symbols pack (~2.85MB, no admin needed) so the icons render?
- If VS Code's integrated terminal is detected without Nerd Font glyph support, add a font fallback entry for it?
- Run the interactive [config wizard](#configuration) now to pick a theme, layout, and extra segments?

It downloads the correct release binary for your platform, verifies its checksum against the release's `SHA256SUMS`, installs it to `~/.claude/zeek-meter-statusline` (`.exe` on Windows), and merges a `statusLine` entry into `~/.claude/settings.json` — existing settings are preserved, and an invalid `settings.json` is backed up rather than overwritten. Restart Claude Code (or start a new session) afterward.

Options:

| Flag (bash) | Flag (PowerShell) | Effect |
|---|---|---|
| `--yes` | `-Yes` | Non-interactive: accept every default (installs the font, configures detected terminals, skips the config wizard) |
| `--version vX.Y.Z` | `-Version vX.Y.Z` | Pin a specific release instead of the latest |
| `--no-font` | `-NoFont` | Skip the Nerd Font install; the statusline falls back to plain ASCII bars |
| `--no-terminal-config` | `-NoTerminalConfig` | Skip editing any terminal config, even if one is detected as needing it |
| `--no-wizard` | `-NoWizard` | Skip the offer to run the interactive config wizard |

```bash
curl -fsSL .../install.sh | bash -s -- --yes
curl -fsSL .../install.sh | bash -s -- --version v1.0.0 --no-font
```

`irm | iex` runs the script with no way to pass params directly, so download it into a scriptblock first to pass flags in PowerShell:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.ps1))) -Yes
& ([scriptblock]::Create((irm .../install.ps1))) -Version v1.0.0 -NoFont
```

### Manual install

Grab the archive matching your platform from the [latest release](https://github.com/artzeeker/zeek-meter-statusline/releases/latest), extract the binary to `~/.claude/zeek-meter-statusline` (`.exe` on Windows), then add this to `~/.claude/settings.json`:

```json
"statusLine": {
  "type": "command",
  "command": "~/.claude/zeek-meter-statusline"
}
```

(`.exe` suffix on Windows.)

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/uninstall.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/uninstall.ps1 | iex
```

Or, if the binary is already on your machine: `~/.claude/zeek-meter-statusline uninstall` (`.exe` on Windows). Either way it reverses what the installer did, in order:

1. Removes the `statusLine` entry from `~/.claude/settings.json` — **only** if it still points at `zeek-meter-statusline`; if you've since switched to a different status line, it's left alone and you're told so.
2. Removes the Nerd Font fallback it added to VS Code's `terminal.integrated.fontFamily`, leaving your primary font and any other fallbacks intact (skip with `--keep-vscode`).
3. Deletes `~/.claude/zeek-meter-statusline.json` (skip with `--keep-config`) — you're asked first unless `--yes` is given.
4. Removes the installed Nerd Font pack itself, but **only** with `--remove-font` — other tools (starship, powerlevel10k, eza, lsd…) commonly depend on the same font, so this isn't on by default.
5. Removes the binary last (on Windows, since a running `.exe` can't delete itself, it renames itself and a detached helper deletes it a couple of seconds later).

Every backed-up/edited file gets a `.bak-<timestamp>` copy first, same as the installer. Add `--dry-run` to see exactly what would happen without changing anything, or `--yes` to skip every confirmation.

## What it shows

| Segment | Meaning |
|---|---|
| Model | the session's model display name |
| Git branch | current branch (read directly from `.git/HEAD`, no subprocess), `*` suffix if the working tree has changes; omitted outside a git repo |
| `ctx [bar] pct%` | context window usage. Green `<50%`, yellow `50-80%`, red `>80%` |
| `5h [bar] pct%` | 5-hour rate-limit usage, with a `\|` pace marker showing how far through the 5-hour window you are |
| `7d [bar] pct%` | 7-day rate-limit usage, with the same pace marker for the weekly window |
| pet | reacts to the worse of context% and how far the 5h bar is running ahead of its pace marker, and animates while Claude Code is actively redrawing the line — see [The pet](#the-pet) |

The pace marker means: if the bar's color is green (`ok`), you're using the window at or under the rate the clock is passing. Yellow (`warn`) means you're up to 15 points ahead of pace; red (`hot`) means more than 15 points ahead. In Nerd Font mode each bar is also tinted cool-to-hot along its length (a gradient, not a threshold); ASCII mode keeps one flat color per bar like v1 did.

Rate-limit bars show `n/a` when `rate_limits` isn't present in the session data (non-subscription accounts, or before the first API response of a session). Same for the context bar before the first response.

Eight more segments — `cost`, `duration`, `lines`, `effort`, `mode`, `pr`, `worktree`, `repo`, `reset`, `tokens` — are available but off by default; see [Configuration](#configuration) to add them.

### The pet

The pet cycles through a short animation while Claude Code is actively redrawing the status line (roughly every 300ms during real work) and holds still when idle:

| State | Trigger | Nerd Font | ASCII |
|---|---|---|---|
| Fresh | no context/rate-limit data yet | `(o_o)?` | `:o` |
| Calm | worst < 15% | `(·ᴗ·)` (blinks) | `:)` |
| Working | worst < 15%, actively redrawing | `(·ᴗ·)ノ` | `:)` |
| Worried | worst 15-40% | `(·﹏·)` | `:/` |
| Stressed | worst 40-70% | `(>_<)` | `>:(` |
| Overheated | worst ≥ 70%, or over 200k tokens | `(x_x)` | `X_X` |
| Celebrating | a rate-limit window just reset | `✧(•ᴗ•)✧` | `\o/` |

"Worst" is the same measure v1 used: the higher of context-window usage and how far the 5h bar is running ahead of its pace marker (never how far behind — being under pace isn't stressful). Disable the pet entirely, or its animation, via `config` or `"pet": {"enabled": false}` in the config file.

## Configuration

Everything below is optional — with no config file, the line looks like v1's did (same segments, same order), just with the new default theme and finer bars.

### The config wizard

```bash
~/.claude/zeek-meter-statusline config              # interactive: pick a theme, layout, extra segments
~/.claude/zeek-meter-statusline config --show        # print the resolved config and where each value came from
~/.claude/zeek-meter-statusline config --set theme=nord
~/.claude/zeek-meter-statusline config --preview --theme dracula --layout two-line
```

(`.exe` suffix on Windows.) The installer offers to run the wizard for you at the end of a fresh install.

### Config file

`~/.claude/zeek-meter-statusline.json` — every key optional, unknown keys ignored:

```jsonc
{
  "theme": "neon",              // neon | warm | mono | dracula | nord
  "color": "auto",              // auto | truecolor | 256 | 16 | none
  "layout": "one-line",         // one-line | two-line
  "nerd_font": true,
  "bar_width": 10,
  "percent_decimals": 0,
  "separator": "pipe",          // pipe | dot | none
  "segments": ["model", "git", "ctx", "5h", "7d", "pet"],
  "pet": { "enabled": true, "animate": true }
}
```

`segments` is also the render order — reorder it, drop entries, or add any of `cost`, `duration`, `lines`, `effort`, `mode`, `pr`, `worktree`, `repo`, `reset`, `tokens`. In `layout: "two-line"`, `ctx`/`5h`/`7d`/`reset`/`tokens`/`pet` render on the second line and everything else on the first; either layout drops the lowest-priority segment first if a row would overflow `$COLUMNS` (Claude Code sets this env var for you).

Precedence for every setting: CLI flag > env var (`CLAUDE_STATUSLINE_NERDFONT`, `CLAUDE_STATUSLINE_THEME`, `CLAUDE_STATUSLINE_COLOR`, `CLAUDE_STATUSLINE_LAYOUT`, `NO_COLOR`) > this config file > default.

### Themes

`neon` (default), `warm`, `mono`, `dracula`, `nord` — each a full RGB ramp that auto-downgrades to 256-color, 16-color ANSI, or plain text depending on what your terminal reports (`COLORTERM`, `TERM`, or `NO_COLOR`). Force a specific depth with `CLAUDE_STATUSLINE_COLOR=truecolor|256|16|none` or the config file's `color` key. Preview any of them without touching your config:

```bash
~/.claude/zeek-meter-statusline config --preview --theme nord
```

## Nerd Font glyphs

Nerd Font icons are **on by default** — install.sh offers to install the (tiny, symbols-only) font pack that supplies them, so most people never have to think about it. If you skip that or don't have a compatible font, disable icons:

```bash
export CLAUDE_STATUSLINE_NERDFONT=0
```

or edit `~/.claude/zeek-meter-statusline.json`:

```json
{ "nerd_font": false }
```

(Precedence: CLI flag > `CLAUDE_STATUSLINE_NERDFONT` env var > that config file > default-on.)

The font installer specifically fetches [`NerdFontsSymbolsOnly`](https://github.com/ryanoasis/nerd-fonts) — just the icon glyphs, not a full patched typeface — so it doesn't touch your primary font. Most terminals (Windows Terminal, iTerm2, kitty, Alacritty…) pick up installed fonts as an automatic fallback for glyphs their primary font lacks, so no further configuration is needed there. VS Code's integrated terminal is the one common exception — it needs the font explicitly listed in `terminal.integrated.fontFamily`, which is what the installer's terminal-config step offers to do for you.

**Icons show as boxes on Windows?** Older installer versions registered the per-user font with a bare filename instead of a full path, which Windows silently resolves relative to the machine-wide `%WINDIR%\Fonts` directory — the font is copied but never actually usable. Check `HKCU:\Software\Microsoft\Windows NT\CurrentVersion\Fonts` in the registry: the `Symbols Nerd Font*` entries should hold a full path (e.g. `C:\Users\you\AppData\Local\Microsoft\Windows\Fonts\SymbolsNerdFont-Regular.ttf`), matching the form of entries Windows itself creates. Re-running the installer repairs this automatically. If you edited VS Code's settings while the font was still broken, fully quit and reopen VS Code afterward — it only reads fonts at process start.

## Performance

Actual computation — JSON parsing, git state, bar rendering — is sub-millisecond, well under the 300ms debounce Claude Code uses between statusline refreshes. Branch name comes from reading `.git/HEAD` directly instead of spawning `git`, and the dirty-flag check (which does need `git status`) is cached for ~2 seconds per directory instead of running on every refresh. What's left is pure OS process-spawn overhead (~30ms measured via Git Bash on Windows — the same floor a completely no-op binary pays).

## Versioning

Tagged releases (`vX.Y.Z`) build binaries for Windows (x86_64), macOS (x86_64 + aarch64), and Linux (x86_64 + aarch64, statically linked via musl), each with a checksum in the release's `SHA256SUMS`. See [CHANGELOG.md](CHANGELOG.md).

## Building from source

```bash
cargo build --release
```

Requires only `serde`/`serde_json` — no other runtime dependencies, on purpose.
