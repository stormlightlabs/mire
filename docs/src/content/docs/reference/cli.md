---
title: CLI Manual
description: Command and option reference for Mire.
section: Reference
group: Reference
order: 8
---

The `mire` command requires a subcommand. Pass `--theme` before the subcommand
to select `auto`, `iceberg`, `eldritch`, or `catppuccin`.

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

Without a structured format, this opens the review in the TUI.

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

## Interactive controls

See the [keyboard reference](/docs/reference/keybindings/) for navigation,
search, display, review-note, editor, filter, and mouse controls.

## Exit status

Argument parsing uses exit status `2`. Input I/O, patch parsing, output,
terminal, Git, review-file, and protocol failures use distinct nonzero statuses
so scripts can separate malformed input from repository and protocol errors.
Use the JSON protocol output for machine-readable failure details where
available.
