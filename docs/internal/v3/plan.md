---
title: "MIRE V3 plan: forge and collaborative review integration"
status: "planned"
updated: "2026-07-14"
---

# MIRE V3 plan

V3 connects MIRE's local evidence ledger to hosted code-review forges. The local
review session remains canonical: forge identifiers locate remote artifacts but
never become finding identity, and no review is submitted without an explicit
user action.

## Objective

Let an engineer import pull-request intent and revisions, map local findings to
forge-native review locations, prepare and publish idempotent draft reviews,
explicitly submit them, synchronize discussion and dispositions, and compare
MIRE evidence with subsequent human and forge outcomes.

## Dependencies and current state

V3 depends on:

- V1 immutable snapshots, revision-aware findings, anchors, context-bound chat,
  human dispositions, provenance, and explicit operations;
- V2 multi-repository identity and policy isolation; and
- a provider-neutral forge boundary whose first concrete forge and credential
  flow are approved before implementation.

Forge state augments the local ledger. It does not replace local Git comparison
semantics, snapshots, finding IDs, evidence, discussion, or export.

Implementation sequencing lives in [the V3 task list](task.md). Provider and
authentication decisions must be recorded here before their implementation
ticket proceeds beyond its approval checkpoint.

## User outcomes

An engineer can:

1. Select a forge change and create a local review round from its exact intent
   and revisions without confusing remote references with canonical Git input.
2. See whether each local anchor can be represented as a forge
   commit/path/side/range coordinate, with ambiguous and unmappable anchors shown
   explicitly.
3. Prepare or update a remote draft idempotently, inspect its exact comments,
   and submit it only through a separate confirmation.
4. Continue a review across local rounds and remote updates without duplicating
   replies, notifications, or findings.
5. Compare local candidates and evidence with human comments, resolutions, and
   final forge outcomes without treating correlation as proof of quality.

## Product contract

### Import

- Imported intent, base/head revisions, merge-base provenance, author/context,
  and remote identifiers are recorded separately from the immutable local
  snapshot.
- V1's committed comparison semantics determine what is reviewed. Missing,
  changed, or unavailable remote revisions fail explicitly rather than silently
  substituting the current branch or worktree.
- When the selected provider supports authenticated revision acquisition, MIRE
  may fetch the exact required objects into private application storage after
  verifying repository identity and object IDs and enforcing resource limits. It
  must not write refs, objects, remotes, or other metadata into the target
  repository. If exact acquisition is unsupported or verification fails, import
  fails explicitly.
- Remote descriptions, comments, and metadata are untrusted context subject to
  the same policy and prompt-injection rules as repository content.

### Anchor mapping

- A mapping records the exact local finding revision and anchor plus forge,
  repository, commit, path, side, line/range, and mapping status.
- Mappable, ambiguous, outdated, unsupported, and unmappable states are distinct.
- Re-mapping creates new provenance; it does not rewrite the original anchor or
  finding identity.

### Publication

- Draft construction is deterministic from explicit publishable comment
  revisions and mapping results.
- Creating or updating a remote draft is idempotent across retries, refreshes,
  and process restarts.
- Submission is a separate explicit user action. Models, analyzers, imported
  repository instructions, and background synchronization cannot submit.
- Partial failures are recoverable and visible. MIRE does not duplicate comments
  or notifications to approximate a failed transaction.

### Synchronization and learning

- Remote replies, threads, review states, outdated markers, and supported
  dispositions synchronize through durable remote-object mappings and cursors or
  equivalent provider state.
- Remote discussion without a validated finding or diff-anchor mapping is stored
  as a separate remote-discussion record. It enters MIRE's chat timeline only
  after satisfying V1's mandatory context binding.
- Replayed or reordered remote data cannot duplicate a local event or notification.
- Local and remote state conflicts remain visible and do not silently change the
  machine-verification record or human disposition.
- Outcome comparison is an evaluation view. It may measure agreement, missed
  issues, false positives, latency, and disposition outcomes, but it cannot infer
  correctness from forge approval or finding count.

## Success criteria

V3 is complete when at least one explicitly selected forge adapter can:

- import exact pull-request intent and revisions into a repository-scoped local
  round without changing MIRE's canonical Git semantics;
- map supported anchors and expose every unsupported, ambiguous, or outdated
  location;
- create and retry a draft review without duplicate comments or submission;
- submit only after explicit confirmation and recover accurately from partial or
  uncertain remote responses;
- synchronize replies and review/disposition states without event or notification
  duplication and without using forge IDs as MIRE identity; and
- produce a provenance-rich comparison between local evidence and later human or
  forge outcomes.

All V1 and V2 local-only workflows remain usable when offline, unauthenticated,
or disconnected from a forge.

## Test and verification boundaries

The stable boundary is the compiled CLI and real local web server against fixture
Git repositories and a deterministic fake forge service. Contract tests cover
authentication failure, missing revisions, permission denial, pagination, rate
limits, timeouts, retries, uncertain writes, duplicate/reordered events, outdated
anchors, partial publication, restart recovery, and credential redaction. Default
tests never require live forge credentials or send real notifications.

Established checks remain:

```text
go build ./cmd/mire
go test ./...
go test -race ./...
pnpm --dir app exec playwright install chromium
pnpm --dir app check
pnpm --dir app lint
pnpm --dir app test:unit --run
pnpm --dir app build
pnpm --dir app exec playwright test
```

A human release review verifies permission scopes, credential handling, draft
preview fidelity, explicit submission, notification behavior, and provider terms.

## Security and implementation boundaries

### Always

- Request the least forge permissions needed for the selected operation.
- Keep credentials private, redact them from logs, and exclude them from every
  export and repository-local representation.
- Validate remote repository identity and revision provenance before import,
  mapping, publication, or synchronization.
- Preserve idempotency and audit records across retries and restarts.

### Ask first

- Approve the first forge, authentication flow, credential store, permission
  scopes, synchronization trigger, and remote API dependencies.
- Approve support for any additional forge or remotely hosted MIRE service.

### Never

- Submit a review, send a comment, resolve a thread, or alter remote state because
  a model, analyzer, repository instruction, import, or background refresh asks.
- Use forge approval, merge state, or remote identifiers as proof of correctness
  or as MIRE finding identity.
- Make local review dependent on network access or valid forge credentials.

## Risks and open decisions

- The first forge and authentication/credential-storage approach are not yet
  selected.
- Forge APIs differ in anchor semantics, draft behavior, idempotency support,
  notification timing, rate limits, and representation of outdated discussions.
- A timeout after a remote write creates an uncertain outcome that must be
  reconciled before retrying.
- Force-pushes and rebases can invalidate both imported revisions and remote
  coordinates without invalidating the historical local finding.
- Synchronization direction, cadence, and conflict UX require explicit design;
  this plan does not imply a daemon or webhook receiver.

## Deferred work

- Additional forge adapters follow the first only after the common boundary is
  proven.
- A shared hosted MIRE service, organization-wide policy administration, webhook
  infrastructure, and autonomous review publication require separate threat and
  tenancy models.
- Navigation telemetry, full SARIF import, and broader benchmark research remain
  later product-hardening work.
