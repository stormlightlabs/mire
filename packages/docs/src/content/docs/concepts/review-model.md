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

Mire's durable review model has five primary objects:

```text
Changeset    source, files, hunks, fingerprint
Anchor       side, path, line range, hunk and content fingerprints
ReviewNote   anchor, author, severity, kind, status, body, provenance
Walkthrough  optional overview, ordered steps, author, provenance
Review       captured changeset, source binding, optional title and
             description, revision, notes, note events, optional walkthrough
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
supplied, so the steps have no cycles. Walkthrough writes cannot claim human
provenance.

A refresh re-anchors every step anchor with the note rules and records the
same `exact`, `moved`, `stale`, or `ambiguous` result on that anchor. A hunk is
covered when an exact or moved step anchor overlaps it. Stale and ambiguous
anchors cover nothing. Files with no hunks, such as binary, rename-only, and
mode-only files, are never covered.

A step anchor that no longer matches the stored capture when the review loads
is marked `stale`. This happens when a Mire release without walkthroughs
refreshes the review, because it keeps the walkthrough without re-anchoring
it.

A walkthrough has these limits:

| Bound                                    | Limit  |
| ---------------------------------------- | ------ |
| Steps                                    | 10,000 |
| Anchors across all steps                 | 10,000 |
| Overview, and each narrative and diagram | 1 MiB  |
| Step title, one line                     | 1 KiB  |

[Walkthrough design](/docs/concepts/walkthrough-design/) records where these
fields come from.

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
Review files written before titles or walkthroughs existed still load.

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
