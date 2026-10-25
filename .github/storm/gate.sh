#!/usr/bin/env bash
# Sets the storm/review check that the ruleset on main requires. No model runs
# here: the agent that opened the pull request reviews it in its own session
# (the `storm` skill) and ends by commenting `Review finished at <sha>`.
#
#   EVENT=push     a new head: pending, unless it is the change last reviewed
#                  with only the base moved (BEFORE is the previous head).
#                  With ACTION=reopened there is no BEFORE: the head stays as
#                  it is when its own latest run is a success, else pending.
#   EVENT=comment  BODY from AUTHOR: `Review finished at <sha>` or
#                  `/storm:ready <sha>` naming the head sets it to success,
#                  marks a draft ready and posts the squash message; the owner
#                  merges, by hand or with auto-merge. `/storm:stop` labels
#                  the pull request and turns off auto-merge.
#   EVENT=edit     SENDER changed the title or body: on a ready pull request
#                  whose head passed review, posts the new squash message.
#                  Edits from anyone without write access are ignored.
#
# A command word ends at whitespace or the end of the line, so
# `/storm:readyish` and `/storm:stopwatch` do nothing.
#
# Runs from a checkout of main with the pull request's head in HEAD_DIR and
# origin/main fetched there. Needs REPO, PR, GH_TOKEN and EVENT. HEAD_SHA is
# the head to act on; without it, the pull request's current head is read.
# With DRY_RUN=1, writes to GitHub are printed instead of sent.
set -euo pipefail

: "${REPO:?}" "${PR:?}" "${EVENT:?}"
HEAD_DIR=${HEAD_DIR:-head}
CHECK_NAME='storm/review'
BOT_SLUG='stormlight-thunderbot'
STOP_LABEL='storm:stopped'
# Credits the owner on the squash commit, as Branch PR does. The settings come
# from main's checkout, like this script.
OWNER_TRAILER=$(sed -n 's/^trailer = "\(.*\)"$/\1/p' .github/fabrial.toml)
: "${OWNER_TRAILER:?.github/fabrial.toml has no trailer}"

# Runs gh for a write, or prints it under DRY_RUN.
gh_write() {
  if [[ ${DRY_RUN:-0} == 1 ]]; then
    printf 'DRY_RUN gh' >&2
    printf ' %q' "$@" >&2
    printf '\n' >&2
    # Show what a --body-file names, since the file is gone afterwards.
    local prev=
    for arg in "$@"; do
      [[ $prev == --body-file ]] && printf 'DRY_RUN body: %q\n' "$(cat "$arg")" >&2
      prev=$arg
    done
    return 0
  fi
  gh "$@"
}

# Long bodies go to gh as files: an argument over the kernel's limit fails
# before gh runs.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# Records the check on a commit: check <sha> <in_progress|success> <title> <summary>.
check() {
  local sha=$1 state=$2 title=$3 summary=$4
  local args=(-f name="$CHECK_NAME" -f head_sha="$sha" -f "output[title]=$title" -f "output[summary]=$summary")
  if [[ $state == in_progress ]]; then
    args+=(-f status=in_progress)
  else
    args+=(-f status=completed -f conclusion="$state")
  fi
  gh_write api -X POST "repos/$REPO/check-runs" "${args[@]}" >/dev/null
}

# Prints the summary of the bot's latest completed storm/review run on a
# commit when that run succeeded, and nothing otherwise. Runs from other apps
# are ignored, since any app with checks:write can post one under this name.
last_success() {
  gh api "repos/$REPO/commits/$1/check-runs?check_name=$CHECK_NAME" \
    --jq "[.check_runs[] | select(.status == \"completed\" and .app.slug == \"$BOT_SLUG\")]
      | sort_by(.completed_at) | last | select(.conclusion == \"success\") | .output.summary" || true
}

# Prints a hash of a commit's diff against its merge base with main, or
# nothing when the commit is not in the clone or adds nothing. The hash leaves
# out where the change sits: `index` lines, and hunk headers cut to `@@`, so a
# rebase over an earlier line of the same file keeps it. Every other byte
# counts, whitespace included.
diff_hash() {
  local sha=$1 base
  git -C "$HEAD_DIR" cat-file -e "$sha^{commit}" 2>/dev/null ||
    git -C "$HEAD_DIR" fetch -q origin "$sha" 2>/dev/null || return 0
  base=$(git -C "$HEAD_DIR" merge-base origin/main "$sha") || return 0
  git -C "$HEAD_DIR" diff --quiet "$base" "$sha" && return 0
  git -C "$HEAD_DIR" diff --binary --no-color --no-ext-diff "$base" "$sha" |
    sed -E '/^(index|similarity index|dissimilarity index) /d; s/^@@ -[0-9,]+ \+[0-9,]+ @@/@@/' | sha256sum | cut -d' ' -f1
}

# The marker a success leaves in its summary, so a later push can tell whether
# only the base moved.
marker() {
  printf '\n\n<!-- storm-review diff=%s -->' "$(diff_hash "$1")"
}

# Prints the SHA a command line names as its word number $1, without
# backticks or a trailing full stop.
named_sha() {
  awk -v n="$1" '{print $n}' <<<"$line" | tr -d '`.'
}

# Succeeds when $1 is at least 7 hex characters and a prefix of the head.
names_head() {
  [[ $1 =~ ^[0-9a-f]{7,40}$ && $head == "$1"* ]]
}

# Marks a draft pull request ready once its head has passed review. Merging,
# by hand or with auto-merge, is the owner's choice: the gate never turns
# auto-merge on or changes a request the owner set. A stopped or closed pull
# request is left alone. A failed call stops the script before the success is
# posted.
mark_ready() {
  local info
  info=$(gh pr view "$PR" --repo "$REPO" --json state,isDraft,labels)
  [[ $(jq -r .state <<<"$info") == OPEN ]] || return 0
  jq -e --arg l "$STOP_LABEL" 'any(.labels[]; .name == $l)' <<<"$info" >/dev/null && return 0
  if [[ $(jq -r .isDraft <<<"$info") == true ]]; then
    gh_write pr ready "$PR" --repo "$REPO" >/dev/null
  fi
}

# Posts the squash message Branch PR's flow builds, for the owner. Runs can
# overlap, such as an edit and a `Review finished`, and each reads the text
# when it starts. A run reads again after each post until a read finds
# nothing new, reading at most six times, so the run that posts last also
# posts the pull request's current text unless edits keep landing. Two runs
# that read the same text can both post it.
post_squash_message() {
  local attempt
  for attempt in 1 2 3 4 5 6; do
    posted=0
    squash_message
    (( posted )) || return 0
    [[ ${DRY_RUN:-0} != 1 ]] || return 0
  done
}

# Posts the squash message built from the pull request as it is now, and sets
# `posted` to 1. Posts nothing when the pull request is closed or stopped, or
# when the bot's newest squash message already says the same. A failed call
# stops the script.
squash_message() {
  local info title branch body text run fence='```' newest
  local intro='When you merge, by hand or with auto-merge, squash with this message:'
  info=$(gh pr view "$PR" --repo "$REPO" --json state,title,body,headRefName,labels,autoMergeRequest,comments)
  [[ $(jq -r .state <<<"$info") == OPEN ]] || return 0
  jq -e --arg l "$STOP_LABEL" 'any(.labels[]; .name == $l)' <<<"$info" >/dev/null && return 0
  # The subject carries the PR number, as GitHub's own squash title does, so
  # `git log` on main leads back to the pull request.
  title=$(jq -r .title <<<"$info")
  [[ $title == *"(#$PR)" ]] || title+=" (#$PR)"
  branch=$(jq -r .headRefName <<<"$info")
  body=$(jq -r '.body // ""' <<<"$info" | .github/storm/squash-body.sh "$branch" "$OWNER_TRAILER")
  # The fence outgrows any backtick run in the message, so a title cannot
  # close it early.
  run=$(grep -o '`*' <<<"$title$body" | awk '{ if (length > n) n = length } END { print n + 0 }')
  while (( ${#fence} <= run )); do fence+='`'; done
  text=$(printf '%s\n\n%stext\n%s\n\n%s\n%s' "$intro" "$fence" "$title" "$body" "$fence")
  # GitHub keeps the message auto-merge was turned on with, whatever the
  # pull request says later.
  if [[ $(jq -r '.autoMergeRequest != null' <<<"$info") == true ]]; then
    text+=$'\n\nAuto-merge is on and squashes with the message it was turned on with. To use this one, turn auto-merge off and on again with it.'
  fi
  newest=$(jq -r --arg slug "$BOT_SLUG" --arg intro "$intro" '
    [.comments[] | select((.author.login // "") | test("^(app/)?\($slug)(\\[bot\\])?$"))
      | select(.body | startswith($intro))] | last | .body // ""' <<<"$info")
  [[ $newest == "$text" ]] && return 0
  printf '%s' "$text" >"$scratch/comment"
  gh_write pr comment "$PR" --repo "$REPO" --body-file "$scratch/comment" >/dev/null
  posted=1
}

# Asks for a command again when the SHA it names is not the head.
wrong_sha() {
  local command=$1 named=$2
  gh_write pr comment "$PR" --repo "$REPO" \
    --body "\`$command\` names \`${named:-nothing}\`, but the head is \`$short\`. Check the head and post it again."
}

head=${HEAD_SHA:-$(gh pr view "$PR" --repo "$REPO" --json headRefOid --jq .headRefOid)}
short=${head:0:7}

if [[ $EVENT == edit ]]; then
  # Only the bot, or someone who could push to the branch, changes the message
  # a reviewed pull request merges with. A fork's author cannot.
  if [[ ${SENDER:-} != "$BOT_SLUG[bot]" ]]; then
    permission=$(gh api "repos/$REPO/collaborators/${SENDER:-none}/permission" --jq .permission 2>/dev/null || true)
    [[ $permission =~ ^(admin|maintain|write)$ ]] || exit 0
  fi
  # Read into variables, so a failed call stops the script.
  draft=$(gh pr view "$PR" --repo "$REPO" --json isDraft | jq -r .isDraft)
  reviewed=$(last_success "$head")
  if [[ $draft == false && -n $reviewed ]]; then
    post_squash_message
  fi
  exit 0
fi

if [[ $EVENT == push ]]; then
  if [[ ${ACTION:-} == reopened && -n $(last_success "$head") ]]; then
    exit 0
  fi
  before=${BEFORE:-}
  if [[ -n $before && ! $before =~ ^0+$ ]]; then
    reviewed=$(sed -n 's/.*<!-- storm-review diff=\([0-9a-f]*\) -->.*/\1/p' <<<"$(last_success "$before")")
    if [[ -n $reviewed && $reviewed == "$(diff_hash "$head")" ]]; then
      check "$head" success "Same change as ${before:0:7}" \
        "Only the base moved since ${before:0:7}, so its review stands.$(marker "$head")"
      exit 0
    fi
  fi
  check "$head" in_progress "Waiting for review" \
    "The agent that opened this pull request reviews it and comments \`Review finished at $short\`. A person can accept it as it is with \`/storm:ready $short\`."
  exit 0
fi

line=$(head -n 1 <<<"${BODY:-}" | tr -d '\r')
author=${AUTHOR:-someone}
case $line in
  'Review finished at '*)
    named=$(named_sha 4)
    if names_head "$named"; then
      mark_ready
      check "$head" success "Reviewed at $short" \
        "@$author finished the review of $short.$(marker "$head")"
      # After the success, so an edit from here on finds the head reviewed
      # and posts its own message.
      post_squash_message
    else
      wrong_sha 'Review finished' "$named"
    fi
    ;;
  /storm:ready | /storm:ready[[:space:]]*)
    named=$(named_sha 2)
    if names_head "$named"; then
      mark_ready
      check "$head" success "Marked ready by @$author" \
        "@$author accepted $short with \`/storm:ready\`.$(marker "$head")"
      post_squash_message
    else
      wrong_sha '/storm:ready' "$named"
    fi
    ;;
  /storm:stop | /storm:stop[[:space:]]*)
    gh_write label create "$STOP_LABEL" --repo "$REPO" --force \
      --description 'No auto-merge or agent work until a person removes this label' >/dev/null
    gh_write pr edit "$PR" --repo "$REPO" --add-label "$STOP_LABEL" >/dev/null
    gh_write pr merge "$PR" --repo "$REPO" --disable-auto >/dev/null 2>&1 || true
    ;;
esac
