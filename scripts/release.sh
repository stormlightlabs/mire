#!/usr/bin/env bash
# Write a release's section of CHANGELOG.md with git-cliff and cliff.toml.
# Needs git and git-cliff.
# Usage: scripts/release.sh <version>
#
# The section lists the conventional commit subjects since the last v* tag,
# headed `## [<version>] - <today>`. It replaces the version's section when
# CHANGELOG.md has one, since the version is not tagged yet, and otherwise
# goes above the newest release's section, or at the end of the file. The
# script fails, changing nothing, when <version> is not a release like 0.2.0,
# differs from the [workspace.package] version in Cargo.toml, is already
# tagged v<version> here or on origin, or has no commits to list. It also
# fails when it cannot read origin's tags.
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
if git remote get-url origin >/dev/null 2>&1; then
  # --exit-code returns 2 when nothing matches; other codes are failures to
  # reach or read origin, not an absent tag.
  status=0
  git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null || status=$?
  case $status in
    0) fail "$tag is already tagged on origin." ;;
    2) ;;
    *) fail "could not read origin's tags (git ls-remote exited $status)." ;;
  esac
fi

section=$(git cliff --config cliff.toml --unreleased --tag "$tag" --strip all)
grep -q '^- ' <<<"$section" || fail "no commits to list since the last release."

# Replace the version's section, which runs to the next `## [` heading, or
# insert above the first `## [<digit>` heading, or append when neither exists.
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
# awk reads the section and heading from the environment, which keeps their
# backslashes.
SECTION=$section HEADING="## [$version]" awk '
  skip && /^## \[/ { skip = 0 }
  skip { next }
  !done && index($0, ENVIRON["HEADING"]) == 1 {
    print ENVIRON["SECTION"]; print ""; done = 1; skip = 1; next
  }
  !done && /^## \[[0-9]/ { print ENVIRON["SECTION"]; print ""; done = 1 }
  { print }
  END { if (!done) { print ""; print ENVIRON["SECTION"] } }
' CHANGELOG.md >"$tmp"
cat "$tmp" >CHANGELOG.md
echo "Wrote the $version section of CHANGELOG.md."
