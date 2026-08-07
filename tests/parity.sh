#!/usr/bin/env bash
# Golden-file parity check: asserts the Rust binary produces byte-identical
# output to the original statusline.js for the same fixture inputs. This is a
# migration-verification tool, not part of the shipped project — statusline.js
# itself isn't part of this repo (it's the prior iteration, still at
# ~/.claude/statusline.js on the machine that built this rewrite). Once this
# passes, statusline.js is safe to delete per the project plan.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN="$REPO_ROOT/target/release/zeek-meter-statusline.exe"
REFERENCE_JS="${REFERENCE_JS:-$HOME/.claude/statusline.js}"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
# node.exe and our Rust binary are plain Win32 executables (not MSYS-linked
# like git.exe), so they can't resolve MSYS-virtual paths such as those
# `mktemp -d` returns (/tmp/...). Any path we embed as JSON *content* (as
# opposed to an argv element MSYS auto-translates) must already be in
# Windows-native form, or reading it from inside node/rust silently fails.
# `pwd -W` gives that form under Git Bash; elsewhere (macOS/Linux) it's not a
# recognized flag and plain `pwd` is already native, so fall back to it.
REPO_ROOT_WIN="$(cd "$REPO_ROOT" && { pwd -W 2>/dev/null || pwd; })"
WORK_DIR="$REPO_ROOT_WIN/target/parity-tmp"
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"
trap 'rm -rf "$WORK_DIR"' EXIT

if [ ! -x "$BIN" ]; then
  echo "error: $BIN not found. Run 'cargo build --release' first." >&2
  exit 1
fi
if [ ! -f "$REFERENCE_JS" ]; then
  echo "error: reference statusline.js not found at $REFERENCE_JS (set REFERENCE_JS=...)" >&2
  exit 1
fi

now=$(date +%s)
five_mid=$((now + 9000))       # 5h window, 50% elapsed
seven_mid=$((now + 302400))    # 7d window, 50% elapsed
five_near_end=$((now + 2000))  # 5h window, ~89% elapsed -> overheated case
seven_near_end=$((now + 50000))

mkdir -p "$FIXTURES_DIR"

# --- Fixture: normal mid-window, no git context (plain non-git cwd) -------
plain_dir="$WORK_DIR/plain"
mkdir -p "$plain_dir"
cat > "$FIXTURES_DIR/mid_window_no_git.json" <<EOF
{"model":{"display_name":"Sonnet 5"},"workspace":{"current_dir":"$plain_dir"},"context_window":{"used_percentage":42},"rate_limits":{"five_hour":{"used_percentage":60,"resets_at":$five_mid},"seven_day":{"used_percentage":20,"resets_at":$seven_mid}}}
EOF

# --- Fixture: missing rate_limits and context_window -----------------------
cat > "$FIXTURES_DIR/missing_fields.json" <<EOF
{"model":{"display_name":"Opus"},"workspace":{"current_dir":"$plain_dir"}}
EOF

# --- Fixture: overheated (ctx 85%, 5h way ahead of pace) -------------------
cat > "$FIXTURES_DIR/overheated.json" <<EOF
{"model":{"display_name":"Opus"},"workspace":{"current_dir":"$plain_dir"},"context_window":{"used_percentage":85},"rate_limits":{"five_hour":{"used_percentage":95,"resets_at":$five_near_end},"seven_day":{"used_percentage":10,"resets_at":$seven_near_end}}}
EOF

# --- Fixture: clean git repo with commits -----------------------------------
clean_repo="$WORK_DIR/clean_repo"
mkdir -p "$clean_repo"
git -C "$clean_repo" init -q -b main
git -C "$clean_repo" -c user.email=t@e.st -c user.name=Test commit -q --allow-empty -m "init"
cat > "$FIXTURES_DIR/clean_git_repo.json" <<EOF
{"model":{"display_name":"Sonnet 5"},"workspace":{"current_dir":"$clean_repo"},"context_window":{"used_percentage":10}}
EOF

# --- Fixture: dirty git repo (staged + untracked) ---------------------------
dirty_repo="$WORK_DIR/dirty_repo"
mkdir -p "$dirty_repo"
git -C "$dirty_repo" init -q -b main
git -C "$dirty_repo" -c user.email=t@e.st -c user.name=Test commit -q --allow-empty -m "init"
echo "change" > "$dirty_repo/file.txt"
git -C "$dirty_repo" add file.txt
cat > "$FIXTURES_DIR/dirty_git_repo.json" <<EOF
{"model":{"display_name":"Sonnet 5"},"workspace":{"current_dir":"$dirty_repo"},"context_window":{"used_percentage":15}}
EOF

# --- Fixture: repo with no commits yet --------------------------------------
no_commits_repo="$WORK_DIR/no_commits_repo"
mkdir -p "$no_commits_repo"
git -C "$no_commits_repo" init -q -b main
echo "x" > "$no_commits_repo/a.txt"
git -C "$no_commits_repo" add a.txt
cat > "$FIXTURES_DIR/no_commits_yet.json" <<EOF
{"model":{"display_name":"Sonnet 5"},"workspace":{"current_dir":"$no_commits_repo"},"context_window":{"used_percentage":5}}
EOF

# --- Fixture: detached HEAD ---------------------------------------------------
detached_repo="$WORK_DIR/detached_repo"
mkdir -p "$detached_repo"
git -C "$detached_repo" init -q -b main
git -C "$detached_repo" -c user.email=t@e.st -c user.name=Test commit -q --allow-empty -m "one"
git -C "$detached_repo" -c user.email=t@e.st -c user.name=Test commit -q --allow-empty -m "two"
git -C "$detached_repo" checkout -q HEAD~1
cat > "$FIXTURES_DIR/detached_head.json" <<EOF
{"model":{"display_name":"Sonnet 5"},"workspace":{"current_dir":"$detached_repo"},"context_window":{"used_percentage":25}}
EOF

# statusline.js defaults Nerd Font OFF (only '1' ever enabled it); the Rust
# binary defaults it ON (v1's intentional behavior change, see plan). Pin
# both to the same explicit value here so this check isolates "did the port
# preserve the rendering logic" from "the default changed" — the latter is
# a deliberate, separately-documented difference, not something parity
# should flag as a regression.
export CLAUDE_STATUSLINE_NERDFONT=0

pass=0
fail=0
for fixture in "$FIXTURES_DIR"/*.json; do
  name="$(basename "$fixture")"
  js_out=$(node "$REFERENCE_JS" < "$fixture")
  rs_out=$("$BIN" < "$fixture")
  if [ "$js_out" = "$rs_out" ]; then
    echo "PASS  $name"
    pass=$((pass + 1))
  else
    echo "FAIL  $name"
    echo "  js: $(printf '%q' "$js_out")"
    echo "  rs: $(printf '%q' "$rs_out")"
    fail=$((fail + 1))
  fi
done

echo ""
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
