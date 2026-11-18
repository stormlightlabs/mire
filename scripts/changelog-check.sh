#!/usr/bin/env bash
# Check cliff.toml's pre-release cleanup against the changelog git-cliff
# renders. Needs git-cliff and the full history.
# Usage: scripts/changelog-check.sh
#
# Each commit cliff.toml skips by SHA must leave no entry, and each commit it
# rewrites by SHA must show the new line in place of its old subject. The
# script fails, naming every commit that breaks a rule, and fails when a
# listed SHA is not in the history, as in a shallow clone. It also fails when
# an entry is written in a layout it cannot read, or when either list changes
# size.
set -euo pipefail

# The pre-release lists are closed, so their sizes are fixed. A deleted entry
# changes the count even though nothing else in cliff.toml names it.
SKIPPED_COUNT=15
REWRITTEN_COUNT=33

fail() {
  echo "changelog-check: $*" >&2
  exit 1
}

repo=$(git rev-parse --show-toplevel)
cd "$repo"

command -v git-cliff >/dev/null || fail "git-cliff is not installed."

# The entry git-cliff writes for a subject, as the body template in
# cliff.toml renders it: the type dropped, the scope in bold before the
# message, and the message's first letter upper-cased.
entry() {
  local scope="" message="$1"
  if [[ $1 =~ ^[a-z]+(\(([^\)]*)\))?!?:\ (.*)$ ]]; then
    scope=${BASH_REMATCH[2]}
    message=${BASH_REMATCH[3]}
  fi
  message="$(tr '[:lower:]' '[:upper:]' <<<"${message:0:1}")${message:1}"
  if [ -n "$scope" ]; then
    printf -- '- **%s:** %s\n' "$scope" "$message"
  else
    printf -- '- %s\n' "$message"
  fi
}

expect_entry() {
  local got
  got=$(entry "$1")
  [ "$got" = "$2" ] || fail "entry \"$1\" gave \"$got\", expected \"$2\"."
}

expect_entry 'docs: theming' '- Theming'
expect_entry 'feat(tui): add keys' '- **tui:** Add keys'
expect_entry 'fix(web)!: drop it' '- **web:** Drop it'
expect_entry 'feat!: break it' '- Break it'

log=$(mktemp)
trap 'rm -f "$log"' EXIT
changelog=$(git cliff --config cliff.toml --strip all 2>"$log") || {
  cat "$log" >&2
  fail "git-cliff failed."
}
errors=0

has_line() {
  grep -qxF -- "$1" <<<"$changelog"
}

# Called outside a command substitution, so its failure ends the script.
require_commit() {
  git cat-file -e "$1^{commit}" 2>/dev/null || fail "$1 in cliff.toml is not in the history."
}

subject() {
  git log -1 --format=%s "$1"
}

# The parsers below accept either key order and either quote style. Each
# list is also counted without parsing, so an entry the parser misses fails
# the check instead of dropping out of it. Comment lines are left out.
config=$(grep -v '^[[:space:]]*#' cliff.toml)
skipped=$(grep -E 'skip *= *true' <<<"$config" | grep -E '\bsha *=' |
  sed -nE "s/.*\\bsha *= *[\"']([0-9a-f]{40})[\"'].*/\\1/p" || true)
[ -n "$skipped" ] || fail "found no skipped SHAs in cliff.toml."
skip_lines=$(grep -cE '\bsha *=' <<<"$config" || true)
[ "$(wc -w <<<"$skipped")" -eq "$skip_lines" ] ||
  fail "cliff.toml has $skip_lines sha parsers but $(wc -w <<<"$skipped") read as skipped SHAs; write each as { sha = \"<40-hex SHA>\", skip = true }."
[ "$skip_lines" -eq "$SKIPPED_COUNT" ] ||
  fail "cliff.toml skips $skip_lines commits by SHA, expected $SKIPPED_COUNT; the list is closed."
for sha in $skipped; do
  require_commit "$sha"
  line=$(entry "$(subject "$sha")")
  if has_line "$line"; then
    echo "changelog-check: $sha is skipped but still listed: $line" >&2
    errors=$((errors + 1))
  fi
done

rewrites=$(grep -F 'COMMIT_SHA' <<<"$config" | sed 's/\\"/"/g' |
  sed -nE "s/.*\\\$COMMIT_SHA\"? *= *\"?([0-9a-f]{40})\"? *\\].* echo '([^']*)'.*/\\1 \\2/p" || true)
[ -n "$rewrites" ] || fail "found no rewritten SHAs in cliff.toml."
sha_refs=$(grep -oF 'COMMIT_SHA' <<<"$config" | wc -l)
commands=$(grep -cF 'replace_command' <<<"$config" || true)
[ "$(wc -l <<<"$rewrites")" -eq "$sha_refs" ] && [ "$sha_refs" -eq "$commands" ] ||
  fail "cliff.toml has $commands replace_command entries and $sha_refs \$COMMIT_SHA references but $(wc -l <<<"$rewrites") read as rewrites; write each as [ \$COMMIT_SHA = <40-hex SHA> ] && { cat >/dev/null; echo '<subject>'; } || cat."
[ "$commands" -eq "$REWRITTEN_COUNT" ] ||
  fail "cliff.toml rewrites $commands commits by SHA, expected $REWRITTEN_COUNT; the list is closed."
while read -r sha message; do
  require_commit "$sha"
  old=$(entry "$(subject "$sha")")
  new=$(entry "$message")
  if has_line "$old"; then
    echo "changelog-check: $sha keeps its old subject: $old" >&2
    errors=$((errors + 1))
  fi
  if ! has_line "$new"; then
    echo "changelog-check: $sha is missing its rewrite: $new" >&2
    errors=$((errors + 1))
  fi
done <<<"$rewrites"

[ "$errors" -eq 0 ] || fail "$errors problem(s) in cliff.toml's pre-release cleanup."
echo "changelog-check: $(wc -w <<<"$skipped") skipped and $(wc -l <<<"$rewrites") rewritten commits match cliff.toml."
