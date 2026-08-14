---
title: Review Notes
description: Create, edit, filter, and disposition anchored notes in a Mire review file.
section: Guides
group: Guides
order: 4
---

A Mire review file stores a captured changeset with anchored notes, authorship,
provenance, and note history. Open one with:

```sh
mire review review.json
```

## Create and edit notes

Press `c` on a source row to create a note. To cover a range, press `v`, move
with `j` or `k`, then press `c`. The editor records the note body, severity, and
annotation kind.

| Key                 | Action                                     |
| ------------------- | ------------------------------------------ |
| `c`                 | Create a note on the row or selected range |
| `e`                 | Edit the selected note                     |
| `Enter`             | Save the note                              |
| `Tab` / `Shift-Tab` | Change severity or kind while editing      |
| `Ctrl-S`            | Retry a failed save                        |

Mire saves changes by atomically replacing the review file. If a write fails,
the editor remains open with the note text intact.

## Navigate and filter notes

Use `p` and `P` to move to the next or previous visible note. Press `f` to
filter by author, status, severity, kind, or file.

Right-clicking a source row opens note creation. Note rows also expose actions
for editing and status changes when mouse input is available.

## Record a decision

A note can remain open or receive a human disposition:

| Key | Disposition |
| --- | ----------- |
| `r` | Resolve     |
| `d` | Dismiss     |
| `a` | Accept risk |
| `o` | Reopen      |

Imported machine findings retain their producer identity. Import does not turn
a machine finding into a human decision.

## Exchange notes with tools

List or export notes without opening the TUI:

```sh
mire notes list review.json
mire notes export review.json --format markdown
mire notes export review.json --format json
```

`notes import` accepts a schema-versioned note batch and validates the complete
transaction before replacing the review file:

```sh
mire notes import review.json findings.json
```

See the [review model](/docs/concepts/review-model/) for anchors, provenance,
and status history.
