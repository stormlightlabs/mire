---
title: "MIRE V2 implementation tasks"
status: "blocked by V1"
updated: "2026-07-14"
source: "plan.md"
---

# MIRE V2 tasks

These tickets implement [the V2 plan](plan.md). Work the frontier: a ticket is
ready only when every listed blocker is complete. The present cross-version
frontier is the V1 release; V2 implementation must build on its released
repository, snapshot, persistence, operation, analyzer, and application-service
contracts rather than anticipating them.

## Milestone 1: Multi-repository workspace

**Exit criterion:** At least two repositories can be selected and used
concurrently through the CLI and browser without state or policy leakage, while
the V1 current-repository workflow still works.

### V2-00: Approve the multi-repository process topology

**What to build:** Decide and document how a local V2 process discovers, enables,
opens, and switches among repositories, including foreground lifecycle, local
authentication, operation ownership, and whether any background component is
actually necessary. Record the approved contract in `plan.md` before workspace
implementation begins.

**Blocked by:** V1-26.

**Acceptance criteria:**

- [ ] The decision preserves V1 foreground current-repository operation.
- [ ] Repository discovery never implies enablement or starts analysis.
- [ ] Process ownership, shutdown, stale leases, deep links, and concurrent
      repository visibility have explicit behavior.
- [ ] A daemon, remote bind mode, or background lifecycle is excluded unless its
      need and threat model are approved explicitly.

**Verification:** Human architecture and security review against the released V1
CLI, server, lease, authentication, and repository-identity contracts.

### V2-01: Migrate the private ledger to a multi-repository workspace

**What to build:** Let a user discover, register, list, and select multiple
repositories using the existing repository-keyed ledger and a forward-compatible
store migration.

**Blocked by:** V2-00.

**Acceptance criteria:**

- [ ] Existing V1 data migrates without identity loss or a parallel store.
- [ ] Two repositories with similar paths and commits remain distinct.
- [ ] Discovery never starts analysis or enables a repository implicitly.
- [ ] The current-repository V1 commands retain their behavior.

**Verification:** Run `go test ./...` and migration/CLI acceptance fixtures for a
V1 store plus two repositories.

### V2-02: Expose repository-scoped sessions and operations

**What to build:** Let users switch repositories and see each repository's
sessions, active operations, failures, and activity in the CLI and browser.

**Blocked by:** V2-01.

**Acceptance criteria:**

- [ ] Repository selection is explicit and visible in every session view.
- [ ] Read activity may proceed across repositories without confusing operation
      ownership.
- [ ] Policy, findings, chat, evidence, and model context never cross repository
      boundaries.
- [ ] Deep links and stale browser state cannot select an entity from another
      repository.

**Verification:** Run `go test -race ./...`, frontend tests, and a browser flow
with simultaneous sessions in two fixture repositories.

## Milestone 2: Opt-in repository-local knowledge

**Exit criterion:** Selected durable knowledge can round-trip through a reviewed,
versioned repository representation without changing the private-by-default
behavior or silently resolving conflicts.

### V2-03: Approve and implement the repo-local representation

**What to build:** Define the approved paths and versioned schema for policy,
accepted risks, learnings, and selected session exports, then support explicit
private-to-repository promotion and repository-to-private import.

**Blocked by:** V2-01.

**Acceptance criteria:**

- [ ] Nothing is written to a repository until the user selects exact material
      and confirms promotion.
- [ ] The proposed paths, schema, precedence, and conflict model are approved and
      recorded in `plan.md` before implementation proceeds past that checkpoint.
- [ ] Files are deterministic, reviewable, versioned, and contain no credentials,
      launch secrets, or private runtime records.
- [ ] Import validates repository identity, schema version, and referenced
      snapshots before use.
- [ ] Private state remains authoritative when repo-local mode is disabled.

**Verification:** Run `go test ./...`, deterministic round-trip golden tests, and
inspect promoted fixtures for forbidden secret and runtime fields.

### V2-04: Make repo-local conflicts, edits, retention, and deletion explicit

**What to build:** Reconcile concurrent private and VCS edits through visible
conflicts and audited user decisions, and provide explicit retention and deletion
operations for promoted material.

**Blocked by:** V2-03.

**Acceptance criteria:**

- [ ] Import never silently overwrites a newer or incompatible private record.
- [ ] Conflict output identifies both versions and blocks ambiguous use.
- [ ] Edit, replacement, retention, and deletion decisions are append-only audit
      events.
- [ ] Deleting a promoted copy does not silently delete private history, and the
      inverse is also true.

**Verification:** Run `go test ./...` with add/add, edit/edit, delete/edit, stale
schema, and repeated-import fixtures.

## Milestone 3: Sandboxed execution and patch validation

**Exit criterion:** Tests and reproduction commands run only in an approved
isolated environment, and a patch can be generated and validated there without
touching the real worktree.

### V2-05: Approve the sandbox execution profile

**What to build:** Select supported sandbox/container backends and platforms and
define enforceable command, filesystem, process, network, environment, secret,
resource, output, cancellation, and cleanup rules.

**Blocked by:** V1-26.

**Acceptance criteria:**

- [ ] The threat model distinguishes isolation guarantees from ordinary process
      limits.
- [ ] Missing or unsupported isolation fails closed with no host fallback.
- [ ] Repository text and model output cannot widen an execution profile.
- [ ] Runtime selection and every granted capability are reviewable and recorded.

**Verification:** Human security review plus executable capability probes for
each claimed platform/backend.

### V2-06: Run bounded commands against frozen snapshots

**What to build:** Run an explicitly approved argument vector inside the selected
isolation backend using a snapshot-derived workspace and retain reproducible
artifacts and complete run provenance.

**Blocked by:** V2-05.

**Acceptance criteria:**

- [ ] Runs never use the live repository as their working directory.
- [ ] Time, output, resource, environment, network, secret, and cancellation
      policies are enforced and visible.
- [ ] Timeout, cancellation, crash, truncation, and cleanup failure are distinct
      durable outcomes.
- [ ] The target repository and Git metadata remain byte-for-byte unchanged.

**Verification:** Run `go test -race ./...` and isolation fixtures covering
escape attempts, limits, cancellation, denied network/secrets, and cleanup.

### V2-07: Attach test and reproduction runs as evidence

**What to build:** Let a user run a bounded test or reproduction command for a
round or finding and inspect its result as snapshot-bound evidence.

**Blocked by:** V2-06.

**Acceptance criteria:**

- [ ] Command, runtime, snapshot, inputs, outputs, exit state, truncation, and
      artifacts are retained with the producing run.
- [ ] Passing, failing, blocked, and inconclusive results remain distinct.
- [ ] A test result cannot change human disposition or approve a change.
- [ ] CLI and browser show progress, cancellation, provenance, and omissions.

**Verification:** Run `go test ./...` and a compiled CLI/browser fixture for
passing, failing, timed-out, and cancelled reproduction runs.

### V2-08: Generate and validate patches in isolation

**What to build:** Let an authorized agent propose a patch inside an isolated
workspace, display the exact diff, and run selected validation without modifying
the live repository.

**Blocked by:** V2-07.

**Acceptance criteria:**

- [ ] The patch is tied to its source snapshot, generating run, validation runs,
      and resulting file digests.
- [ ] Invalid paths, binary policy violations, symlink escapes, partial patches,
      and failed validation remain explicit.
- [ ] The patch cannot stage, commit, push, or write to the live worktree.
- [ ] Validation results do not hide earlier failures or contradictory evidence.

**Verification:** Run `go test ./...` with valid, conflicting, path-traversal,
symlink, binary, and failed-validation patch fixtures.

### V2-09: Apply a validated patch only after explicit confirmation

**What to build:** Provide a separate preview-and-confirm operation that applies
an exact validated patch to the real worktree only if current state satisfies its
recorded preconditions.

**Blocked by:** V2-08 and human approval of the patch-application UX and dirty
worktree policy.

**Acceptance criteria:**

- [ ] The service checks repository identity, target paths, base digests, and
      current divergence immediately before writing.
- [ ] A stale, conflicting, or partially applicable patch fails without silent or
      partial changes.
- [ ] Confirmation is explicit, scoped to one patch revision, and audited.
- [ ] Application never stages, commits, pushes, or marks a finding resolved.

**Verification:** Run `go test ./...` and compiled CLI/browser checks for clean,
dirty, diverged, cancelled, and repeated-application cases; inspect Git metadata
for unintended writes.

## Milestone 4: Semantic and extension boundaries

**Exit criterion:** One selected semantic provider produces provenance-rich,
language-aware evidence through a stable process contract, and MIRE has an
evidence-backed compatibility position on Thunderus Quiver.

### V2-10: Stabilize the semantic-provider process contract

**What to build:** Extend the bounded analyzer seam with versioned capability
discovery, declared effects, semantic vocabularies, limits, cancellation, and
compatibility diagnostics without exposing provider schemas as domain types.

**Blocked by:** V2-06.

**Acceptance criteria:**

- [ ] Capability negotiation distinguishes semantic, lexical, metric, and textual
      evidence.
- [ ] Explicit argument vectors, schema versions, provenance, limits, and
      unsupported capabilities are mandatory.
- [ ] Providers cannot grant themselves filesystem, network, execution, or model
      authority.
- [ ] Existing V1 analyzer adapters and future semantic providers execute through
      the fail-closed sandbox boundary with no host fallback.
- [ ] Existing V1 analyzer adapters continue to work with their original evidence
      labels and limitations.

**Verification:** Run `go test ./...` against valid, old, future, malformed,
misdeclared, slow, noisy, and cancelled helper providers.

### V2-11: Integrate the first genuine semantic provider

**What to build:** Integrate one explicitly selected compiler-, language-server-,
or dedicated-analyzer provider and normalize its resolved language relationships
as evidence.

**Blocked by:** V2-10 and human selection of the first language/provider.

**Acceptance criteria:**

- [ ] Deterministic fixtures demonstrate at least one resolved relationship that
      lexical matching alone cannot establish.
- [ ] Ambiguity, unsupported constructs, partial indexing, and provider failures
      are preserved.
- [ ] Evidence records language, provider and schema versions, snapshot, exact
      anchors, and normalized-output pointers.
- [ ] UI and exports never relabel lexical Setaryb output as semantic.

**Verification:** Run `go test ./...` and provider fixture tests for resolved,
ambiguous, unsupported, and incompatible projects.

### V2-12: Assess and constrain Thunderus Quiver convergence

**What to build:** Compare MIRE's implemented process contract with the current
implemented, versioned Quiver contract and publish a compatibility map; converge
only where behavior and security guarantees can be tested.

**Blocked by:** V2-10. Direct interoperability is additionally blocked until an
implemented, stable Quiver contract exists.

**Acceptance criteria:**

- [ ] The assessment covers discovery, enablement, capabilities, effects, argv,
      schemas, limits, provenance, redaction, lifecycle, and failure behavior.
- [ ] Stable compatible behavior has contract tests or a minimal boundary
      adapter; incompatible behavior is documented without speculative coupling.
- [ ] MIRE does not gain a general plugin framework merely to mirror a draft.
- [ ] Absence of a stable Quiver contract does not misrepresent compatibility or
      block unrelated V2 safety work.

**Verification:** Human architecture review and, when upstream is stable,
cross-project contract fixtures pinned to explicit versions.

### V2-13: Expose a capability-limited MCP adapter

**What to build:** Let Thunderus or another authorized local agent host invoke a
small read-only MCP surface: list enabled repositories and sessions, and retrieve
rounds, diffs, findings, evidence, coverage, and explicit handoff exports.

**Blocked by:** V2-02.

**Acceptance criteria:**

- [ ] MCP is an outer adapter and no core package, CLI, browser, analyzer, or
      store depends on MCP representations.
- [ ] Every call is repository-scoped, authorized, validated, and auditable.
- [ ] The initial surface has no review-start, verification-start, chat,
      disposition, patch, execution, repository-enable, or publication operation.
- [ ] MCP cannot silently enable tools, apply patches, change dispositions, or
      publish data.
- [ ] The product remains fully usable with MCP disabled.

**Verification:** Run `go test ./...` with authorized, denied, cross-repository,
malformed, cancelled, and unavailable-host protocol fixtures.

## Milestone 5: V2 release hardening

**Exit criterion:** All V2 success criteria pass together without regressing V1
or weakening the security and evidence boundaries.

### V2-14: Prove V2 isolation, migration, and end-to-end behavior

**What to build:** Complete the release matrix, documentation, migration checks,
security review, and end-to-end workflows for the V2 contract.

**Blocked by:** V2-02, V2-04, V2-07, V2-09, V2-11, V2-12, and V2-13.

**Acceptance criteria:**

- [ ] Multi-repository leakage and concurrent-operation tests pass under race
      detection.
- [ ] V1 data and workflows survive upgrade, use, and restart.
- [ ] Repo-local data contains no credentials or private runtime state.
- [ ] Missing isolation never runs commands on the host, and patch application
      always requires a fresh explicit confirmation.
- [ ] Semantic, lexical, metric, and textual evidence remain distinguishable in
      CLI, web, JSON, and bundle exports.
- [ ] The supported platform/runtime matrix and remaining limitations are
      documented accurately.

**Verification:** Run `go build ./cmd/mire`, `go test ./...`,
`go test -race ./...`, the app checks from the plan, sandbox release fixtures,
and the human V2 security and UX checklist.

## Frontier

No V2 implementation ticket is currently unblocked. Complete V1-26 first. V2-00
and V2-05 then become the first parallel tickets: workspace topology and sandbox
policy may be decided independently before their implementation branches meet.
