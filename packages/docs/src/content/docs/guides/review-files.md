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

## Complete a review

Capture the comparison, install the bundled skill, and leave the review open
while an agent works. Give the agent the path printed by `mire skill path` and
the review path.

```sh
mire review init review.json main...HEAD -- src tests
mire skill path
mire review review.json --watch
mire context review.json
mire context review.json --file src/example.rs --max-bytes 30000
mire notes apply review.json --stdin < findings.json
```

After source edits, refresh the capture. A human can then record a disposition
against the revision they observed. Export the completed review in either
format.

```sh
mire review refresh review.json
mire review status review.json
mire note resolve review.json note-1 --revision 3 --author reviewer
mire notes export review.json --format json > review-notes.json
mire notes export review.json --format markdown > review-notes.md
```

The JSON export includes finding authorship, provenance, events, original
anchors, and re-anchor outcomes. The Markdown export renders those details for
reading. `mire review status review.json` prints a compact progress report; add
`--format json` for structured output.

## Export the captured patch

Write the captured changeset as a Git-compatible patch for a tool or another
worktree:

```sh
mire review export review.json --format patch --output changes.patch
git apply --check changes.patch
```

Omit `--output` to write the patch to standard output. Mire serializes files and
hunks in its normalized order. The export preserves text changes, modes,
renames, copies, byte paths, CRLF content, and missing-final-newline markers.
It cannot reproduce the original diff byte stream, header spelling, or Git blob
IDs.

Mire does not retain binary payloads. If a review includes a binary change,
patch export names the affected files and fails before writing standard output
or replacing `--output`.

## Create and edit notes

Press `c` on a source row to create a note. To cover a range, press `v`, move
with `j` or `k`, then press `c`. The editor records the note body, severity, and
annotation kind.

| Key                 | Action                                         |
| ------------------- | ---------------------------------------------- |
| `c`                 | Create a note on the row or selected range     |
| `e`                 | Edit the selected note                         |
| `Enter`             | Insert a newline in the note body              |
| `Ctrl-Enter`        | Save the note                                  |
| `Tab` / `Shift-Tab` | Move field focus forward or backward           |
| `Up` / `Down`       | Change the focused severity or annotation kind |
| Paste               | Insert text into the focused note body         |
| `Ctrl-S`            | Retry a failed save                            |

Mire saves changes by atomically replacing the review file. If a write fails,
the editor remains open with the note text intact.

## Refresh changed code

Reviews created with `mire review init` retain their Git comparison. Refresh the
capture after editing the source:

```sh
mire review refresh review.json
```

Each finding receives an `exact`, `moved`, `stale`, or `ambiguous` result. Mire
moves a finding only when its path, selected content, and nearby context leave
one candidate. Duplicate matches will remain ambiguous.

JSON and Markdown exports retain authorship, provenance, decision events, the
initial anchor, current candidates, and match evidence.

A refresh advances the review revision only when the captured changeset changes.

The write validates and replaces the complete review, so a failed match or concurrent
note update cannot leave a partly refreshed file.

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
them and to refresh when the bound Git source changes. Mire installs the skill in the standard
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
mire review export review.json --format patch
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
