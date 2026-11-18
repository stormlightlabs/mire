#!/usr/bin/env bash
# Check cliff.toml's pre-release cleanup against the changelog git-cliff
# renders. Needs git-cliff and the full history.
# Usage: scripts/changelog-check.sh
#
# Each commit cliff.toml skips by SHA must leave no entry, and each commit it
# rewrites by SHA must show the new line in place of its old subject. The
# script fails, naming every commit that breaks a rule, and fails when a
# listed SHA is not in the history, as in a shallow clone.
set -euo pipefail

fail() {
  echo "changelog-check: $*" >&2
  exit 1
}

repo=$(git rev-parse --show-toplevel)
cd "$repo"

command -v git-cliff >/dev/null || fail "git-cliff is not installed."

# The entry git-cliff writes for a subject: the type and scope dropped, the
# first letter upper-cased, as the body template in cliff.toml does.
entry() {
  local message
  message=$(sed -E 's/^[a-z]+(\([^)]*\))?!?: //' <<<"$1")
  printf -- '- %s%s\n' "$(tr '[:lower:]' '[:upper:]' <<<"${message:0:1}")" "${message:1}"
}

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

skipped=$(sed -nE 's/^ *\{ sha = "([0-9a-f]{40})", skip = true \}.*/\1/p' cliff.toml)
[ -n "$skipped" ] || fail "found no skipped SHAs in cliff.toml."
for sha in $skipped; do
  require_commit "$sha"
  line=$(entry "$(subject "$sha")")
  if has_line "$line"; then
    echo "changelog-check: $sha is skipped but still listed: $line" >&2
    errors=$((errors + 1))
  fi
done

rewrites=$(sed -nE "s/.*\\\$COMMIT_SHA = ([0-9a-f]{40}) \].* echo '([^']*)'.*/\1 \2/p" cliff.toml)
[ -n "$rewrites" ] || fail "found no rewritten SHAs in cliff.toml."
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
