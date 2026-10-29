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

## Generated files

Mire reads the repository's attributes when it captures a Git comparison. A
file is generated when its `linguist-generated` attribute is set, the
attribute GitHub reads to collapse a file:

```text
# .gitattributes
Cargo.lock linguist-generated
dist/** linguist-generated=true
*.svg -diff
docs/api.md -linguist-generated
```

A file whose `diff` attribute is unset (`-diff`, or the `binary` macro)
counts as generated too, unless `-linguist-generated` or
`linguist-generated=false` says otherwise. Git resolves the attributes, so
nested `.gitattributes` files, `.git/info/attributes` and
`core.attributesFile` apply as they do in Git.

The attributes come from the new side of the comparison. A worktree
comparison reads the worktree, `--staged` reads the index, and a comparison
whose new side is a revision, such as `A..B`, `A...B`, `A B` or `mire show
REV`, reads that revision's tree. Reading a tree needs Git 2.40 or later;
older Git reads the worktree's attributes instead. The marks come from the
local Git and its settings, so another Git version or `core.attributesFile`
can mark different files. When Git cannot report the attributes, Mire
captures the comparison with no file marked.

The terminal file list shows `gen` after a generated file's path, and the
browser lists generated files in a collapsed group and hides each one's diff
until you select Show diff. Selecting a finding on a generated file opens
its diff at the finding. Mire still captures a generated file's hunks, and
notes and viewed marks work on it as on any file. Patch input carries no
attributes, so `mire patch` marks nothing. A review refresh reads the
attributes again, so an attribute change, or a review created before Mire
marked files, shows its marks after the next refresh. A refresh that changes
only the marks keeps the review revision.

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
the commits' changes to each file together, so every file appears once. A file
that one commit adds and a later commit deletes does not appear. Mire rejects a
series whose commits do not apply on top of each other.

The folded diff has the same content as the plain diff, but a file's status can
differ:

- A file renamed and then re-added under its old path shows as a rename plus an
  added file. The plain diff shows the new path as added and the old path as
  modified.
- A binary file that one commit changes and a later commit reverts shows as
  modified. A patch does not carry binary content, so Mire cannot tell that the
  file is back to its base. The plain diff leaves it out.

Pass the pull request's title and description too, so they show in the review
header. The description dialog opens with `i`:

```sh
mire patch <(gh pr diff 123) \
  --title "$(gh pr view 123 --json title -q .title)" \
  --description-file <(gh pr view 123 --json body -q .body)
```

`mire review init` takes the same `--title` and `--description-file` and
stores both in the review file, where `mire context` hands them to agents.

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

## Read one file in the browser

The browser review that `mire serve` opens has a focus mode, which shows the
open file at full width with the file tree and review queue hidden. Press `b`
or select Focus in the header to enter it. Press `Esc` or `b`, or select Leave
focus, to return.

The focus bar names the file, its status and line count, its place in the file
tree's order, such as "3 of 6", and its Viewed box. An added or deleted file
reports how many lines it has; any other file reports the lines added and
removed. Press `[` and `]`, or use the arrows beside the path, to open the
previous or next file. A generated file keeps its Show diff button.

Focus mode lasts until you leave it or reload the page. The server stores
nothing about it, so each tab has its own.

## Select a theme

Use `--theme auto`, `iceberg`, `eldritch`, or `catppuccin`. Each family has a
dark and a light palette:

| Theme        | Dark             | Light            |
| ------------ | ---------------- | ---------------- |
| `iceberg`    | Iceberg Dark     | Nord Light       |
| `eldritch`   | Eldritch Minimal | Eldritch Dusk    |
| `catppuccin` | Catppuccin Mocha | Catppuccin Latte |
| `auto`       | Eldritch Minimal | Nord Light       |

The terminal picks the palette that matches its background, and uses the dark
one when it cannot tell.

`mire serve` takes the same flag. The browser review follows the browser's
light or dark color scheme, and the header button switches between the two.
The browser saves the button's choice for the server's address and stops
following the color scheme. The review has no control to undo this: to follow
the scheme again, clear the site data for that address, or remove the
`mire:theme` key from its local storage.

Set [`NO_COLOR`](https://github.com/jcs/no_color) to disable terminal color.
A terminal with `TERM=dumb` gets a basic ANSI theme.
