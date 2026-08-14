---
title: Changesets
description: Understand how Mire normalizes Git and patch sources.
section: Concepts
group: Concepts
order: 6
---

Mire converts every input into a normalized changeset before the TUI, review
file, or structured output sees it. A changeset records its source, files,
hunks, lines, and a stable fingerprint.

## One model for every source

Git worktrees, staged changes, revision comparisons, commits, and unified patch
files share the same representation. This keeps file and hunk navigation,
syntax highlighting, review anchors, and output formats independent from the
input adapter.

Native Git owns Git semantics. Mire invokes Git without a shell, bounds command
output, then parses the resulting patch. It does not reimplement revision
resolution or repository rules.

## Paths and text

Paths remain repository-relative byte strings inside the core model. Display
conversion happens at the UI boundary so a path that is not valid UTF-8 can
still be represented without silently changing its identity.

Mire reviews textual languages through the same model. Syntax highlighting is
a presentation choice, not part of changeset identity. Unsupported languages
fall back to plain text.

## Fingerprints

A changeset fingerprint identifies normalized review input. Hunks and selected
content also carry fingerprints used by durable review anchors.

Line numbers are locations, not durable identity. When code moves, matching
content and hunk evidence are more useful than treating the previous line
number as authoritative.

## Structured output

Use `--format json` to emit the normalized changeset for scripts and tools:

```sh
mire diff main...HEAD --format json
mire patch changes.diff --format json
mire review review.json --format json
```

Redirecting output selects JSON automatically where the command supports it.
The schema includes an explicit version so consumers can reject unsupported
major versions.
