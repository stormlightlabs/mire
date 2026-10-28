#!/usr/bin/env bash
# Write a release's section of CHANGELOG.md with git-cliff and cliff.toml.
# Needs git and git-cliff.
# Usage: scripts/release.sh <version>
#
# The section lists the conventional commit subjects since the last v* tag,
# headed `## [<version>] - <today>`. It goes above the newest release's
# section, or at the end of the file when there is none. The script fails,
# changing nothing, when <version> is not a release like 0.2.0, differs from
# the [workspace.package] version in Cargo.toml, is already tagged
# v<version> here or on origin, or has no commits to list.
set -euo pipefail

usage() {
  echo "usage: scripts/release.sh <version>" >&2
  exit 2
}

fail() {
  echo "release: $*" >&2
  exit 1
}

[ "$#" -eq 1 ] || usage
version=$1
tag=v$version

repo=$(git rev-parse --show-toplevel)
cd "$repo"

[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "$version is not a release like 0.2.0."

current=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml)
[ "$current" = "$version" ] || fail "$version differs from the workspace version $current in Cargo.toml."

if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  fail "$tag is already tagged."
fi
if git remote get-url origin >/dev/null 2>&1 && git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
  fail "$tag is already tagged on origin."
fi

section=$(git cliff --config cliff.toml --unreleased --tag "$tag" --strip all)
grep -q '^- ' <<<"$section" || fail "no commits to list since the last release."

# Insert above the first `## [<digit>` heading, or append when none exists.
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
# awk reads the section from the environment, which keeps its backslashes.
SECTION=$section awk '
  !done && /^## \[[0-9]/ { print ENVIRON["SECTION"]; print ""; done = 1 }
  { print }
  END { if (!done) { print ""; print ENVIRON["SECTION"] } }
' CHANGELOG.md >"$tmp"
cat "$tmp" >CHANGELOG.md
echo "Wrote the $version section of CHANGELOG.md."
