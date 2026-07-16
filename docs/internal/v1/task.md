---
title: "MIRE V1 implementation tasks"
status: "in progress"
updated: "2026-07-16"
source: "plan.md"
---

## Milestone 1: Durable private sessions

**Exit criterion:** Private repository/session state survives restart, the CLI can
list and delete existing sessions, and interrupted or competing operations leave
durable, truthful state. V1-03 now provides user-visible session creation with
snapshot capture in Milestone 2.

### V1-01 — Establish private review state and session lifecycle

Establishes the private repository/session store and let users list and delete
review sessions from the CLI.

## Milestone 2: Immutable Git snapshots

Committed two-dot and three-dot comparisons and complete working-tree states are
captured atomically into application-owned storage.

## Milestone 3: Evidence-led review and model roles

**Exit criterion:** The review ledger retains complete pass outcomes, derives
verified and candidate lanes from evidence, supports human decisions, and
rejects unscoped chat. Model requests expose only capabilities MIRE can execute,
and explicitly configured ChatGPT Codex, OpenCode Zen, OpenCode Go, and Umans
models can run every role through the normal CLI and web application.

### V1-08 — Assemble review intent and the frozen change model

Turns a captured snapshot into a deterministic review input
that combines the diff, file and hunk inventory, affected surfaces, user intent,
and allowed repository guidance under the settled policy precedence.

### V1-09 — Produce a review plan and explainable logical slices

Run a provider-neutral planner over the change model and
persist risk areas, planned passes, required context, coverage limits, and logical
change slices that can later drive both terminal and browser navigation.

### V1-10 — Retain every emitted candidate and honest coverage

Run applicable specialized review passes with bounded,
finding-specific snapshot retrieval, retain every well-formed candidate they
emit, and report completed, failed, skipped, truncated, and unsupported work
without turning incompleteness into “no findings.”

### V1-11 — Verify candidates against an evidence floor

Investigate each candidate adversarially, record supporting
and contradictory evidence, and derive verified, candidate, and refuted views
without permitting model confidence or analyzer output alone to promote a
finding.

### V1-12 — Preserve finding identity and explicit human decisions

Give findings stable, revision-aware identities across review
rounds while letting users record dispositions and edit publishable wording
without rewriting machine evidence or history.

### V1-13 — Keep every chat turn bound to review context

Provide one durable chat timeline per session for questions,
challenges, candidate proposals, and re-verification requests, while requiring
every turn to reference an exact finding revision or validated diff selection.

### V1-14 — Run all model roles through OpenAI-compatible endpoints

Let a user configure planner, reviewer, verifier, and chat
roles against an OpenAI-compatible HTTP endpoint while preserving the common run
contract and explicit capability differences.

### V1-15 — Run all model roles through Anthropic

Lets a user route any review role to Anthropic's native API
with the same cancellation, provenance, validation, and privacy guarantees as
the OpenAI-compatible adapter.

### V1-27 — Make the bounded model-completion contract honest

**What to build:** Keep V1 model execution as a bounded, one-shot structured
completion by removing advertised application tools that the planner, reviewer,
verifier, and chat runners cannot execute. Continue assembling and retrieving
the required snapshot context before each provider request.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [ ] Planner, reviewer, verifier, and chat requests do not advertise
      `snapshot_read` or any other application tool until an application-owned
      tool loop exists.
- [ ] Each role receives only bounded context assembled or retrieved by MIRE
      before the request, and retrieved artifacts, exclusions, truncation, and
      digests remain in the durable run record.
- [ ] Provider-native structured-output mechanisms may use adapter-private
      schemas or synthetic tools without appearing in the provider-neutral
      request as MIRE application authority.
- [ ] A provider response that requests an unsupported application tool is a
      visible malformed/unsupported-output diagnostic; MIRE executes nothing and
      does not convert the run into a successful no-finding result.
- [ ] Deterministic fixtures cover all four roles through both existing wire
      adapters and prove that no application tool call is offered or executed.
- [ ] Built-in policy, prompt text, capability reports, provenance, and README
      claims consistently describe V1 as read-only pre-retrieval plus structured
      completion rather than a tool-using agent loop.

**Verification:**

- `go test ./...`
- `go test -race ./...`
- Inspect recorded requests and run provenance from fixture reviews for planner,
  reviewer, verifier, repair, and contextual-chat paths.

### V1-28 — Activate Thunderus-aligned first-party model providers

**What to build:** Let users explicitly configure ChatGPT Codex, OpenCode Zen,
OpenCode Go, and Umans for planner, reviewer, verifier, and chat roles through
the normal CLI and web application. Separate product-provider behavior from
Responses, Messages, and Chat Completions wire transports while retaining the
credential-free fixture baseline by default.

**Blocked by:** V1-27

**Acceptance criteria:**

- [ ] A single role resolver is used by CLI review, web review, re-verification,
      and contextual chat; it supports one shared model or role-specific aliases
      without importing provider DTOs into review-domain packages.
- [ ] Public provider and model conventions match pinned Thunderus contracts:
      `chatgpt-codex/<model>`, `opencode/<model>`,
      `opencode-go/<model>`, and the supported/discovered Umans IDs.
- [ ] A Responses transport supports ChatGPT Codex and the applicable OpenCode
      Zen GPT models with the same structured-output, streaming, usage, timeout,
      cancellation, retry, output-limit, repair, and redaction guarantees as the
      existing transports.
- [ ] OpenCode Zen routes GPT models to Responses, Claude/Qwen models to
      Messages, and other supported models to Chat Completions; OpenCode Go
      routes MiniMax/Qwen models to Messages and other supported models to Chat
      Completions.
- [ ] Umans uses its Messages-compatible endpoint and model metadata without
      exposing Umans wire shapes as domain records.
- [ ] `OPENCODE_ZEN_KEY`, `OPENCODE_GO_KEY`, and `UMANS_API_KEY` resolve only at
      request time or through managed user credential storage. ChatGPT Codex
      uses refreshable OAuth and the required account identity rather than
      `OPENAI_API_KEY`.
- [ ] Configuration stores credential references, never values. Repository
      content and browser requests cannot select arbitrary endpoints,
      credentials, executables, or permission-bearing provider options.
- [ ] Missing credentials, rejected authentication, unsupported models,
      incompatible capabilities, rate limits, malformed streams, and provider
      outages produce sanitized, durable incomplete states rather than a
      successful zero-finding review.
- [ ] Every run records product adapter, wire protocol, requested and resolved
      model, prompt version, parameters, input/output digests, usage when
      supplied, finish reason, redactions, and terminal cause.
- [ ] Normal commands remain credential-free and deterministic when no live
      provider is enabled. Live-provider tests are explicit and separate from
      the default suite.
- [ ] README and command help document provider selection, per-role routing,
      credential setup, model discovery, privacy implications, and ChatGPT
      Codex's experimental subscription-backed status.

**Verification:**

- `go test ./...`
- `go test -race ./...`
- Run deterministic HTTP fixtures for every provider/transport route, model-ID
  validation, authentication mode, discovery response, stream terminal state,
  retry class, malformed response, cancellation, redaction, and role selection.
- Run opt-in credentialed smoke tests for each provider without placing secrets
  or private repository data in fixtures, logs, state, or exports.

**Notes:** Align public provider behavior and pinned fixtures with Thunderus; do
not shell out to `thndrs`, import its interactive agent loop, or create a general
provider plugin framework. V1 remains a structured review pipeline.

## Milestone 4: Terminal, exports, and optional analyzers

**Exit criterion:** The native CLI renders a deterministic human review, exports
all four V1 formats from the same ledger, and can enrich a review through bounded
Setaryb and Mccabre subprocesses without making either executable mandatory.

### V1-16 — Review and inspect results in a static terminal report

Completed `mire review` and `mire show` so users receive stable progress on stderr and
a width-aware static diff with anchored findings, candidates, and incomplete-analysis diagnostics on stdout.

### V1-17 — Export one canonical ledger into all V1 formats

Lets `mire export` deterministically project a stored session into Markdown, canonical JSON,
SARIF 2.1.0, or an inspectable multi-file bundle, without implying the export can restore the private snapshot.

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

Lets `mire web [session]` serve the embedded static SvelteKit application and versioned
JSON/SSE API from one foreground Go process, using the same application service as the CLI.

### V1-22 — Explore diffs, slices, lanes, and evidence in the browser

**What to build:** Replace the SvelteKit scaffold with an accessible review
workspace for navigating rounds, logical change slices, files, unified or
side-by-side diffs, finding lanes, coverage, evidence, provenance, and omissions.

**Blocked by:** V1-10, V1-11, V1-12, V1-21

**Acceptance criteria:**

- [x] Session and round overview shows intent, status, snapshot identity,
      divergence, models, analyzers, coverage, and omissions.
- [x] Users can navigate logical slices or files and order by recorded risk,
      relevance, diff size, tests, or dependency impact without implying that
      navigation proves review coverage.
- [x] Unified and side-by-side diffs expose stable, selectable anchors and remain
      usable at ordinary laptop widths.
- [x] Verified, candidate, and optional refuted views are visually and
      semantically distinct using more than color.
- [x] Finding details expose claim, impact, anchors, supporting and contradicting
      evidence, retrieved context, run provenance, analyzer limitations, and
      relationships.
- [x] The read-only API exposes diff, slice, finding, coverage, evidence, and
      provenance resources without accepting arbitrary paths.
- [x] Keyboard navigation and focus behavior permit complete read-only
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

**Blocked by:** V1-19, V1-20, V1-21, V1-23, V1-28

**Acceptance criteria:**

- [ ] Black-box monitoring confirms committed and working-tree reviews never
      write target files or Git metadata.
- [ ] Repository instructions and analyzer output cannot grant tools, alter
      policy precedence, run arbitrary commands, contact arbitrary URLs through
      MIRE, or escape snapshot paths.
- [ ] Analyzer invocations cannot use a shell or user-controlled arbitrary
      executables and visibly disclose that enabled tools retain host authority.
- [ ] Model access is limited to configured endpoints and bounded snapshot
      context selected by MIRE before each request; no API accepts arbitrary
      commands, executable paths, project paths, provider credentials, or remote
      bind addresses.
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

**Blocked by:** V1-10, V1-11, V1-28

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

**Blocked by:** V1-06, V1-16, V1-17, V1-19, V1-20, V1-23, V1-24, V1-25, V1-28

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
