---
title: CLI Manual
description: Command and option reference for Mire.
section: Reference
group: Reference
order: 8
---

Run `mire` with no subcommand to open the current worktree diff. Pass `--theme`
before a subcommand, or on its own with the default diff, to select `auto`,
`iceberg`, `eldritch`, or `catppuccin`.

## Git commands

### `mire diff`

Review unstaged and untracked worktree changes by default.

```text
mire diff [--staged] [REVISION]... [--format json] [--language LANGUAGE] [--watch] [-- PATH]...
```

`--staged` compares the index with `HEAD` and cannot be combined with revision
arguments. One or more revision arguments are passed to Git as a comparison.
Paths after `--` are repository-relative filters.

### `mire show`

Review one commit. The revision defaults to `HEAD`.

```text
mire show [REVISION] [--format json] [--language LANGUAGE] [--watch] [-- PATH]...
```

### `mire watch`

Open a watched Git comparison.

```text
mire watch [--staged] [REVISION]... [--language LANGUAGE] [-- PATH]...
```

## Patch and review commands

### `mire patch`

```text
mire patch PATH|- [--format json] [--language LANGUAGE] [--watch]
```

`-` reads a patch from standard input and produces JSON. `--watch` works only
with a file-backed patch in an interactive terminal.

### `mire review`

```text
mire review REVIEW.json [--format json] [--watch]
```

Without a structured format, this opens the review in the TUI. Initialize or
refresh a source-backed review with:

```text
mire review init REVIEW.json [--staged] [REVISION]... [-- PATH]...
mire review refresh REVIEW.json
mire review status REVIEW.json [--format json]
mire review export REVIEW.json --format patch [--output PATH]
```

`review refresh` repeats the recorded Git comparison, re-anchors every finding,
and atomically replaces the file. It prints `status: unchanged` without writing
when the capture fingerprint has not changed. Reviews without a source binding
cannot be refreshed.

`review status` reads and validates a review without opening the TUI. Its text
output reports the captured source, review revision, changes, finding
dispositions, and re-anchor results. Pass `--format json` for deterministic
structured output.

`review export --format patch` writes the captured text changeset without review
notes or decisions. It writes to standard output by default, or atomically
replaces `PATH` with `--output`. The patch preserves normalized text changes,
modes, renames, copies, byte paths, CRLF content, and missing-final-newline
markers. It does not preserve the original diff byte stream or Git blob IDs.
Binary changes fail before any output is written.

### `mire context`

Export bounded context from a durable review:

```text
mire context REVIEW.json [--file PATH | --hunk FINGERPRINT | --patch] [--max-bytes BYTES] [--format json]
```

The compact manifest does not require `--max-bytes`. File, hunk, and patch
expansion does.

## Note commands

```text
mire note add REVIEW.json --revision REVISION --file PATH (--old-line LINE | --new-line LINE) [--end-line LINE] --author ID --provenance agent|analyzer|interchange --producer NAME --severity SEVERITY --kind KIND --body BODY
mire notes apply REVIEW.json --stdin
mire note resolve|dismiss|accept-risk REVIEW.json NOTE_ID --revision REVISION --author ID
mire notes import REVIEW.json BATCH.json|- --revision REVISION
mire notes list REVIEW.json [--format json]
mire notes export REVIEW.json [--format json|markdown]
```

`notes apply` accepts location-based findings and creates identifiers and anchor
fingerprints. It validates the whole batch before writing. Every mutation
requires the revision returned by `mire context` or `mire notes list`; stale
writes fail with `revision_conflict`. Full-note import remains available for
compatible clients. List returns deterministic JSON. Export can produce JSON
or standalone Markdown.

## Agent skill

```text
mire skill path
```

Installs the bundled provider-neutral review skill in the standard
`$HOME/.agents/skills/mire` directory when needed, then prints its `SKILL.md`
path.

## Live sessions

```text
mire session list
mire session inspect SESSION
mire session focus SESSION --note NOTE_ID
mire session focus SESSION --file PATH --side old|new --start-line LINE [--end-line LINE]
mire session next|previous|reload SESSION
mire session walkthrough start|next|previous|stop SESSION
```

These commands control presentation state in a local open TUI. They do not
create, edit, or disposition findings. See the
[live-session protocol](/docs/reference/live-session-protocol/) for response
format, authentication, limits, and transport behavior.

## Interactive controls

See the [keyboard reference](/docs/reference/keybindings/) for navigation,
search, display, review-note, editor, filter, and mouse controls.

## Exit status

Argument parsing uses exit status `2`. Input I/O, patch parsing, output,
terminal, Git, review-file, and protocol failures use distinct nonzero statuses
so scripts can separate malformed input from repository and protocol errors.
Use the JSON protocol output for machine-readable failure details where
available.
