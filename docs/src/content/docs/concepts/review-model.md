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
Review      captured changeset, revision, notes, and note events
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

A note import validates the entire transaction before Mire atomically replaces
the review file. Failed validation leaves the previous file unchanged.

## Context for agents

`mire context` exports a bounded projection of a review:

```sh
mire context review.json
mire context review.json --file src/lib.rs --max-bytes 20000
mire context review.json --patch --max-bytes 50000
```

The default manifest is compact. Complete file or patch expansion requires an
explicit byte limit so callers choose the amount of code they are prepared to
consume.
