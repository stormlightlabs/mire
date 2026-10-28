#!/usr/bin/env bash
# Capture Mire's TUI states as PNG and ANSI files with .github/storm/shot.sh.
# Needs tmux, freeze and jq.
# Usage: scripts/shots.sh [<scenario>...]
#        scripts/shots.sh --list
#
# With no scenario, captures every one. Each scenario runs the debug build of
# mire against a fixture repository created in a temporary directory, so the
# captures do not depend on the checkout's own changes. The files go under
# $FAB_SHOT_DIR, resolved against the current directory, or else under
# ${TMPDIR:-/tmp}/fabrial-shots/<branch>, named after the Mire checkout's
# branch. A directory inside the checkout is refused. shot.sh prints each
# PNG's path.
set -euo pipefail

SCENARIOS=(unified split narrow help sidebar-hidden collapsed note viewed walkthrough walkthrough-step walkthrough-narrow session-walkthrough generated no-color)

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

repo=$(cd "$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)" && pwd -P)
shot=$repo/.github/storm/shot.sh

# shot.sh runs inside the fixture, so it would name and check the output
# directory against the fixture. Resolve it here against the Mire checkout,
# through its nearest existing parent, and hand shot.sh the absolute path.
ref=$(git -C "$repo" symbolic-ref --short -q HEAD || git -C "$repo" rev-parse --short HEAD)
out=${FAB_SHOT_DIR:-${TMPDIR:-/tmp}/fabrial-shots/${ref//\//-}}
case "$out" in /*) ;; *) out=$PWD/$out ;; esac
dir=${out%/} rest=''
while [ ! -d "$dir" ]; do
  rest=/${dir##*/}$rest
  dir=$(dirname "$dir")
done
out=$(cd "$dir" && pwd -P)$rest
case "$out/" in
  "$repo/"*)
    echo "shots: $out is inside the repository" >&2
    exit 2
    ;;
esac
export FAB_SHOT_DIR=$out

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p mire --bin mire
# CARGO_TARGET_DIR or build.target-dir can move the build out of the repo.
target=$(cargo metadata --format-version 1 --no-deps --manifest-path "$repo/Cargo.toml" | jq -r .target_directory)
mire=$target/debug/mire

# The fixture and a private XDG_RUNTIME_DIR share one temporary directory.
# Live sessions register under $XDG_RUNTIME_DIR/mire, so the walkthrough
# scenario sees only the session it starts, not other open reviews.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fixture=$work/repo
mkdir -m 700 "$fixture" "$work/runtime"
export XDG_RUNTIME_DIR=$work/runtime

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
  (cd "$fixture" && "$mire" walkthrough apply review.json --stdin >/dev/null) <<'EOF'
{
  "schema_version": { "major": 1, "minor": 4 },
  "review_revision": 3,
  "overview": "Parsed values kept their spaces, so `port = 8080` never matched. This change trims them, reads the port from `MIRE_PORT`, and documents the format.",
  "author": { "id": "review-agent", "display_name": null },
  "provenance": { "kind": "agent", "producer": "shots" },
  "steps": [
    {
      "title": "Trim keys and values",
      "narrative": "`split` now uses `split_once` and trims both sides. A line with an empty key returns `None`, so callers skip it instead of storing an empty name.",
      "locations": [{ "file": "src/parse.rs", "new_line": 3, "end": 8 }]
    },
    {
      "title": "Read the port from the environment",
      "narrative": "`default_port` reads `MIRE_PORT` and falls back to 8080 when it is unset or does not parse. Check what a value outside 1-65535 does.",
      "locations": [{ "file": "src/lib.rs", "new_line": 7, "end": 10 }]
    },
    {
      "title": "Document the format",
      "narrative": "The new README names the `key = value` format and the function that reads it.",
      "locations": [{ "file": "README.md", "new_line": 1, "end": 3 }]
    }
  ]
}
EOF
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
    viewed) run --keys Tab --keys j --keys m viewed 120 32 "${review[@]}" ;;
    walkthrough) run --keys W walkthrough 120 32 "${review[@]}" ;;
    walkthrough-step) run --keys J --keys J walkthrough-step 120 32 "${review[@]}" ;;
    walkthrough-narrow) run --keys J --keys J walkthrough-narrow 60 24 "${review[@]}" ;;
    session-walkthrough)
      # The live-session command needs a running TUI, so it starts in the
      # background and drives the TUI that replaces the shell.
      run --wait-for "walkthrough 1/" session-walkthrough 120 32 bash -c '
        (sleep 2
          session=$("$0" session list | jq -r ".sessions[0].id")
          "$0" session walkthrough start "$session" >/dev/null
          "$0" session walkthrough next "$session" >/dev/null) &
        exec "$0" review review.json --theme eldritch' "$mire"
      ;;
    generated)
      # A global attributes file marks the README as generated without
      # adding a .gitattributes file to the fixture's diff.
      printf 'README.md linguist-generated\n' >"$work/attributes"
      run generated 120 32 env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.attributesFile \
        GIT_CONFIG_VALUE_0="$work/attributes" "${diff[@]}"
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
