# claude-statusline

A [Claude Code](https://claude.com/claude-code) status line: model name, git branch (with a dirty marker), a context-window usage bar, and pace-aware bars for the 5-hour and 7-day rate-limit windows — plus a small mood face that reacts to how hot your usage is running. No dependencies beyond Node.js, which Claude Code already requires.

```
Sonnet 5 | main* | ctx [####------] 42% | 5h [#####|----] 60% | 7d [##---|----] 20% | :)
```

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/artzeeker/claude-statusline/main/install.sh | bash
```

This downloads `statusline.js` to `~/.claude/statusline.js` and merges a `statusLine` entry into `~/.claude/settings.json` (existing settings are preserved; if `settings.json` is invalid JSON it's backed up first). Works under Git Bash on Windows, macOS, and Linux. Restart Claude Code (or start a new session) afterward.

### Manual install

1. Copy `statusline.js` to `~/.claude/statusline.js`.
2. Add this to `~/.claude/settings.json`:
   ```json
   "statusLine": {
     "type": "command",
     "command": "node ~/.claude/statusline.js"
   }
   ```

## What it shows

| Segment | Meaning |
|---|---|
| Model | `context_window`'s model display name |
| Git branch | current branch, `*` suffix if `git status` has changes; omitted outside a git repo |
| `ctx [bar] pct%` | context window usage. Green `<50%`, yellow `50-80%`, red `>80%` |
| `5h [bar] pct%` | 5-hour rate-limit usage, with a `\|` pace marker showing how far through the 5-hour window you are |
| `7d [bar] pct%` | 7-day rate-limit usage, with the same pace marker for the weekly window |
| face | mood based on the worse of context% and how far the 5h bar is running ahead of its pace marker: `:)` calm, `:/` worried, `>:(` stressed, `X_X` overheated |

The pace marker means: if the bar's color is green, you're using the window at or under the rate the clock is passing. Yellow means you're up to 15 points ahead of pace; red means more than 15 points ahead.

Rate-limit bars show `n/a` when `rate_limits` isn't present in the session data (non-subscription accounts, or before the first API response of a session). Same for the context bar before the first response.

## Options

Set `CLAUDE_STATUSLINE_NERDFONT=1` in your shell profile to use Nerd Font icons and block-character bars instead of plain ASCII labels and `#`/`-` bars. Requires a terminal using a font with Nerd Font glyphs.
