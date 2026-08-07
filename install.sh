#!/usr/bin/env bash
# Installs the claude-statusline status line for Claude Code.
# Usage: curl -fsSL https://raw.githubusercontent.com/artzeeker/claude-statusline/main/install.sh | bash
set -euo pipefail

REPO_RAW="${CLAUDE_STATUSLINE_RAW_URL:-https://raw.githubusercontent.com/artzeeker/claude-statusline/main}"
CLAUDE_DIR="$HOME/.claude"
SCRIPT_PATH="$CLAUDE_DIR/statusline.js"
SETTINGS_PATH="$CLAUDE_DIR/settings.json"

if ! command -v node >/dev/null 2>&1; then
  echo "Error: node is required but was not found on PATH." >&2
  echo "Install Node.js (https://nodejs.org) and re-run this installer." >&2
  exit 1
fi

mkdir -p "$CLAUDE_DIR"

echo "Downloading statusline.js to $SCRIPT_PATH ..."
curl -fsSL "$REPO_RAW/statusline.js" -o "$SCRIPT_PATH"

echo "Updating $SETTINGS_PATH ..."
node -e '
const fs = require("fs");
const settingsPath = process.argv[1];

let settings = {};
if (fs.existsSync(settingsPath)) {
  const raw = fs.readFileSync(settingsPath, "utf8");
  try {
    settings = raw.trim() ? JSON.parse(raw) : {};
  } catch (e) {
    const backupPath = settingsPath + ".bak-" + Date.now();
    fs.copyFileSync(settingsPath, backupPath);
    console.error(`Warning: existing settings.json was invalid JSON. Backed it up to ${backupPath} and starting fresh.`);
    settings = {};
  }
}

settings.statusLine = {
  type: "command",
  command: "node ~/.claude/statusline.js",
};

fs.writeFileSync(settingsPath, JSON.stringify(settings, null, 2) + "\n");
' "$SETTINGS_PATH"

echo "Done. Start a new Claude Code session to see the new status line."
echo "Optional: export CLAUDE_STATUSLINE_NERDFONT=1 in your shell profile if your terminal uses a Nerd Font."
