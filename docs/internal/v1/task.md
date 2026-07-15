---
title: "MIRE V1 implementation tasks"
status: "in progress"
updated: "2026-07-14"
source: "plan.md"
---

# MIRE V1 implementation tasks

These tickets deliver the local review workbench specified in
[plan.md](plan.md). They are ordered by dependency, not by package or UI layer.
Work the frontier: any ticket whose blockers are complete.

## Dependency frontier

Completed prerequisites: V1-01, V1-02, and V1-03.

The current frontier contains two tickets:

- **V1-04 — Preserve three-dot merge-base semantics.**
- **V1-05 — Capture the complete working tree without tearing.**

Later tickets may proceed as soon as their declared blockers are complete; a
milestone need not finish before work starts on an unblocked ticket in the next
milestone.

## Milestone 1: Durable private sessions

**Exit criterion:** Private repository/session state survives restart, the CLI can
list and delete existing sessions, and interrupted or competing operations leave
durable, truthful state. V1-03 now provides user-visible session creation with
snapshot capture in Milestone 2.

### V1-01 — Establish private review state and session lifecycle

**What to build:** Establish the private repository/session store and let users
list and delete review sessions from the CLI. Store application state in the
operating system's private per-user state directory, outside the repository, and
preserve the repository identity needed for later multi-repository support.
`mire review` will create user-visible sessions when snapshot capture arrives in
V1-03; do not add an empty-session creation command.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] `mire sessions list` initializes private state when needed and lists only
      persisted session metadata.
- [x] The application service can transactionally create a session for the
      current repository, show it after process restart, and delete it explicitly;
      the CLI exposes list and delete but not empty-session creation.
- [x] Session and repository records use stable IDs and every session is keyed by
      `repository_id`.
- [x] SQLite enables foreign keys, WAL, a busy timeout, transactional forward
      migrations, and restrictive state-directory and database permissions.
- [x] Deleting an unknown session and running outside a Git repository return
      clear errors without changing Git or project files.
- [x] CLI stdout remains suitable for the requested result while diagnostics go
      to stderr.

**Verification:**

- `go test ./...`
- In a temporary Git repository, create a session through the application-service
  fixture, restart the command, list it, delete it through the CLI, and confirm no
  file or Git metadata changed in the target repository.

**Notes:** Handwrite migrations and SQL. Do not introduce an ORM, event-sourcing
framework, or repository-local state.

### V1-02 — Recover interrupted and competing operations truthfully

**What to build:** Represent long-running review work as durable operations so a
session cannot run two state-changing or model operations at once and an
interrupted process is reported as abandoned rather than still running.

**Blocked by:** V1-01 (complete)

**Acceptance criteria:**

- [x] Operations implement `queued`, `running`, `complete`, `failed`,
      `cancelled`, and `abandoned` states with validated transitions.
- [x] A transactional lease and unique active-operation constraint permit only
      one state-changing or model operation per session across processes.
- [x] A random process-instance owner renews a bounded heartbeat; acquisition or
      startup marks an expired operation abandoned and its round incomplete.
- [x] Cancellation is durable and idempotent, and completed state is never
      inferred from transient progress output.
- [x] Durable activity entries are committed atomically with the state changes
      they describe and receive monotonically increasing IDs.
- [x] Concurrent read-only session queries remain available while an operation
      runs.

**Verification:**

- `go test -race ./...`
- Exercise two processes against one temporary state directory and confirm the
  second mutation is rejected, then terminate the lease owner and confirm a later
  acquisition records abandonment.

**Notes:** This is an enabling prefactor for CLI, HTTP, and SSE work. Keep the
operation model transport-neutral; streamed token deltas are not durable state.

## Milestone 2: Immutable Git snapshots

**Exit criterion:** Committed two-dot and three-dot comparisons and complete
working-tree states are captured atomically into application-owned storage. All
later reads survive source Git-object deletion, and races or resource-limit
violations fail explicitly before review begins.

### V1-03 — Capture a durable two-dot review snapshot

**What to build:** Let `mire review --range <base>..<head>` resolve a committed
comparison once, copy every required byte into MIRE's private content-addressed
store, persist an immutable manifest and review round, and perform no model call
yet.

**Blocked by:** V1-01 (complete)

**Acceptance criteria:**

- [x] Symbolic revisions resolve once to object IDs; invalid or ambiguous
      revisions fail before a round is created.
- [x] The snapshot records the requested expression, effective base and target
      OIDs, Git object format, capture time, repository ID, and policy hash.
- [x] `mire review` creates the user-visible session and first round only when the
      snapshot transaction succeeds; failed capture leaves neither usable record.
- [x] Complete base and target tree manifests include every tracked path, not only
      changed paths, while representing deletions, renames, binary files,
      executable modes, symlink targets, and submodule Git links without following
      symlinks.
- [x] Every byte referenced by either complete tree is written and digest-verified
      in the private object store before the manifest transaction commits.
- [x] Snapshot reads use the stored manifest and objects rather than the live
      worktree or Git object database.
- [x] A failed object write leaves no committed snapshot or apparently usable
      round.

**Verification:**

- `go test ./...`
- `go test -race ./...`
- `go vet ./...`
- `go build ./cmd/mire`
- Fixture repositories cover complete unchanged trees, regular and binary files,
  executable modes, symlink targets, deletions, exact renames, invalid and
  ambiguous revisions, failed object writes, transaction rollback, and reads
  after source `.git` removal.

**Status:** Complete. The initial capture path is model-free; `--session` append
behavior remains part of V1-07.

**Notes:** Git object IDs are provenance, never the only durable copy. Normalize
and validate repository-relative paths before object-store access.

### V1-04 — Preserve three-dot merge-base semantics

**What to build:** Let `mire review --range <base>...<head>` capture the change
from the resolved merge base to the target while preserving the user's original
comparison and the exact objects that determined it.

**Blocked by:** V1-03 (complete)

**Acceptance criteria:**

- [ ] Three-dot input resolves and stores the base OID, target OID, and effective
      merge-base OID exactly once.
- [ ] The captured diff matches Git's merge-base comparison for diverged fixture
      histories.
- [ ] Missing, multiple, or invalid merge-base situations produce explicit
      diagnostics before model work.
- [ ] Later movement of branches or tags cannot change the stored round.
- [ ] Two-dot behavior remains unchanged and the requested comparison kind is
      distinguishable in storage and exports.

**Verification:**

- `go test ./...`
- Compare MIRE's manifest and diff with Git output in temporary linear, diverged,
  renamed-branch, and invalid-history fixtures.

### V1-05 — Capture the complete working tree without tearing

**What to build:** Let `mire review --worktree` freeze the current `HEAD`, index,
and final worktree layers, including nonignored untracked files, so overlapping
staged and unstaged edits remain intelligible after the live repository changes.

**Blocked by:** V1-03 (complete)

**Acceptance criteria:**

- [ ] Capture preserves distinct `HEAD`, index, and final-worktree identities and
      content for paths changed in more than one layer.
- [ ] Nonignored untracked files are included; ignored files are excluded and the
      policy is recorded.
- [ ] Deletions, renames, binary files, executable bits, spaces, Unicode paths,
      and symlink targets are represented without following symlinks.
- [ ] Clean submodules are opaque Git links; dirty submodule state fails with
      guidance to review that repository separately.
- [ ] Every stored manifest is complete and all subsequent reads come from its
      private objects.
- [ ] Capturing and later reading the snapshot causes no target-repository or Git
      metadata writes.

**Verification:**

- `go test ./...`
- Run the working-tree fixture matrix from the V1 plan, mutate the live files
  after capture, and confirm the snapshot still presents the original three
  layers.

### V1-06 — Reject raced, oversized, or unsafe captures

**What to build:** Protect committed and working-tree capture from concurrent
changes, path escapes, partial object writes, and explicit resource-ceiling
violations, retrying only when a coherent capture is still possible.

**Blocked by:** V1-04, V1-05

**Acceptance criteria:**

- [ ] Working-tree capture reads identities and inventory before copying, then
      rechecks identities and file metadata/content before committing.
- [ ] Concurrent changes cause a bounded retry or a torn-snapshot diagnostic;
      they never produce a mixed manifest.
- [ ] Explicit ceilings cover at least file count, individual object size, and
      aggregate captured bytes, and explain which configured limit was exceeded.
- [ ] Limit failures occur before a review round or model call and never silently
      truncate the snapshot.
- [ ] Absolute paths, traversal, special devices, and object-store escapes are
      rejected at the boundary.
- [ ] Orphaned temporary content from failed capture is safely recoverable or
      collectible without deleting referenced objects.

**Verification:**

- `go test -race ./...`
- Use controlled concurrent writers and limit-boundary fixtures to prove that
  each result is either one coherent version or an explicit failure.

**Notes:** Numeric defaults are configuration decisions to set from measured
fixtures before release; the contract is explicit failure, not a particular
initial number.

### V1-07 — Append review rounds and report live divergence

**What to build:** Let a user append a newly captured snapshot as another round
of the same repository-scoped session and see whether the live repository has
diverged from any frozen round without changing that round.

**Blocked by:** V1-02, V1-04, V1-05

**Acceptance criteria:**

- [ ] `mire review --session <id>` accepts either range mode or worktree mode and
      appends a monotonically numbered immutable round.
- [ ] Appending to a session for another repository is rejected.
- [ ] Each round records its snapshot and optional predecessor; a new capture
      never mutates prior round data.
- [ ] Divergence distinguishes unchanged, changed, unavailable, and unsupported
      comparisons and identifies affected paths or refs when possible.
- [ ] Restart preserves the current round and complete prior history.
- [ ] A failed or cancelled new round does not replace the prior current round
      with an apparently complete result.

**Verification:**

- `go test ./...`
- Create range and worktree rounds in a temporary repository, modify branches and
  files between them, restart MIRE, and inspect round history and divergence.

## Milestone 3: Evidence-led review and model roles

**Exit criterion:** A deterministic fixture model and both supported provider
adapters can run the planner, reviewer, verifier, and contextual-chat roles over
frozen snapshots. The ledger retains complete pass outcomes, derives verified and
candidate lanes from evidence, supports human decisions, and rejects unscoped
chat.

### V1-08 — Assemble review intent and the frozen change model

**What to build:** Turn a captured snapshot into a deterministic review input
that combines the diff, file and hunk inventory, affected surfaces, user intent,
and allowed repository guidance under the settled policy precedence.

**Blocked by:** V1-04, V1-05

**Acceptance criteria:**

- [ ] The change model identifies files, hunks, tests, contracts,
      configuration, dependencies, migrations, and public surfaces when the
      snapshot provides evidence for them.
- [ ] Intent may include the user's prompt, pinned commit messages, base-snapshot
      `AGENTS.md`/contribution/architecture guidance, and an earlier same-session
      round.
- [ ] Built-in safety rules outrank private request/configuration, which outranks
      base policy, base documentation, and target policy changes in that order.
- [ ] Path-specific policy only overrides general policy within its own tier;
      same-tier conflicts are recorded and use the safer interpretation.
- [ ] Target policy changes are review evidence, not authority to review
      themselves, except for the recorded no-base-revision case.
- [ ] All context and pinned Git metadata come from immutable snapshot inputs and
      are digest-recorded; repository text cannot grant tools or permissions.

**Verification:**

- `go test ./...`
- Run table-driven precedence and path-scope fixtures, including conflicting and
  target-modified policy, and compare canonical change-model output.

### V1-09 — Produce a review plan and explainable logical slices

**What to build:** Run a provider-neutral planner over the change model and
persist risk areas, planned passes, required context, coverage limits, and logical
change slices that can later drive both terminal and browser navigation.

**Blocked by:** V1-02, V1-08

**Acceptance criteria:**

- [ ] The core owns provider-neutral message, tool, structured-output, usage,
      status, retry, timeout, cancellation, and provenance contracts.
- [ ] A deterministic fixture model can complete planner runs without network or
      credentials.
- [ ] Logical slices reference exact snapshot hunks and explain their grouping
      and risk cues; a file-oriented view remains derivable.
- [ ] The plan records required context, applicable and skipped passes, ordering,
      and known coverage limitations without claiming universal optimality.
- [ ] Malformed structured output follows a bounded repair/retry policy and ends
      as a visible failed run when still invalid.
- [ ] Run records include adapter/protocol and prompt-template versions, model
      selection, parameters, input manifest, digests, usage when supplied, finish
      reason, redactions, status, and termination cause.

**Verification:**

- `go test ./...`
- Run fixture planner responses for valid, malformed, repaired, timed-out,
  cancelled, and budget-exhausted cases and inspect persisted plan/run records.

**Notes:** Provider DTOs must stay outside the review domain. Define only the
small model interface consumed by the orchestrator.

### V1-10 — Retain every emitted candidate and honest coverage

**What to build:** Run applicable specialized review passes with bounded,
finding-specific snapshot retrieval, retain every well-formed candidate they
emit, and report completed, failed, skipped, truncated, and unsupported work
without turning incompleteness into “no findings.”

**Blocked by:** V1-09

**Acceptance criteria:**

- [ ] Applicable passes cover the V1 categories in the plan, with no finding
      quota and no requirement that every pass emit a candidate.
- [ ] Retrieval starts from changed code and records each additional artifact,
      relationship, requesting run, digest, exclusion, and truncation.
- [ ] Every schema-valid plausible candidate within the declared pass budget is
      persisted before correlation or presentation filtering.
- [ ] Unsupported prose and malformed candidate payloads become diagnostics,
      not fabricated findings.
- [ ] Coverage records examined files/hunks, retrieved tests/contracts, completed
      passes, analyzer availability, exclusions, failures, and declared gaps.
- [ ] A successful empty pass, a failed pass, and a truncated pass remain
      semantically distinct after restart.

**Verification:**

- `go test ./...`
- Use fixture passes that emit duplicates, weak candidates, zero candidates,
  malformed output, failures, and truncation; confirm retention and coverage.

### V1-11 — Verify candidates against an evidence floor

**What to build:** Investigate each candidate adversarially, record supporting
and contradictory evidence, and derive verified, candidate, and refuted views
without permitting model confidence or analyzer output alone to promote a
finding.

**Blocked by:** V1-10

**Acceptance criteria:**

- [ ] Verification states are `not_run`, `supported`, `inconclusive`, `refuted`,
      and `blocked`, with validated transitions and immutable run provenance.
- [ ] The verifier states the suspected invariant violation, traces a concrete
      path, searches for guards/tests, and records an attempted refutation and
      material contradictory evidence.
- [ ] Evidence records relation, snapshot, anchors, summary, producing run,
      artifact digest, and an exact retained-output pointer when available.
- [ ] The verified lane requires a valid claim and impact, a snapshot anchor,
      independent concrete supporting evidence, and a completed qualifying
      verifier run.
- [ ] Inconclusive and blocked items remain candidates; refuted items remain
      auditable but hidden from the default view.
- [ ] The evidence floor cannot be weakened by configuration and confidence is
      descriptive only.

**Verification:**

- `go test ./...`
- Table-test every evidence-floor boundary and prove no contradictory writable
  lane state can be stored.

### V1-12 — Preserve finding identity and explicit human decisions

**What to build:** Give findings stable, revision-aware identities across review
rounds while letting users record dispositions and edit publishable wording
without rewriting machine evidence or history.

**Blocked by:** V1-07, V1-11

**Acceptance criteria:**

- [ ] Finding revisions are immutable and carry claim, impact, category,
      severity, confidence, verification, anchors, evidence, origin, and
      relationships.
- [ ] Anchors combine snapshot side/layer, path, blob digest, line range,
      original hunk, context and hunk digests, plus optional symbol/syntax
      fingerprints; line numbers alone are never identity.
- [ ] Strong claim/invariant and anchor matches retain an ID across rounds;
      ambiguous matches create linked possible successors or duplicates instead
      of false continuity.
- [ ] Human dispositions support `open`, `accepted`, `intentional`, `dismissed`,
      `deferred`, `resolved`, and `accepted_risk` independently of verification.
- [ ] Disposition changes are append-only and rationales required by the selected
      disposition are retained.
- [ ] Editing a comment creates a versioned presentation record and never alters
      the finding's evidence or machine-verification history.

**Verification:**

- `go test ./...`
- Exercise moved lines, renamed paths, rewritten claims, ambiguous matches,
  dispositions, and comment edits across two fixture rounds.

### V1-13 — Keep every chat turn bound to review context

**What to build:** Provide one durable chat timeline per session for questions,
challenges, candidate proposals, and re-verification requests, while requiring
every turn to reference an exact finding revision or validated diff selection.

**Blocked by:** V1-11, V1-12

**Acceptance criteria:**

- [ ] A user chat message is rejected unless it contains at least one finding
      revision or diff anchor from its active round and snapshot.
- [ ] The service validates and persists canonical bindings before a model starts;
      assistant replies inherit the initiating turn's primary binding.
- [ ] Changing later selection never rewrites prior message context, and live
      divergence marks chat stale relative to the worktree without invalidating
      its snapshot meaning.
- [ ] Any extra snapshot context retrieved by chat is logged as run input.
- [ ] Chat may propose a structured candidate or request verification but cannot
      silently change a finding, disposition, wording, snapshot, or repository.
- [ ] Re-verification creates a new run and revision-aware evidence history rather
      than overwriting the previous result.
- [ ] Timeline, failures, cancellation, and usage survive restart.

**Verification:**

- `go test ./...`
- Submit scoped and deliberately unscoped turns directly to the service, change
  live files, restart, and inspect preserved bindings and re-verification history.

### V1-14 — Run all model roles through OpenAI-compatible endpoints

**What to build:** Let a user configure planner, reviewer, verifier, and chat
roles against an OpenAI-compatible HTTP endpoint while preserving the common run
contract and explicit capability differences.

**Blocked by:** V1-09

**Acceptance criteria:**

- [ ] Role configuration supports one shared model or separate aliases, base URL,
      requested model, timeouts, retry and budget limits, and a credential
      reference.
- [ ] Streaming and nonstreaming responses, structured output, usage, finish
      reasons, provider errors, rate limits, malformed frames, and cancellation
      map into provider-neutral records.
- [ ] Capability detection reports partial compatibility rather than assuming
      every endpoint supports every OpenAI feature.
- [ ] Bounded retries respect cancellation and retryable status guidance; partial
      output never becomes a completed structured result.
- [ ] Authorization values and launch secrets never enter logs, the database,
      browser payloads, or exports.
- [ ] Default tests use a local HTTP fixture and require no live credentials.

**Verification:**

- `go test ./...`
- Exercise the adapter against `httptest.Server` fixtures for streaming,
  malformed events, retries, timeouts, cancellation, usage, redaction, and budget
  termination.

### V1-15 — Run all model roles through Anthropic

**What to build:** Let a user route any review role to Anthropic's native API
with the same cancellation, provenance, validation, and privacy guarantees as
the OpenAI-compatible adapter.

**Blocked by:** V1-09

**Acceptance criteria:**

- [ ] Role configuration supports a native Anthropic endpoint, model, credential
      reference, timeouts, retries, and budgets without leaking provider types
      into the domain.
- [ ] Native streaming events, tool/structured output, usage, stop reasons,
      overload/rate-limit responses, malformed frames, and cancellation map into
      common run records.
- [ ] Structured-output validation and bounded repair behavior match the domain
      guarantees used by other providers.
- [ ] Secrets and raw authorization data are redacted from all durable and
      exported records.
- [ ] Planner, reviewer, verifier, and chat can all use Anthropic or participate
      in cross-provider routing.
- [ ] Default tests use local HTTP fixtures and require no live credentials.

**Verification:**

- `go test ./...`
- Exercise native Anthropic fixtures for streaming, malformed events, retries,
  timeouts, cancellation, usage, redaction, and budget termination.

## Milestone 4: Terminal, exports, and optional analyzers

**Exit criterion:** The native CLI renders a deterministic human review, exports
all four V1 formats from the same ledger, and can enrich a review through bounded
Setaryb and Mccabre subprocesses without making either executable mandatory.

### V1-16 — Review and inspect results in a static terminal report

**What to build:** Complete `mire review` and `mire show` so users receive stable
progress on stderr and a width-aware static diff with anchored findings,
candidates, and incomplete-analysis diagnostics on stdout.

**Blocked by:** V1-10, V1-11

**Acceptance criteria:**

- [ ] `mire review` captures, runs, and persists a review, prints stable progress
      and a final summary to stderr, and does not fail merely because findings
      exist.
- [ ] `mire show [session]` renders the selected round's unified diff and verified
      findings as the primary section.
- [ ] `--candidates` reveals a separate candidate section; refuted findings and
      omissions remain separately identifiable and are never blended into
      verified output.
- [ ] Anchored comments remain readable for additions, deletions, moved context,
      Unicode, and narrow terminals.
- [ ] Output is deterministic at a fixed width, respects `NO_COLOR`, and never
      requires an interactive TUI.
- [ ] Provider or pass failure is displayed as incomplete analysis rather than a
      successful no-findings result.

**Verification:**

- `go test ./...`
- Run compiled-CLI golden tests at fixed widths with and without color, checking
  stdout/stderr separation and exit semantics.

### V1-17 — Export one canonical ledger into all V1 formats

**What to build:** Let `mire export` deterministically project a stored session
into Markdown, canonical JSON, SARIF 2.1.0, or an inspectable multi-file bundle,
without implying the export can restore the private snapshot.

**Blocked by:** V1-12, V1-13

**Acceptance criteria:**

- [ ] Canonical `review.json` is versioned independently of SQLite and contains
      the normalized ledger, snapshot manifest, artifact descriptors,
      provenance, coverage, and omissions without credentials or snapshot-object
      contents.
- [ ] Markdown is a readable front door with clearly distinct verified,
      candidate, refuted/audit, chat, coverage, and incomplete-analysis sections.
- [ ] SARIF 2.1.0 contains only representable findings with valid locations,
      stable rule/result identities, and a declared loss of chat, detailed
      verification history, and rich dispositions.
- [ ] Bundle output contains `REVIEW.md`, `review.json`, `manifest.json`,
      `diff.patch`, `findings.json`, `evidence.jsonl`, `chat.jsonl`,
      `activity.jsonl`, and `findings.sarif`, plus only named evidence artifacts.
- [ ] IDs and ordering are deterministic; repeated exports of unchanged state
      are byte-stable except for fields explicitly defined as export-instance
      metadata.
- [ ] Export is explicit, never silently overwrites a destination, warns that
      code/conversation may be sensitive, and cannot be mistaken for a V1 import
      or replay format.

**Verification:**

- `go test ./...`
- Export a fixture session twice to every format, compare bytes and schemas,
  inspect the bundle manifest, and scan outputs for fixture credentials.

**Notes:** Generate every view from the stored domain projection, not from
another export format. SARIF and SQLite representations must not become domain
types.

### V1-18 — Run fixed analyzers with bounded, auditable subprocesses

**What to build:** Add the internal process boundary through which explicitly
enabled, trusted Setaryb and Mccabre executables analyze a private snapshot
materialization under strict time, output, environment, and cancellation limits.

**Blocked by:** V1-06, V1-09

**Acceptance criteria:**

- [ ] The user must explicitly enable a fixed first-party adapter; arbitrary
      executable names, argument strings, project commands, and shell invocation
      are impossible through this boundary.
- [ ] A capability probe runs before analysis and records availability, version,
      expected schema, and compatibility.
- [ ] Analysis uses an explicit argument vector, minimal environment, private
      snapshot materialization, timeout, cancellation, and separate stdout/stderr
      limits.
- [ ] Run provenance records adapter and executable versions/digest, argument
      template, configuration, snapshot, limits, exit status, diagnostics, raw
      and normalized output digests, and limitations.
- [ ] Success with no evidence, partial output, incompatible schema, invalid JSON,
      nonzero exit, timeout, cancellation, and oversized output remain distinct.
- [ ] Absence or failure degrades the review visibly and cannot corrupt the round
      or prevent baseline text review.

**Verification:**

- `go test ./...`
- Use helper executables to exercise every status, timeout, cancellation, output
  cap, environment, argument, and provenance case; confirm no shell is involved.

**Notes:** This is a closed internal seam, not a public plugin or Thunderus Quiver
manifest. V1 subprocesses are trusted host programs, not sandboxed code.

### V1-19 — Add syntax-aware lexical evidence from Setaryb

**What to build:** Normalize Setaryb's map JSON into MIRE-owned symbols,
references, ranking, limitations, coverage, and evidence so review retrieval can
use lexical structure without presenting it as semantic resolution.

**Blocked by:** V1-18

**Acceptance criteria:**

- [ ] The adapter invokes only Setaryb's map operation against the snapshot
      materialization and validates its supported schema/version.
- [ ] Symbols, lexical references, rankings, omissions, parse/query failures, and
      provenance normalize into MIRE-owned records with source pointers.
- [ ] All resulting evidence and UI/terminal labels say `lexical`; imports, types,
      macros, runtime behavior, and call targets are never claimed as resolved.
- [ ] Ambiguous references and partial language support remain explicit rather
      than being guessed or discarded.
- [ ] Review planning/retrieval may consume normalized evidence, while promotion
      to verified still requires the normal independent evidence floor.
- [ ] Missing, incompatible, noisy, or failing Setaryb leaves a usable baseline
      review and a precise coverage diagnostic.

**Verification:**

- `go test ./...`
- Run recorded Setaryb JSON fixtures for supported languages, ambiguity, empty
  maps, parse failures, omissions, incompatible schemas, and oversized reports.

**Notes:** Setaryb history output is outside V1 because its revision/provenance
contract is not yet reconciled with MIRE snapshots.

### V1-20 — Add complexity and clone evidence from Mccabre

**What to build:** Normalize Mccabre's read-only JSON analysis into MIRE-owned
LOC, heuristic complexity, and exact-token clone evidence that can guide review
planning and candidate investigation.

**Blocked by:** V1-18

**Acceptance criteria:**

- [ ] The adapter invokes only the approved read-only analysis operation against
      the snapshot materialization and validates its schema/version.
- [ ] LOC, heuristic complexity, exact-token clone groups, omissions, and
      provenance normalize into MIRE-owned evidence with source pointers.
- [ ] Metrics are described with their actual heuristic/lexical limits and never
      presented as proof of a defect or semantic call relationship.
- [ ] Planner and verifier runs can cite the evidence, but analyzer output alone
      cannot satisfy verification or change human disposition.
- [ ] Missing, incompatible, partial, or failed Mccabre produces an explicit
      coverage diagnostic while baseline review completes.
- [ ] Coverage-report generation and ingestion are not exposed through this V1
      adapter.

**Verification:**

- `go test ./...`
- Run recorded Mccabre fixtures for empty and populated metrics, clones,
  omissions, incompatible schemas, invalid JSON, and failures.

## Milestone 5: Secure interactive web review

**Exit criterion:** `mire web` starts one authenticated foreground loopback
process serving embedded static assets, JSON commands/queries, and resumable SSE.
The SvelteKit app can explore a review and complete every V1 human action,
including mandatory-context chat, across refresh and reconnect.

### V1-21 — Serve the review API, durable progress, and embedded app securely

**What to build:** Let `mire web [session]` serve the embedded static SvelteKit
application and versioned JSON/SSE API from one foreground Go process, using the
same application service as the CLI.

**Blocked by:** V1-02, V1-07

**Acceptance criteria:**

- [ ] The server binds only to loopback on an available or requested port and
      shuts down cleanly on interruption; there is no daemon or remote-bind mode.
- [ ] A high-entropy launch capability establishes an HttpOnly, SameSite cookie
      through a one-time URL and redirects to a clean URL.
- [ ] Unexpected Host and Origin values, CORS requests, unauthenticated API/SSE,
      non-JSON mutations, and invalid paths are rejected.
- [ ] `/api/v1` exposes validated bootstrap, session, round, operation,
      cancellation, activity, and divergence resources; later tickets extend the
      API for review data and actions.
- [ ] Long mutations return `202` operations, creation uses idempotency keys, and
      revision changes reject stale expected revisions.
- [ ] SSE emits versioned events with monotonic activity IDs, supports
      `Last-Event-ID`, and recovers durable state written by another CLI process;
      transient deltas may be lost without losing canonical results.
- [ ] Static assets are built from `app/`, embedded in the binary, receive correct
      cache headers, and support client-side fallback routing without a Node
      process.

**Verification:**

- `go test ./...`
- `pnpm --dir app build`
- Run HTTP contract tests for authentication, Host/Origin, idempotency,
  concurrency, validation, SSE reconnect, cancellation, static caching, and
  fallback routing.

**Notes:** The CLI invokes the application service directly; it must never call
this HTTP API. The browser must use the API and never bypass it.

### V1-22 — Explore diffs, slices, lanes, and evidence in the browser

**What to build:** Replace the SvelteKit scaffold with an accessible review
workspace for navigating rounds, logical change slices, files, unified or
side-by-side diffs, finding lanes, coverage, evidence, provenance, and omissions.

**Blocked by:** V1-10, V1-11, V1-12, V1-21

**Acceptance criteria:**

- [ ] Session and round overview shows intent, status, snapshot identity,
      divergence, models, analyzers, coverage, and omissions.
- [ ] Users can navigate logical slices or files and order by recorded risk,
      relevance, diff size, tests, or dependency impact without implying that
      navigation proves review coverage.
- [ ] Unified and side-by-side diffs expose stable, selectable anchors and remain
      usable at ordinary laptop widths.
- [ ] Verified, candidate, and optional refuted views are visually and
      semantically distinct using more than color.
- [ ] Finding details expose claim, impact, anchors, supporting and contradicting
      evidence, retrieved context, run provenance, analyzer limitations, and
      relationships.
- [ ] The read-only API exposes diff, slice, finding, coverage, evidence, and
      provenance resources without accepting arbitrary paths.
- [ ] Keyboard navigation and focus behavior permit complete read-only
      exploration, and refresh restores canonical state from the API.

**Verification:**

- `pnpm --dir app check`
- `pnpm --dir app lint`
- `pnpm --dir app test:unit --run`
- Run component/browser tests for navigation, ordering, diff modes, lane filters,
  evidence/provenance views, non-color cues, focus, and responsive layouts.

### V1-23 — Triage, discuss, re-verify, and export in the browser

**What to build:** Complete the interactive workflows for dispositions, comment
revisions, contextual chat, re-verification, operation progress/cancellation,
and explicit exports while keeping every action revision-safe and scoped.

**Blocked by:** V1-13, V1-17, V1-21, V1-22

**Acceptance criteria:**

- [ ] Users can record each supported disposition with required rationale and
      edit publishable wording through explicit actions that show stale-write
      conflicts.
- [ ] The action API exposes validation and optimistic concurrency for
      dispositions, comment revisions, verification, contextual chat, and export.
- [ ] The chat composer remains disabled until an exact finding revision or diff
      selection is bound, and every sent and received message displays its
      immutable context.
- [ ] Users can challenge or explain findings, propose candidates, and request
      re-verification without chat silently mutating structured state.
- [ ] Operation progress, incomplete state, failure, retry eligibility, and
      cancellation are visible without treating transient SSE text as canonical.
- [ ] Users can explicitly start and retrieve each export format, see privacy and
      fidelity warnings, and cannot silently overwrite a destination.
- [ ] Refresh and SSE reconnect recover chat, dispositions, comments, finding
      lanes, operations, and export state.
- [ ] One Playwright flow against the real Go server covers review reload,
      contextual chat, persistence, re-verification, cancellation/reconnect, and
      export without live model credentials.

**Verification:**

- `pnpm --dir app check`
- `pnpm --dir app test:unit --run`
- `pnpm --dir app exec playwright test`
- `go test ./...`

## Milestone 6: Release confidence

**Exit criterion:** Security/privacy checks, quality evaluation, reproducible
builds, race tests, and black-box release smoke all pass on macOS and Linux. The
single native binary works with no Node process and remains useful without
optional analyzers or live model credentials in its default test suite.

### V1-24 — Audit V1 authority, privacy, and failure boundaries

**What to build:** Exercise the complete product as an adversarial local input
and process boundary, close any path that can widen V1 authority or leak secrets,
and make every partial or failed operation visible in stored and exported state.

**Blocked by:** V1-14, V1-15, V1-19, V1-20, V1-21, V1-23

**Acceptance criteria:**

- [ ] Black-box monitoring confirms committed and working-tree reviews never
      write target files or Git metadata.
- [ ] Repository instructions and analyzer output cannot grant tools, alter
      policy precedence, run arbitrary commands, contact arbitrary URLs through
      MIRE, or escape snapshot paths.
- [ ] Analyzer invocations cannot use a shell or user-controlled arbitrary
      executables and visibly disclose that enabled tools retain host authority.
- [ ] Model access is limited to configured endpoints and read-only snapshot
      tools; no API accepts arbitrary commands, executable paths, project paths,
      provider credentials, or remote bind addresses.
- [ ] Credentials, authorization headers, launch tokens, and configured secret
      fixtures are absent from logs, SQLite, browser payloads, diagnostics, model
      provenance, and every export.
- [ ] Capture, provider, analyzer, pass, stream, and export failures cannot appear
      as a complete no-findings review.
- [ ] State and object permissions, path canonicalization, Host/Origin checks,
      local authentication, cancellation, and process shutdown pass their threat
      fixtures.

**Verification:**

- `go test -race ./...`
- `pnpm --dir app test:unit --run`
- Run the release security fixture matrix and scan all resulting state, logs, and
  exports for sentinel secrets and unauthorized writes.

### V1-25 — Gate review quality with a frozen adjudicated corpus

**What to build:** Establish a repeatable evaluation that measures factuality,
recall, redundancy, actionability, structured-output reliability, human triage
burden, latency, and cost independently, then apply the V1 verified-lane release
gate to each supported model configuration.

**Blocked by:** V1-10, V1-11, V1-14, V1-15

**Acceptance criteria:**

- [ ] A frozen, versioned, human-adjudicated corpus contains at least 100
      representative seeded or historical candidates with expected evidence and
      severity constraints.
- [ ] Evaluation reports verified precision, candidate recall, redundancy,
      abstentions, parsing/repair failures, actionability, triage burden, latency,
      token use, and cost without collapsing them into one score.
- [ ] Each supported release model configuration reaches at least 90% factual and
      actionable precision in the verified lane.
- [ ] No verified item lacks the normative evidence floor and no unsupported
      blocker or high-severity finding passes the gate.
- [ ] Candidate recall and every non-gating dimension are still reported when
      they miss a target; no minimum finding count is imposed.
- [ ] Corpus fixtures and default correctness tests can run without sending
      private project data or requiring live provider credentials; live release
      evaluations are explicit and separately configured.

**Verification:**

- Run the frozen offline evaluation fixtures and compare the deterministic
  report with its checked schema.
- Run the explicit release evaluation for each supported model configuration and
  archive its configuration hash and metrics with the release evidence.

**Notes:** Do not tune against finding count. Changes to corpus labels or release
thresholds require review and versioned rationale.

### V1-26 — Produce and smoke-test the single native release artifact

**What to build:** Reproducibly build the static frontend into the Go binary,
exercise the compiled CLI and real loopback server against temporary
repositories, and publish a release candidate only when all V1 gates pass on the
claimed platforms.

**Blocked by:** V1-06, V1-16, V1-17, V1-19, V1-20, V1-23, V1-24, V1-25

**Acceptance criteria:**

- [ ] The SvelteKit app uses a pinned static adapter/build and the generated
      assets embedded in `mire` are reproducible from the lockfile.
- [ ] One native binary provides the CLI and foreground web server without a
      runtime Node process, daemon, container, Setaryb, or Mccabre requirement.
- [ ] Release smoke captures and reviews both committed-range and working-tree
      fixtures, renders results, serves `/` and `/api/v1`, reconnects SSE, exports
      all formats, and shuts down cleanly.
- [ ] Optional analyzers are exercised as absent, incompatible, slow, noisy,
      failing, and successful without corrupting the session.
- [ ] Formatting, static checks, unit/integration tests, race tests, CLI
      black-box tests, HTTP contracts, frontend tests, browser E2E, security
      checks, and the quality gate all pass.
- [ ] macOS and Linux artifacts report version/schema information and pass their
      platform smoke tests; no Windows-support claim is made by V1.
- [ ] The release notes state snapshot/export privacy, analyzer host authority,
      SARIF/bundle fidelity limits, provider variability, and the absence of an
      exhaustive-review or approval claim.

**Verification:**

- `pnpm --dir app install --frozen-lockfile`
- `pnpm --dir app exec playwright install chromium`
- `pnpm --dir app check`
- `pnpm --dir app lint`
- `pnpm --dir app test:unit --run`
- `pnpm --dir app build`
- `pnpm --dir app exec playwright test`
- `go fmt ./...`
- `go vet ./...`
- `go test ./...`
- `go test -race ./...`
- `mkdir -p dist`
- `go build -trimpath -o dist/mire ./cmd/mire`
- `go test ./internal/acceptance -run TestReleaseSmoke`

**Notes:** CI should fail if formatting changes tracked files. A thin Makefile may
alias these commands but must not contain hidden build or release logic.

## Final frontier

V1-01, V1-02, and V1-03 are complete. The current frontier is:

- **V1-04 — Preserve three-dot merge-base semantics.**
- **V1-05 — Capture the complete working tree without tearing.**

Continue with any ticket whose listed blockers are complete. Prefer one ticket per
fresh agent context, and do not add ordering edges merely to keep milestone work
sequential.
