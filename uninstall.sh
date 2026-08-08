#!/usr/bin/env bash
# Uninstalls zeek-meter-statusline: locates the installed binary and
# delegates the actual work to its `uninstall` subcommand (which owns all
# the JSON edits — settings.json, this tool's own config file, VS Code's
# settings.json — the same way install.sh delegates JSON work to
# `init --merge-settings` rather than doing it in bash).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/uninstall.sh | bash
#   curl -fsSL .../uninstall.sh | bash -s -- --yes                # non-interactive, all defaults
#   curl -fsSL .../uninstall.sh | bash -s -- --keep-config        # leave the config file in place
#   curl -fsSL .../uninstall.sh | bash -s -- --remove-font        # also remove the Nerd Font pack
#   curl -fsSL .../uninstall.sh | bash -s -- --dry-run            # report what would happen, change nothing
set -euo pipefail

BIN_NAME="zeek-meter-statusline"
CLAUDE_DIR="${CLAUDE_STATUSLINE_CLAUDE_DIR:-$HOME/.claude}"
INSTALLED_BIN="$CLAUDE_DIR/$BIN_NAME"

ASSUME_YES=0
DRY_RUN=0
PASSTHROUGH_ARGS=()

for arg in "$@"; do
  case "$arg" in
    --yes|-y) ASSUME_YES=1 ;;
    --dry-run) DRY_RUN=1 ;;
  esac
  PASSTHROUGH_ARGS+=("$arg")
done

log()  { printf '%s\n' "$*"; }
warn() { printf 'Warning: %s\n' "$*" >&2; }

# Piping this script via `curl | bash` makes stdin the script itself, so any
# interactive prompt the binary's `uninstall` subcommand makes must read from
# the controlling terminal directly, same as install.sh's `confirm()`.
TTY="/dev/tty"

if [ -x "$INSTALLED_BIN" ]; then
  if [ "$ASSUME_YES" -eq 1 ] || [ ! -e "$TTY" ]; then
    "$INSTALLED_BIN" uninstall "${PASSTHROUGH_ARGS[@]}"
  else
    "$INSTALLED_BIN" uninstall "${PASSTHROUGH_ARGS[@]}" < "$TTY"
  fi
  exit $?
fi

# Fallback: the binary is missing (already removed, or never installed here).
# There's nothing to delegate JSON-editing work to, so do the parts that
# don't need JSON parsing and tell the user the rest.
warn "$INSTALLED_BIN not found — doing minimal cleanup without it."

CONFIG_FILE="$CLAUDE_DIR/zeek-meter-statusline.json"
if [ -f "$CONFIG_FILE" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    log "Would remove $CONFIG_FILE"
  else
    rm -f "$CONFIG_FILE"
    log "Removed $CONFIG_FILE"
  fi
else
  log "No config file found at $CONFIG_FILE"
fi

log ""
log "Could not update settings.json automatically (the binary that does that JSON edit is gone)."
log "If $CLAUDE_DIR/settings.json still has a \"statusLine\" entry pointing at $BIN_NAME, remove it by hand."
log "If VS Code's settings.json still lists 'Symbols Nerd Font Mono' in terminal.integrated.fontFamily, you can leave it — it's harmless if the font stays installed, or remove it by hand."
