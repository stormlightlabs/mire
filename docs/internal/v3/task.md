---
title: "MIRE V3 implementation tasks"
status: "blocked by V2"
updated: "2026-07-14"
source: "plan.md"
---

# MIRE V3 tasks

These tickets implement [the V3 plan](plan.md). Work the frontier: a ticket is
ready only when every listed blocker is complete. Forge integration augments the
local ledger; no ticket may replace local snapshot, finding, anchor, evidence, or
repository identity with a provider representation.

## Milestone 1: Provider boundary and local-first import

**Exit criterion:** One approved forge can import exact change intent and
revisions into a local round with durable provenance and no silent revision
substitution.

### V3-01: Approve the forge contract and first provider

**What to build:** Select the first forge and authentication flow, then define a
small provider-neutral boundary for identity, revisions, intent, anchor mapping,
draft publication, submission, discussion, synchronization, and rate/error state.

**Blocked by:** V2-14.

**Acceptance criteria:**

- [ ] The decision records provider capabilities, least permission scopes,
      credential storage/redaction, notification behavior, and API limitations.
- [ ] Forge DTOs cannot become core repository, finding, anchor, discussion, or
      disposition types.
- [ ] Remote reads and writes are explicit operations with provenance,
      cancellation, retries, and uncertain-outcome states.
- [ ] Local-only use remains valid with the adapter disabled.

**Verification:** Human architecture/security review and contract fixtures for
the selected provider's documented behavior.

### V3-02: Import pull-request intent and exact revisions

**What to build:** Let a user select a remote change, inspect its identity and
resolved revisions, and explicitly create a local snapshot and review round with
the imported intent attached as untrusted context.

**Blocked by:** V3-01.

**Acceptance criteria:**

- [ ] Import records forge/repository/change identity, requested and resolved
      revisions, author/context provenance, and retrieval time.
- [ ] Local V1 comparison and snapshot rules remain authoritative.
- [ ] Authenticated acquisition, when supported, writes exact verified objects
      only to private MIRE storage with bounded resources and never mutates target
      Git metadata or refs.
- [ ] Missing, moved, ambiguous, or inaccessible revisions fail visibly before
      review rather than falling back to the current branch or worktree.
- [ ] Repeating an import is idempotent or creates an explicit new round when the
      remote revision changed.

**Verification:** Run `go test ./...` against fake-forge fixtures for successful,
missing, force-pushed, unauthorized, paginated, rate-limited, and offline imports.

## Milestone 2: Anchor translation and draft publication

**Exit criterion:** Mappable local comments can be previewed and published as an
idempotent remote draft, while every unmappable or stale location remains visible.

### V3-03: Translate local anchors into forge coordinates

**What to build:** Map an exact local finding revision and anchor to the selected
forge's commit, path, side, line/range, and current mapping status.

**Blocked by:** V3-02.

**Acceptance criteria:**

- [ ] Mapping handles added, deleted, renamed, and modified files plus supported
      single- and multi-line ranges.
- [ ] Mappable, ambiguous, outdated, unsupported, and unmappable results are
      distinct and retain their reasoning/provenance.
- [ ] Re-mapping never rewrites the historical local anchor or finding identity.
- [ ] CLI and browser previews match the exact remote payload location.

**Verification:** Run `go test ./...` with provider coordinate fixtures for each
file/range state and a force-pushed revision.

### V3-04: Create and update idempotent draft reviews

**What to build:** Build a draft from explicitly selected publishable comment
revisions, preview its exact payload, and create or update it remotely without
submitting or duplicating comments.

**Blocked by:** V3-03.

**Acceptance criteria:**

- [ ] Unmappable comments are excluded with visible reasons, never silently
      relocated.
- [ ] A stable local publication record maps each selected comment revision to
      its remote draft object and request outcome.
- [ ] Retries, restarts, and repeated clicks do not create duplicate remote
      comments or drafts.
- [ ] Partial and uncertain writes are reconciled before another write attempt.
- [ ] Draft creation cannot submit the review or imply approval.

**Verification:** Run `go test -race ./...` against success, timeout-before-write,
timeout-after-write, partial failure, retry, restart, and duplicate-request
fixtures.

### V3-05: Submit a draft only through explicit confirmation

**What to build:** Show the final remote draft, mappings, exclusions, and expected
notification effect, then submit that exact draft through a separately confirmed
operation.

**Blocked by:** V3-04 and human approval of the submission/notification UX.

**Acceptance criteria:**

- [ ] Confirmation is explicit and scoped to the current draft revision,
      repository, change, and intended review action.
- [ ] A stale draft, changed remote head, permission change, or unresolved write
      outcome blocks submission.
- [ ] Models, analyzers, MCP clients, imports, and synchronization cannot confirm
      submission.
- [ ] Repeated or retried submission does not duplicate the review or
      notifications.

**Verification:** Run `go test ./...` and a browser flow for preview, cancel,
stale-head rejection, permission denial, uncertain response, retry, and successful
explicit submission.

## Milestone 3: Discussion and state synchronization

**Exit criterion:** Remote discussion and review state can round-trip without
duplicate events or loss of MIRE's independent identity and evidence axes.

### V3-06: Synchronize remote threads and replies idempotently

**What to build:** Import remote review threads and replies, map them to local
findings or anchors only when the binding validates, and otherwise retain them as
separate remote discussion.

**Blocked by:** V3-02 and V3-03.

**Acceptance criteria:**

- [ ] Remote object mappings and provider progress state survive restart.
- [ ] Pagination, replay, overlap, and reordered responses do not duplicate local
      messages, events, or notifications.
- [ ] Ambiguous correlation creates a visible possible relationship rather than
      reusing a finding ID.
- [ ] Unmappable general discussion remains a remote-discussion record; only a
      validated finding or diff binding can create a V1 chat message.
- [ ] Remote prose remains untrusted context and cannot directly change findings
      or permissions.

**Verification:** Run `go test -race ./...` with replayed, reordered, paginated,
edited, deleted, and duplicated thread fixtures.

### V3-07: Reconcile outdated and disposition states without identity loss

**What to build:** Show remote outdated, resolved, dismissed, and review-round
states alongside local machine verification and human disposition, with explicit
conflict handling.

**Blocked by:** V3-05 and V3-06.

**Acceptance criteria:**

- [ ] Remote state never overwrites machine verification or local disposition
      without a supported, explicit user action.
- [ ] Force-push, rebase, outdated anchor, reopened thread, and new review round
      preserve historical mappings and create new provenance.
- [ ] Local-to-remote state changes show their remote effect and require the
      permissions and confirmation defined by V3-01.
- [ ] Conflicts and unsupported remote states remain visible and recoverable.

**Verification:** Run `go test ./...` against force-push, reopen, resolve,
dismiss, stale-cursor, and conflicting local/remote state fixtures.

## Milestone 4: Outcome comparison and release hardening

**Exit criterion:** Users can inspect how local evidence related to later review
outcomes, and the complete import-to-sync workflow is reliable under provider
failure without regressing offline use.

### V3-08: Compare local evidence with human and forge outcomes

**What to build:** Provide an auditable comparison view linking local candidates,
verified findings, evidence, and dispositions to later human comments, fixes,
review decisions, and merge outcomes.

**Blocked by:** V3-07.

**Acceptance criteria:**

- [ ] Correlations preserve source, time, revision, mapping confidence, and
      ambiguity.
- [ ] The view reports agreement, missed issues, false positives, latency, and
      disposition outcomes without using finding count or forge approval as a
      correctness score.
- [ ] Comparisons do not silently turn historical outcomes into durable policy or
      model-training consent.
- [ ] Export declares remote provenance and format omissions without including
      credentials.

**Verification:** Run `go test ./...` against an adjudicated fixture containing
matched, unmatched, ambiguous, superseded, and contradicted outcomes.

### V3-09: Prove V3 idempotency, security, and end-to-end behavior

**What to build:** Complete the provider contract matrix, credential and
permission review, failure/recovery tests, documentation, and browser workflow for
the V3 release.

**Blocked by:** V3-05, V3-07, and V3-08.

**Acceptance criteria:**

- [ ] Import, mapping, drafting, submission, and synchronization survive restart,
      retries, rate limits, pagination, reordering, and uncertain writes.
- [ ] No test scenario duplicates a remote comment, review, local event, or user
      notification.
- [ ] Credentials are absent from logs, errors, exports, repo-local state, and
      test snapshots.
- [ ] Local V1/V2 review remains fully usable offline and with the forge adapter
      disabled.
- [ ] Supported provider capabilities and limitations are documented without
      claiming parity with other forges.

**Verification:** Run `go build ./cmd/mire`, `go test ./...`,
`go test -race ./...`, all app checks from the plan, a fake-forge end-to-end
browser flow, and the human V3 security/publication checklist. No default
verification sends a real remote notification.

## Frontier

No V3 ticket is currently unblocked. V3-01 becomes the first ticket after V2-14
completes the V2 release gate. Selecting the first forge and approving its
authentication and permission model are part of that ticket's required outcome,
not assumptions to hide in later implementation.
