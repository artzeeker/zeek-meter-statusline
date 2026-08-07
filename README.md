# zeek-meter-statusline

A [Claude Code](https://claude.com/claude-code) status line, compiled to a single native binary: model name, git branch (with a dirty marker), a context-window usage bar, and pace-aware bars for the 5-hour and 7-day rate-limit windows — plus a small mood face that reacts to how hot your usage is running.

```
Sonnet 5 | main* | ctx [####------] 42% | 5h [#####|----] 60% | 7d [##---|----] 20% | :)
```

With Nerd Font icons enabled (the default — see [Nerd Font glyphs](#nerd-font-glyphs) below):

```
 Sonnet 5 |  main* |  [####------] 42% |  [#####|----] 60% |  [##---|----] 20% | :)
```

**No runtime dependency at all** — not even Node. The binary is statically compiled per platform; the installer only needs `curl` (already on every supported OS). If you're coming from the old Node-based version, this is a straight upgrade: the installer detects and removes it automatically.

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

It downloads the correct release binary for your platform, verifies its checksum against the release's `SHA256SUMS`, installs it to `~/.claude/zeek-meter-statusline` (`.exe` on Windows), and merges a `statusLine` entry into `~/.claude/settings.json` — existing settings are preserved, and an invalid `settings.json` is backed up rather than overwritten. Restart Claude Code (or start a new session) afterward.

Options:

| Flag (bash) | Flag (PowerShell) | Effect |
|---|---|---|
| `--yes` | `-Yes` | Non-interactive: accept every default (installs the font, configures detected terminals) |
| `--version vX.Y.Z` | `-Version vX.Y.Z` | Pin a specific release instead of the latest |
| `--no-font` | `-NoFont` | Skip the Nerd Font install; the statusline falls back to plain ASCII bars |
| `--no-terminal-config` | `-NoTerminalConfig` | Skip editing any terminal config, even if one is detected as needing it |

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

## What it shows

| Segment | Meaning |
|---|---|
| Model | the session's model display name |
| Git branch | current branch (read directly from `.git/HEAD`, no subprocess), `*` suffix if the working tree has changes; omitted outside a git repo |
| `ctx [bar] pct%` | context window usage. Green `<50%`, yellow `50-80%`, red `>80%` |
| `5h [bar] pct%` | 5-hour rate-limit usage, with a `\|` pace marker showing how far through the 5-hour window you are |
| `7d [bar] pct%` | 7-day rate-limit usage, with the same pace marker for the weekly window |
| face | mood based on the worse of context% and how far the 5h bar is running ahead of its pace marker: `:)` calm, `:/` worried, `>:(` stressed, `X_X` overheated |

The pace marker means: if the bar's color is green, you're using the window at or under the rate the clock is passing. Yellow means you're up to 15 points ahead of pace; red means more than 15 points ahead.

Rate-limit bars show `n/a` when `rate_limits` isn't present in the session data (non-subscription accounts, or before the first API response of a session). Same for the context bar before the first response.

## Nerd Font glyphs

Nerd Font icons are **on by default** in v1 — install.sh offers to install the (tiny, symbols-only) font pack that supplies them, so most people never have to think about it. If you skip that or don't have a compatible font, disable icons:

```bash
export CLAUDE_STATUSLINE_NERDFONT=0
```

or edit `~/.claude/zeek-meter-statusline.json`:

```json
{ "nerd_font": false }
```

(Precedence: CLI flag > `CLAUDE_STATUSLINE_NERDFONT` env var > that config file > default-on.)

The font installer specifically fetches [`NerdFontsSymbolsOnly`](https://github.com/ryanoasis/nerd-fonts) — just the icon glyphs, not a full patched typeface — so it doesn't touch your primary font. Most terminals (Windows Terminal, iTerm2, kitty, Alacritty…) pick up installed fonts as an automatic fallback for glyphs their primary font lacks, so no further configuration is needed there. VS Code's integrated terminal is the one common exception — it needs the font explicitly listed in `terminal.integrated.fontFamily`, which is what the installer's terminal-config step offers to do for you.

## Performance

The old Node-based version cost ~358ms per invocation on Windows (Node startup + a `git status` subprocess for the branch name), measurably slower than the 300ms debounce Claude Code uses between statusline refreshes. This version's actual computation — JSON parsing, git state, bar rendering — is sub-millisecond; branch name comes from reading `.git/HEAD` directly instead of spawning `git`, and the dirty-flag check (which does need `git status`) is cached for ~2 seconds per directory instead of running on every refresh. What's left is pure OS process-spawn overhead (~30ms measured via Git Bash on Windows — the same floor a completely no-op binary pays), still 3-4x faster than a no-op Node startup alone.

## Versioning

Tagged releases (`vX.Y.Z`) build binaries for Windows (x86_64), macOS (x86_64 + aarch64), and Linux (x86_64 + aarch64, statically linked via musl), each with a checksum in the release's `SHA256SUMS`. See [CHANGELOG.md](CHANGELOG.md).

## Building from source

```bash
cargo build --release
```

Requires only `serde`/`serde_json` — no other runtime dependencies, on purpose.
