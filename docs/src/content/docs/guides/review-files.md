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

## Hand a review to an agent

Print the path to Mire's bundled, provider-neutral review skill:

```sh
mire skill path
```

Give that skill path and the review file path to the agent. Keep the review open
with `mire review review.json --watch` to see findings as the agent applies
them. Mire installs the skill in the standard
`$HOME/.agents/skills/mire` directory.

The skill starts with the context manifest, expands only named context with an
explicit byte limit, and submits location-based findings in one atomic batch.
It does not depend on a model provider or agent runtime.

## Exchange notes with tools

List or export notes without opening the TUI:

```sh
mire notes list review.json
mire notes export review.json --format markdown
mire notes export review.json --format json
```

Agents and analyzers should start with the context manifest, then expand only
the hunk or file they need:

```sh
mire context review.json
mire context review.json --hunk HUNK_FINGERPRINT --max-bytes 20000
mire notes apply review.json --stdin < findings.json
```

A location batch includes the manifest's `review_revision`. Each request names a
file, side and inclusive range, author, non-human provenance, severity, kind,
and body. Mire validates every location and writes none if any request fails.
It also assigns note identifiers and computes anchor fingerprints.

`notes import` remains available for clients that already construct complete
notes. It also requires the revision read by the client:

```sh
mire notes import review.json findings.json --revision 4
```

See the [review model](/docs/concepts/review-model/) for anchors, provenance,
and status history.
