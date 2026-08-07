#!/usr/bin/env bash
# Installs zeek-meter-statusline: downloads the release binary for this
# platform, merges its statusLine entry into Claude Code's settings.json, and
# (optionally, interactively) sets up Nerd Font glyphs.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/artzeeker/zeek-meter-statusline/main/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --yes                 # non-interactive, all defaults
#   curl -fsSL .../install.sh | bash -s -- --version v1.2.0      # pin a version
#   curl -fsSL .../install.sh | bash -s -- --no-font              # skip Nerd Font install
#   curl -fsSL .../install.sh | bash -s -- --no-terminal-config   # skip VS Code font-fallback edit
#
# No dependency beyond curl + the downloaded binary itself: settings.json and
# VS Code config edits are delegated to `zeek-meter-statusline init ...`
# rather than done here in bash, so this script never needs jq or node.
set -euo pipefail

REPO="artzeeker/zeek-meter-statusline"
GITHUB="https://github.com/$REPO"
API="https://api.github.com/repos/$REPO"
NERD_FONTS_REPO="ryanoasis/nerd-fonts"
NERD_FONTS_ASSET="NerdFontsSymbolsOnly"

VERSION=""
ASSUME_YES=0
WANT_FONT=1
WANT_TERMINAL_CONFIG=1

for arg in "$@"; do
  case "$arg" in
    --yes|-y) ASSUME_YES=1 ;;
    --no-font) WANT_FONT=0 ;;
    --no-terminal-config) WANT_TERMINAL_CONFIG=0 ;;
    --version)
      : # value consumed below
      ;;
    --version=*) VERSION="${arg#--version=}" ;;
    *) ;;
  esac
done
# Handle `--version vX.Y.Z` (space-separated) since the case above can't peek ahead.
prev=""
for arg in "$@"; do
  if [ "$prev" = "--version" ]; then
    VERSION="$arg"
  fi
  prev="$arg"
done

# Piping this script via `curl | bash` makes stdin the script itself, so
# interactive prompts must read from the controlling terminal directly.
TTY="/dev/tty"
INTERACTIVE=1
if [ "$ASSUME_YES" -eq 1 ] || [ ! -e "$TTY" ] || ! : < "$TTY" 2>/dev/null; then
  INTERACTIVE=0
fi

log()  { printf '%s\n' "$*"; }
warn() { printf 'Warning: %s\n' "$*" >&2; }
die()  { printf 'Error: %s\n' "$*" >&2; exit 1; }

# curl's built-in --retry only covers certain HTTP-transient failure classes,
# not a local write error (e.g. curl exit 23, seen in testing from brief
# antivirus file-lock contention on a freshly-created temp file) — so retry
# at the shell level too, for any failure mode. `curl_retry` behaves like
# `curl -fsSL` otherwise; pass any additional curl args through.
curl_retry() {
  local tries=3 attempt
  for attempt in $(seq 1 "$tries"); do
    if curl -fsSL "$@"; then
      return 0
    fi
    [ "$attempt" -lt "$tries" ] && sleep 1
  done
  return 1
}

confirm() {
  # confirm "question" default(y|n)
  local question="$1" default="$2" reply
  if [ "$INTERACTIVE" -eq 0 ]; then
    [ "$default" = "y" ] && return 0 || return 1
  fi
  local prompt="[Y/n]"
  [ "$default" = "n" ] && prompt="[y/N]"
  printf '%s %s ' "$question" "$prompt" > "$TTY"
  read -r reply < "$TTY" || reply=""
  reply="$(printf '%s' "$reply" | tr '[:upper:]' '[:lower:]')"
  if [ -z "$reply" ]; then
    [ "$default" = "y" ] && return 0 || return 1
  fi
  [ "$reply" = "y" ] || [ "$reply" = "yes" ]
}

# ---------------------------------------------------------------------------
# Platform detection
# ---------------------------------------------------------------------------

detect_os() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) echo "windows" ;;
    Darwin) echo "macos" ;;
    Linux) echo "linux" ;;
    *) die "unsupported OS: $(uname -s)" ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    *) die "unsupported architecture: $(uname -m)" ;;
  esac
}

# Maps (os, arch) to the Rust target triple used in release asset names.
# Must match the build matrix in .github/workflows/release.yml exactly.
target_triple() {
  local os="$1" arch="$2"
  case "$os" in
    windows)
      [ "$arch" = "x86_64" ] || die "no Windows build for $arch"
      echo "x86_64-pc-windows-msvc"
      ;;
    macos)
      case "$arch" in
        x86_64) echo "x86_64-apple-darwin" ;;
        aarch64) echo "aarch64-apple-darwin" ;;
      esac
      ;;
    linux)
      case "$arch" in
        x86_64) echo "x86_64-unknown-linux-musl" ;;
        aarch64) echo "aarch64-unknown-linux-musl" ;;
      esac
      ;;
  esac
}

OS="$(detect_os)"
ARCH="$(detect_arch)"
TARGET="$(target_triple "$OS" "$ARCH")"
BIN_NAME="zeek-meter-statusline"
[ "$OS" = "windows" ] && EXE_SUFFIX=".exe" || EXE_SUFFIX=""
ARCHIVE_EXT="tar.gz"
[ "$OS" = "windows" ] && ARCHIVE_EXT="zip"

CLAUDE_DIR="${CLAUDE_STATUSLINE_CLAUDE_DIR:-$HOME/.claude}"
INSTALLED_BIN="$CLAUDE_DIR/${BIN_NAME}${EXE_SUFFIX}"

mkdir -p "$CLAUDE_DIR"

# ---------------------------------------------------------------------------
# Version resolution + download
# ---------------------------------------------------------------------------

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

# GitHub's release JSON is pretty-printed (one field per line), so a plain
# grep for the first tag_name line is reliable without needing jq. Downloads
# to a file rather than piping straight into grep: retrying a failed curl
# *through* a live pipe is unsafe (a downstream reader like `grep -m1` can
# close its end early, or a retried attempt's output can land mixed with a
# prior partial stream in the same pipe) — a file gets cleanly overwritten
# on each attempt instead.
resolve_latest_version() {
  local api_url="$1" dest="$WORK_DIR/latest_release.json"
  curl_retry "$api_url" -o "$dest" || return 1
  grep -m1 '"tag_name"' "$dest" | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
}

if [ -z "$VERSION" ]; then
  VERSION="$(resolve_latest_version "$API/releases/latest")"
  [ -n "$VERSION" ] || die "could not resolve the latest release version"
fi

log "Installing zeek-meter-statusline $VERSION for $TARGET..."

ARCHIVE_NAME="${BIN_NAME}-${TARGET}.${ARCHIVE_EXT}"
DOWNLOAD_URL="$GITHUB/releases/download/$VERSION/$ARCHIVE_NAME"
CHECKSUMS_URL="$GITHUB/releases/download/$VERSION/SHA256SUMS"

curl_retry "$DOWNLOAD_URL" -o "$WORK_DIR/$ARCHIVE_NAME" \
  || die "failed to download $DOWNLOAD_URL (check the version exists: $GITHUB/releases)"
curl_retry "$CHECKSUMS_URL" -o "$WORK_DIR/SHA256SUMS" \
  || die "failed to download SHA256SUMS for $VERSION"

verify_checksum() {
  local expected
  expected="$(grep "$ARCHIVE_NAME"'$' "$WORK_DIR/SHA256SUMS" | awk '{print $1}')"
  [ -n "$expected" ] || die "no checksum entry for $ARCHIVE_NAME in SHA256SUMS"
  local actual
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$WORK_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$WORK_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
  else
    warn "no sha256sum/shasum found; skipping checksum verification"
    return 0
  fi
  [ "$expected" = "$actual" ] || die "checksum mismatch for $ARCHIVE_NAME (expected $expected, got $actual)"
}
verify_checksum

log "Checksum verified."

extract_dir="$WORK_DIR/extracted"
mkdir -p "$extract_dir"
if [ "$ARCHIVE_EXT" = "zip" ]; then
  unzip -o -q "$WORK_DIR/$ARCHIVE_NAME" -d "$extract_dir"
else
  tar -xzf "$WORK_DIR/$ARCHIVE_NAME" -C "$extract_dir"
fi

found_bin="$(find "$extract_dir" -type f -name "${BIN_NAME}${EXE_SUFFIX}" | head -n1)"
[ -n "$found_bin" ] || die "archive didn't contain ${BIN_NAME}${EXE_SUFFIX}"

# ---------------------------------------------------------------------------
# Migrate off the old Node-based statusline, if present
# ---------------------------------------------------------------------------

SETTINGS_PATH="$CLAUDE_DIR/settings.json"
OLD_NODE_SCRIPT="$CLAUDE_DIR/statusline.js"
migrating_from_node=0
if [ -f "$SETTINGS_PATH" ] && grep -q 'statusline\.js' "$SETTINGS_PATH" 2>/dev/null; then
  migrating_from_node=1
  log "Detected the previous Node-based statusline; migrating to the compiled binary."
fi

cp "$found_bin" "$INSTALLED_BIN"
chmod +x "$INSTALLED_BIN" 2>/dev/null || true
log "Installed $INSTALLED_BIN"

if [ "$migrating_from_node" -eq 1 ] && [ -f "$OLD_NODE_SCRIPT" ]; then
  rm -f "$OLD_NODE_SCRIPT"
  log "Removed the old $OLD_NODE_SCRIPT"
fi

# ---------------------------------------------------------------------------
# settings.json (delegated to the binary — no jq/node needed here)
# ---------------------------------------------------------------------------

"$INSTALLED_BIN" init --merge-settings

# ---------------------------------------------------------------------------
# Nerd Font (symbols-only, ~2.85MB, user-scope, no admin)
# ---------------------------------------------------------------------------

install_font_windows() {
  local zip_path="$1"
  local dest="$LOCALAPPDATA/Microsoft/Windows/Fonts"
  mkdir -p "$dest"
  local extract="$WORK_DIR/font-extract"
  mkdir -p "$extract"
  unzip -o -q "$zip_path" -d "$extract"
  local installed=0
  for f in "$extract"/*.ttf "$extract"/*.otf; do
    [ -e "$f" ] || continue
    local base
    base="$(basename "$f")"
    if [ -f "$dest/$base" ]; then
      continue
    fi
    cp "$f" "$dest/$base"
    local display_name="${base%.*}"
    reg add "HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts" \
      /v "$display_name (TrueType)" /t REG_SZ /d "$base" /f >/dev/null 2>&1 || true
    installed=1
  done
  [ "$installed" -eq 1 ] && log "Installed Nerd Font symbols to $dest (restart your terminal to pick it up)." \
    || log "Nerd Font symbols already installed."
}

install_font_macos() {
  local zip_path="$1"
  local dest="$HOME/Library/Fonts"
  mkdir -p "$dest"
  local extract="$WORK_DIR/font-extract"
  mkdir -p "$extract"
  unzip -o -q "$zip_path" -d "$extract"
  local installed=0
  for f in "$extract"/*.ttf "$extract"/*.otf; do
    [ -e "$f" ] || continue
    local base
    base="$(basename "$f")"
    [ -f "$dest/$base" ] && continue
    cp "$f" "$dest/$base"
    installed=1
  done
  [ "$installed" -eq 1 ] && log "Installed Nerd Font symbols to $dest." || log "Nerd Font symbols already installed."
}

install_font_linux() {
  local zip_path="$1"
  local dest="$HOME/.local/share/fonts"
  mkdir -p "$dest"
  local extract="$WORK_DIR/font-extract"
  mkdir -p "$extract"
  unzip -o -q "$zip_path" -d "$extract"
  local installed=0
  for f in "$extract"/*.ttf "$extract"/*.otf; do
    [ -e "$f" ] || continue
    local base
    base="$(basename "$f")"
    [ -f "$dest/$base" ] && continue
    cp "$f" "$dest/$base"
    installed=1
  done
  if [ "$installed" -eq 1 ]; then
    command -v fc-cache >/dev/null 2>&1 && fc-cache -f "$dest" >/dev/null 2>&1 || true
    log "Installed Nerd Font symbols to $dest."
  else
    log "Nerd Font symbols already installed."
  fi
}

font_already_installed() {
  case "$OS" in
    windows) [ -d "$LOCALAPPDATA/Microsoft/Windows/Fonts" ] && ls "$LOCALAPPDATA/Microsoft/Windows/Fonts" 2>/dev/null | grep -qi "symbols nerd font" ;;
    macos) [ -d "$HOME/Library/Fonts" ] && ls "$HOME/Library/Fonts" 2>/dev/null | grep -qi "symbols nerd font" ;;
    linux) [ -d "$HOME/.local/share/fonts" ] && ls "$HOME/.local/share/fonts" 2>/dev/null | grep -qi "symbols nerd font" ;;
  esac
}

font_installed_ok=0
if [ "$WANT_FONT" -eq 1 ]; then
  do_font=1
  if [ "$INTERACTIVE" -eq 1 ] && ! font_already_installed; then
    confirm "Install the Nerd Font symbols pack (~2.85MB, user-scope, no admin needed) so icons render?" y || do_font=0
  fi
  if [ "$do_font" -eq 1 ]; then
    if font_already_installed; then
      log "Nerd Font symbols already installed."
      font_installed_ok=1
    else
      nf_latest="$(resolve_latest_version "https://api.github.com/repos/$NERD_FONTS_REPO/releases/latest")"
      if [ -n "$nf_latest" ]; then
        nf_url="https://github.com/$NERD_FONTS_REPO/releases/download/$nf_latest/${NERD_FONTS_ASSET}.zip"
        if curl_retry "$nf_url" -o "$WORK_DIR/nerd-fonts-symbols.zip"; then
          case "$OS" in
            windows) install_font_windows "$WORK_DIR/nerd-fonts-symbols.zip" ;;
            macos) install_font_macos "$WORK_DIR/nerd-fonts-symbols.zip" ;;
            linux) install_font_linux "$WORK_DIR/nerd-fonts-symbols.zip" ;;
          esac
          font_installed_ok=1
        else
          warn "could not download the Nerd Font symbols pack; continuing without it"
        fi
      else
        warn "could not resolve the latest Nerd Fonts release; continuing without font install"
      fi
    fi
  fi
fi

if [ "$font_installed_ok" -eq 0 ]; then
  # Persist the "no Nerd Font" choice so the statusline defaults to plain
  # ASCII bars instead of showing tofu boxes for missing glyphs. Lives in a
  # small dedicated config file (not settings.json) so it survives even if
  # the user never sets a shell env var — important on Windows.
  cfg="$CLAUDE_DIR/zeek-meter-statusline.json"
  printf '{"nerd_font": false}\n' > "$cfg"
  log "Nerd Font icons disabled (set CLAUDE_STATUSLINE_NERDFONT=1 or edit $cfg once a Nerd Font is available)."
fi

# ---------------------------------------------------------------------------
# Terminal config (delegated to the binary for the JSON edit itself)
# ---------------------------------------------------------------------------

if [ "$WANT_TERMINAL_CONFIG" -eq 1 ] && [ "$font_installed_ok" -eq 1 ]; then
  while IFS='|' read -r name path needs_edit note; do
    [ -z "$name" ] && continue
    if [ "$name" = "VS Code" ] && [ "$needs_edit" = "true" ]; then
      do_edit=1
      if [ "$INTERACTIVE" -eq 1 ]; then
        confirm "VS Code's integrated terminal needs an explicit font-fallback entry to show icons — add it to $path?" y || do_edit=0
      fi
      if [ "$do_edit" -eq 1 ]; then
        "$INSTALLED_BIN" init --configure-vscode --apply && log "Updated $path"
      fi
    elif [ "$name" = "Windows Terminal" ] && [ -n "$path" ]; then
      log "Windows Terminal: $note"
    fi
  done < <("$INSTALLED_BIN" init --detect-terminals)
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

log ""
log "Done. Start a new Claude Code session to see the status line."
if [ "$font_installed_ok" -eq 1 ]; then
  log "Glyph test (should show distinct icons, not boxes): $(printf '\357\213\233 \357\204\230 \357\203\244 \357\200\227 \357\204\263')"
fi
log "Options: --version vX.Y.Z to pin, --no-font, --no-terminal-config, --yes for non-interactive."
