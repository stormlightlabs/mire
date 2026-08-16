---
title: Quick start
description: Open a Git changeset, move through it, and choose a review layout.
section: Get started
group: Get started
order: 2
---

Run Mire inside a Git worktree. `mire` opens unstaged and untracked worktree
changes; `mire diff` is the equivalent explicit command:

```sh
mire
```

## Choose a changeset

Mire accepts the Git comparisons you already use:

```sh
# Staged changes
mire diff --staged

# Current branch since it diverged from main
mire diff main...HEAD

# Latest commit
mire show
```

Place repository-relative path filters after `--`:

```sh
mire diff main...HEAD -- src tests
```

## Move through the review

The review is one continuous, multi-file stream. Use `j` and `k` to move by
row, `]` and `[` to move by file, and `}` and `{` to move by hunk. Press `Tab`
to switch focus between the file sidebar and the review.

Press `?` at any time for the complete keybinding reference.

## Adjust the view

Press `1` for a unified diff, `2` for a split diff, or `3` to let Mire choose
from the terminal width. Use `+` and `-` to change the amount of surrounding
context, and `w` to wrap long lines.

Search the full changeset with `/`. Press `n` or `N` to visit the next or
previous match.

## Keep the review current

Add `--watch` when you want Mire to reload after the source changes:

```sh
mire diff main...HEAD --watch
```

See [Watch mode](/docs/guides/watch-mode/) for refresh behavior and recovery.
