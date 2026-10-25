---
title: Reviewing Changes
description: Inspect worktrees, revisions, commits, and patch files in the terminal viewer.
section: Guides
group: Guides
order: 3
---

Mire normalizes Git and patch input into the same changeset model before it
renders anything. Navigation, layouts, search, syntax highlighting, and JSON
output therefore behave consistently across sources.

## Review Git changes

Use `diff` for worktree, index, revision, and range comparisons:

```sh
mire diff
mire diff --staged
mire diff HEAD~3..HEAD
mire diff main...HEAD
```

Mire invokes Git directly without a shell. It reads the repository and Git
metadata but does not edit files, stage changes, create commits, or change Git
configuration.

Use `show` for one commit. It defaults to `HEAD`:

```sh
mire show
mire show HEAD~2
mire show HEAD -- crates/core
```

## Review a patch

Open a patch saved on disk:

```sh
mire patch changes.diff
```

Mire also reads patch text from standard input. Since the interactive viewer
needs standard input for keyboard events, piped patches produce JSON:

```sh
git diff --no-color | mire patch - > changeset.json
```

Redirecting output also selects JSON automatically.

## Review a GitHub pull request

Mire does not talk to GitHub. Let [`gh`](https://cli.github.com) fetch the pull
request as a patch, and pass it to Mire as a file, so standard input stays free
for the viewer:

```sh
mire patch <(gh pr diff 123)
mire patch <(gh pr diff 123 --patch)
```

`gh pr diff` prints the pull request's net diff. `gh pr diff --patch` prints
one patch per commit, as `git format-patch --stdout` does. Mire reads that
series as one net diff: it skips each commit's headers and message and folds
the commits' changes to each file together, so every file appears once, as it
would in the plain diff. A file that one commit adds and a later commit deletes
does not appear. Mire rejects a series whose commits do not apply on top of
each other.

The same works for any `git format-patch` series:

```sh
mire patch <(git format-patch --stdout main..HEAD)
```

## Override syntax detection

Mire detects syntax from file names, extensions, and supported shebangs. Apply
one language to every text file when detection is wrong:

```sh
mire diff --language typescript
mire patch changes.diff --language plain
```

Supported values include Rust, Python, JavaScript, TypeScript, TSX, JSON, YAML,
TOML, Markdown, HTML, CSS, shell, and plain text. Their common extensions and
short aliases are accepted.

## Select a theme

Use `--theme auto`, `iceberg`, `eldritch`, or `catppuccin`. Mire adapts each
family to the terminal's color support. Set
[`NO_COLOR`](https://github.com/jcs/no_color) to disable color output.
