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

Mire's durable review model has four primary objects:

```text
Changeset   source, files, hunks, fingerprint
Anchor      side, path, line range, hunk and content fingerprints
ReviewNote  anchor, author, severity, kind, status, body, provenance
Review      captured changeset, source binding, revision, notes, and note events
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
