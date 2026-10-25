#!/usr/bin/env bash
# Capture Mire's TUI states as PNG and ANSI files with .github/storm/shot.sh.
# Needs tmux, freeze and jq.
# Usage: scripts/shots.sh [<scenario>...]
#        scripts/shots.sh --list
#
# With no scenario, captures every one. Each scenario runs the debug build of
# mire against a fixture repository created in a temporary directory, so the
# captures do not depend on the checkout's own changes. shot.sh writes the
# files under $FAB_SHOT_DIR, or a temporary directory named after the branch,
# and prints each PNG's path.
set -euo pipefail

SCENARIOS=(unified split narrow help sidebar-hidden collapsed note walkthrough no-color)

usage() {
  echo "usage: scripts/shots.sh [--list | <scenario>...]" >&2
  echo "scenarios: ${SCENARIOS[*]}" >&2
  exit 2
}

if [ "${1:-}" = --list ]; then
  printf '%s\n' "${SCENARIOS[@]}"
  exit 0
fi
for name in "$@"; do
  [[ " ${SCENARIOS[*]} " == *" $name "* ]] || usage
done
[ "$#" -gt 0 ] || set -- "${SCENARIOS[@]}"

repo=$(git rev-parse --show-toplevel)
shot=$repo/.github/storm/shot.sh
cargo build --quiet --manifest-path "$repo/Cargo.toml" -p mire --bin mire
mire=$repo/target/debug/mire

fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

# A small Rust project with one commit and a worktree change that edits,
# adds, and deletes files. Fixed names, dates, and contents keep the
# captures stable between runs.
build_fixture() {
  local g=(git -C "$fixture" -c user.name=Mire -c user.email=mire@example.invalid)
  export GIT_AUTHOR_DATE=2026-01-01T00:00:00Z GIT_COMMITTER_DATE=2026-01-01T00:00:00Z
  git -C "$fixture" init --quiet --initial-branch=main
  mkdir -p "$fixture/src"
  cat >"$fixture/src/lib.rs" <<'EOF'
//! Parses key-value configuration.

pub mod parse;

/// Returns the default port.
pub fn default_port() -> u16 {
    8080
}

/// Returns the default host.
pub fn default_host() -> &'static str {
    "localhost"
}
EOF
  cat >"$fixture/src/parse.rs" <<'EOF'
/// Splits one `key=value` line.
pub fn split(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, '=');
    let key = parts.next()?;
    let value = parts.next()?;
    Some((key, value))
}
EOF
  printf 'Old notes about the parser.\n' >"$fixture/NOTES.md"
  "${g[@]}" add -A
  "${g[@]}" commit --quiet -m "Add the parser"

  cat >"$fixture/src/lib.rs" <<'EOF'
//! Parses key-value configuration.

pub mod parse;

/// Returns the default port, or `MIRE_PORT` when it is set.
pub fn default_port() -> u16 {
    std::env::var("MIRE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8080)
}

/// Returns the default host.
pub fn default_host() -> &'static str {
    "127.0.0.1"
}
EOF
  cat >"$fixture/src/parse.rs" <<'EOF'
/// Splits one `key = value` line, trimming spaces around both parts.
pub fn split(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value.trim()))
}
EOF
  printf '# Config\n\nRead `key = value` lines with `parse::split`.\n' >"$fixture/README.md"
  rm "$fixture/NOTES.md"
  "${g[@]}" add -A

  (cd "$fixture" && "$mire" review init review.json --staged >/dev/null)
  (cd "$fixture" && "$mire" note add review.json --revision 1 --file src/lib.rs --new-line 8 \
    --author reviewer --provenance agent --producer shots --severity medium --kind defect \
    --body "A port outside 1-65535 falls back silently. Report it instead." >/dev/null)
  (cd "$fixture" && "$mire" note add review.json --revision 2 --file src/parse.rs --new-line 4 \
    --author reviewer --provenance agent --producer shots --severity note --kind question \
    --body "Should an empty value be accepted?" >/dev/null)
}

# Runs one scenario: shot.sh options, the name, the size, then the command.
capture() {
  local name=$1
  local diff=("$mire" diff --staged --theme eldritch)
  local review=("$mire" review review.json --theme eldritch)
  case "$name" in
    unified) run --keys 1 unified 120 32 "${diff[@]}" ;;
    split) run --keys 2 split 160 32 "${diff[@]}" ;;
    narrow) run narrow 60 24 "${diff[@]}" ;;
    help) run --keys '?' help 100 32 "${diff[@]}" ;;
    sidebar-hidden) run --keys b sidebar-hidden 120 32 "${diff[@]}" ;;
    collapsed) run --keys Enter collapsed 120 32 "${diff[@]}" ;;
    note) run --keys p note 120 32 "${review[@]}" ;;
    walkthrough)
      # The live-session command needs a running TUI, so it starts in the
      # background and drives the TUI that replaces the shell.
      run --wait-for "walkthrough 1/" walkthrough 120 32 bash -c '
        (sleep 2
          session=$("$0" session list | jq -r ".sessions[0].id")
          "$0" session walkthrough start "$session" >/dev/null) &
        exec "$0" review review.json --theme eldritch' "$mire"
      ;;
    no-color) run no-color 120 32 env NO_COLOR=1 "$mire" diff --staged ;;
  esac
}

run() {
  (cd "$fixture" && COLORTERM=truecolor "$shot" --wait-for Mire --timeout 20 "$@")
}

build_fixture
status=0
for name in "$@"; do
  capture "$name" || status=$?
done
exit "$status"
