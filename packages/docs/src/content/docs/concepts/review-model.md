---
title: Review model
description: Learn how Mire stores anchors, findings, provenance, decisions, and history.
section: Concepts
group: Concepts
order: 7
---

The review artifact is separate from the changeset it captures. A changeset
describes code. A review stores findings and decisions about that code.

## Core objects

Mire's durable review model has seven primary objects:

```text
Changeset    source, files, hunks, fingerprint
Anchor       side, path, line range, hunk and content fingerprints,
             optional checkpoint
ReviewNote   anchor, author, severity, kind, status, body, provenance
Walkthrough  optional overview, ordered steps, author, provenance
ViewedMark   path, file fingerprint, revision
Checkpoint   revision, one row per changed file: path, fingerprint,
             blob ids, patch copy
Review       captured changeset, source binding, optional title and
             description, revision, notes, note events, optional walkthrough,
             viewed marks by reader, checkpoints, pinned rows
```

The TUI and CLI use the same validation rules. Tools can therefore add findings
through the JSON protocol and people can inspect or disposition them in the
terminal.

## Anchors

Callers identify a path, diff side, and source range. Mire resolves the
fingerprints needed for a durable anchor and rejects missing or ambiguous
locations.

An anchor contains the original location and evidence tying the note to the
reviewed content. This avoids treating a mutable line number as permanent
identity.

A note's anchor also names its `checkpoint`: the revision and path of the
checkpoint row that held the file when the note was added. Re-anchoring does
not change it. Notes written before checkpoints have none.

## Re-anchoring

A review initialized from Git stores the repository identity and comparison
needed to capture it again. Mire validates that identity before every refresh.
Moving, replacing, or deleting the repository returns an error rather than
reading a different source.

A refresh classifies each finding:

- `exact`: the complete prior anchor still exists;
- `moved`: path, selected content, and nearby context identify one candidate;
- `stale`: no supported candidate exists;
- `ambiguous`: several candidates have equal support.

Mire never chooses an ambiguous candidate. The note retains its initial anchor,
each candidate anchor, and the evidence used for the result, keeping the data
available in review JSON and note exports.

## Walkthroughs

A walkthrough explains a change in the order a reader should take it. A review
holds at most one. It has an optional Markdown overview, one or more steps, and
an author and provenance, as a note does. Each step has an `id`, a one-line
`title`, a Markdown `narrative`, one or more anchors, optional `depends_on`
step ids, and an optional Mermaid `diagram`.

Mire keeps the steps in the order the agent wrote them. A write names the review
revision it read, gives each step a path, side, and line range, and replaces the
whole walkthrough. Mire resolves each location to an anchor with the note rules.
A write against an older revision fails with `revision_conflict`. Supplied step
ids are kept. A step id, like a title, is one line with no control characters. A
step without one gets `step-<n>`, where `n` is its position, counting up past
ids already taken. `depends_on` may name only an earlier step whose id the agent
supplied, and may name each step once, so the steps have no cycles. A repeated
id fails with `duplicate_step_dependency`. Walkthrough writes cannot claim human
provenance.

A refresh re-anchors every step anchor with the note rules and records the
same `exact`, `moved`, `stale`, or `ambiguous` result on that anchor. A hunk is
covered when an exact or moved step anchor overlaps it. Stale and ambiguous
anchors cover nothing. Files with no hunks, such as binary, rename-only, and
mode-only files, are never covered.

A step anchor whose path, hunk, or line range is missing from the stored
capture when the review loads is marked `stale`. An ambiguous anchor drops only
the candidates that are missing, and stays ambiguous while two remain. This
happens when a Mire release without walkthroughs refreshes the review, because
it keeps the walkthrough without re-anchoring it. The change is made in memory;
the next write saves it, as a refresh would, and keeps the original anchor. Any
other mismatch, such as a changed content fingerprint, fails the load as it does
for a note. A file whose anchors all match loads unchanged.

A walkthrough has these limits:

| Bound                                    | Limit  |
| ---------------------------------------- | ------ |
| Steps                                    | 10,000 |
| Anchors across all steps                 | 10,000 |
| Overview, and each narrative and diagram | 1 MiB  |
| Step title, one line                     | 1 KiB  |

A whole review file is at most 128 MiB. Mire refuses to write a review larger
than that, such as one with a very large walkthrough, and leaves the file as it
was, so every file it writes can be read back.

[Walkthrough design](/docs/concepts/walkthrough-design/) records where these
fields come from.

## Checkpoints

A checkpoint records a revision where at least one file's fingerprint changed.
The first revision records every file. Each refresh that changes a file records
a row for it: its path, its new fingerprint (`null` once the file leaves the
changeset), and the Git blob ids of its two sides. To find a file at revision
R, Mire takes the latest checkpoint at or before R that names the path.

When the bound repository does not hold a changed file's blobs, as for an
unstaged worktree file, the row keeps a patch copy of the file's hunks. A copy
may take 1 MiB and all history 16 MiB. Past that budget Mire folds the oldest
checkpoint into the next one; rows a note points at move to `pinned` with their
copies. Mire never writes objects into the repository.

`mire file history` lists the revisions that changed a file and prints its
hunks at one of them, from the current capture, from Git blobs, or from the
copy. When none of these holds them, it reports `checkpoint_unavailable`.

A review file written before checkpoints loads with a baseline checkpoint at
its current revision. A file that an older release refreshed loads with a
`gap` checkpoint holding the files that changed in between.

## Viewed marks

A review's `viewed` map holds, for each reader id, the files that reader marked
viewed. Each mark records the file's path, the file fingerprint the reader saw,
and the review revision they read it at. A file is viewed for a reader while
the mark's fingerprint equals the file's current fingerprint. After a refresh
changes the file, the mark keeps the older fingerprint and the file shows as
changed since viewed. A mark for a path that left the changeset stays until the
reader clears it.

A mark names the fingerprint the reader saw and the revision they read it at.
Mire looks up the file at that revision in the checkpoints and rejects the mark
with `viewed_mismatch` when the fingerprints differ or history does not reach
that revision. Marks do not advance the revision.
Every other write takes the marks from the file on disk, read under the review
file lock, so a refresh or a note write prepared before a mark landed keeps
that mark. A reader may hold 10,000 marks, and a review may hold marks for 100
readers. Review files written before viewed marks load with none.

Mark files from the terminal with `m`, from the browser with the Viewed button,
or with `mire file viewed` and `mire file unviewed`.
[Checkpoints and viewed state](/docs/concepts/checkpoints-and-viewed-state/)
records the design.

## Authorship and provenance

Every note records its author and producer provenance. Human, agent, analyzer,
and imported findings keep their original identity. Provenance says who made a
claim; it does not decide whether the claim is correct.

Resolve, dismiss, accept-risk, and reopen operations append note events. The
review retains the sequence of decisions rather than replacing history with
only the latest label.

## Validation and writes

Review schemas carry explicit versions and reject unsupported major versions.
Mire bounds review size, note count, imported payloads, and expanded context.
A review's title is one line of at most 1 KiB with no control characters.
Its description is at most 65,536 characters, GitHub's limit for a pull
request body, and carries no control characters other than newline and tab.
Review files written before titles, walkthroughs, viewed marks or checkpoints
existed still load.

A location batch and a source refresh validate the entire transaction before
Mire atomically replaces the review file.

Failed validation leaves the previous file unchanged.

Every mutation includes the revision its caller read, so a concurrent update
produces a `revision_conflict` instead of overwriting newer data.

A capture with the same fingerprint does not advance the revision or rewrite the file.

## Context for agents

`mire context` exports a bounded projection of a review:

```sh
mire context review.json
mire context review.json --file src/lib.rs --max-bytes 20000
mire context review.json --hunk HUNK_FINGERPRINT --max-bytes 20000
mire context review.json --patch --max-bytes 50000
```

The default manifest is compact so file, hunk, and patch expansion requires an
explicit byte limit so callers choose the amount of code they are prepared to consume.
