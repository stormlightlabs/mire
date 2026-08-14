---
title: Review changes
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
