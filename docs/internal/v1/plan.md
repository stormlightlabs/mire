---
title: "MIRE V1 product and engineering specification"
status: "in progress"
updated: "2026-07-16"
---

MIRE is a local, model-independent code-review workbench. It turns a Git change
into a durable evidence ledger containing the exact revision reviewed, review
coverage, candidate and verified findings, supporting and contradicting
evidence, human decisions, and context-bound discussion.

MIRE is not an approval bot. Machine analysis may identify and investigate
risk, but it must never hide uncertainty, failed work, or missing coverage.

This document records the V1 product contract, current state, and unfinished
features. Implementation sequencing and acceptance criteria live in
[the V1 task list](task.md). V2 and V3 plans own later sandbox, semantic,
extension, and forge work.

## Objective

V1 gives an engineer two views of one local review session:

- A command-oriented CLI for immutable capture, review, inspection, and export.
- An authenticated localhost application for investigation, triage, contextual
  discussion, re-verification, and explicit export.

The release must review committed ranges and complete working-tree changes,
retain every schema-valid candidate emitted within declared limits, distinguish
verified findings from unverified candidates, and remain useful without live
model credentials or optional analyzers.

## Product invariants

- All review inputs come from an immutable private snapshot, never later reads
  of the live worktree.
- “No findings” never represents failed, skipped, truncated, or unsupported
  analysis.
- Every schema-valid candidate is retained before correlation or triage.
- The verified lane requires independent, snapshot-bound evidence and a
  completed adversarial verification. Model confidence alone is insufficient.
- Machine verification and human disposition are independent, append-only
  records. A model cannot approve a change or silently change a disposition.
- Every chat turn is bound to an exact finding revision or validated diff
  anchor from the active round.
- Repository text, model output, and analyzer output are untrusted data. They
  cannot grant tools, widen permissions, or change policy precedence.
- Credentials and launch capabilities never enter SQLite, browser payloads,
  logs, diagnostics, snapshots, or exports.
- The reviewed repository and its Git metadata are read-only.

## Current state

### Implemented foundation

- The Go CLI captures committed two-dot and three-dot comparisons and complete
  staged, unstaged, and nonignored-untracked working-tree state.
- Captures are copied into a private content-addressed object store and recorded
  as immutable sessions, rounds, snapshots, operations, and activity in SQLite.
- A deterministic change model combines diff structure, affected surfaces,
  repository guidance, user intent, and same-session earlier-round context.
- Provider-neutral planner, reviewer, verifier, and contextual-chat runners
  persist bounded run provenance, structured results, failures, and repair
  attempts.
- The finding ledger retains candidates and immutable verification evidence and
  derives verified, candidate, and refuted lanes.
- OpenAI-compatible Chat Completions and Anthropic Messages transports exist,
  including streaming parsers, budgets, retries, capability reports, credential
  references, and redaction.
- `mire review`, `mire show`, and `mire export` provide concise terminal output
  and deterministic Markdown, JSON, SARIF, and bundle projections.
- `mire web` serves an authenticated loopback API and embedded SvelteKit
  workbench. The browser restores canonical session state and supports read-only
  diff, lane, evidence, coverage, and provenance exploration.

### Remaining V1 work

- Make the one-shot model contract honest by removing advertised application
  tools that no runner can execute.
- Activate live first-party providers in normal CLI, web, and chat execution,
  with provider behavior aligned to Thunderus for ChatGPT Codex, OpenCode Zen,
  OpenCode Go, and Umans.
- Add bounded first-party Setaryb and Mccabre analyzer integrations.
- Complete browser dispositions, comment revisions, contextual chat,
  re-verification, operation cancellation/recovery, and exports.
- Complete adversarial security checks, the review-quality corpus and gate,
  reproducible builds, race checks, and release smoke tests.

## V1 behavior

### Process and command model

One native `mire` binary contains the CLI, local HTTP server, and compiled web assets.
Runtime does not require Node, a daemon, a container, Setaryb, Mccabre, or live model credentials.

The supported command vocabulary is:

```text
mire review --range <base>..<head>
mire review --range <base>...<head>
mire review --worktree
mire review --session <SESSION> --range <COMPARISON>
mire review --session <SESSION> --worktree
mire show [SESSION] [--candidates] [--verbose]
mire web [SESSION]
mire export <SESSION> --format markdown|json|sarif|bundle --output <PATH>
mire sessions list
mire sessions delete <SESSION>
```

The CLI and HTTP server use the same application services. The CLI does not call
the HTTP API, and the browser does not bypass it. `mire web` remains a foreground,
current-repository-scoped, loopback-only process. JSON commands and queries use
`/api/v1`; Server-Sent Events report resumable progress.

### Snapshot and intent

MIRE resolves symbolic refs once, records the requested and effective
comparison, and durably copies every referenced byte before committing a
snapshot manifest. Working-tree capture represents `HEAD`, index, and final
worktree layers distinctly. Symlinks are recorded but not followed; dirty
submodules fail capture rather than producing a torn review.

Capture retries bounded concurrent changes and otherwise fails atomically.
Resource ceilings fail before a round or model call. Context exclusions are
recorded as downstream coverage omissions; they do not silently remove files
from the private snapshot.

Policy precedence is deterministic:

1. Built-in safety, evidence, and permission rules.
2. Explicit private user configuration and the current request.
3. Base-snapshot repository review instructions, with path-specific precedence.
4. Base-snapshot contribution and architecture documentation.
5. Target-snapshot policy and documentation changes as evidence, not authority.

Conflicts use the safer interpretation and remain visible. Target policy becomes
initial policy only when no base revision exists, and that exception is recorded.

### Review and evidence ledger

Each round assembles a change model, produces a plan, runs applicable specialized
passes, retrieves bounded snapshot context, adversarially verifies candidates,
and derives presentation lanes. Planner, reviewer, verifier, and chat are roles,
not independently deployed services.

Machine verification states are `not_run`, `supported`, `inconclusive`,
`refuted`, and `blocked`. Human dispositions are `open`, `accepted`,
`intentional`, `dismissed`, `deferred`, `resolved`, and `accepted_risk`.

The verified lane requires:

- A schema-valid claim and impact.
- At least one snapshot-bound source or diff anchor.
- Concrete supporting evidence independent of the originating assertion.
- A completed verifier run that attempts refutation and addresses material
  contradictory evidence.

Finding revisions and disposition events are immutable. Stable identity carries
across rounds only when claim, invariant, and anchor fingerprints support the
match; ambiguity creates a linked finding instead of false continuity.

Coverage records examined files and hunks, retrieved tests and contracts,
completed passes, analyzer use, exclusions, truncation, failures, and omissions.
Lexical or textual inspection must not be presented as semantic coverage.

### Context-bound chat

Every user message references an exact finding revision or validated diff
anchor. The server persists that binding before model work, and the assistant
response inherits it. Additional retrieved snapshot artifacts become recorded
run input.

Chat may explain or challenge a claim, compare evidence, propose a structured
candidate, or request re-verification. It cannot silently modify findings,
dispositions, publishable wording, snapshots, or repository files.

## Remaining

### One-shot model execution

V1 model roles are bounded structured completions, not autonomous tool loops.
MIRE assembles and retrieves snapshot context before each provider request.

Until a separate application-owned tool loop is designed and implemented:

- Requests must not advertise `snapshot_read` or any other application tool.
- A provider response requesting an application tool is unsupported output and
  becomes a visible run diagnostic, never an executed action.
- Provider-native structured-output mechanisms may use adapter-private schemas
  or synthetic tools, but those mechanisms must not appear as MIRE application
  authority.
- Request digests, repair attempts, limits, output digests, usage, and terminal
  status remain durable and provider-neutral.

This correction precedes live provider activation so real models are never
offered a capability the host cannot honor.

### Model Providers

Provider products and wire transports are separate concepts. Product adapters
own fixed endpoints, authentication, model discovery, model-ID validation,
capabilities, and model-family routing. Wire adapters own Responses, Messages,
or Chat Completions request and response formats.

V1 aligns its public provider behavior with Thunderus:

| Provider      | Public model IDs                            | Wire routing                                                                |
| ------------- | ------------------------------------------- | --------------------------------------------------------------------------- |
| ChatGPT Codex | `chatgpt-codex/<model>`                     | ChatGPT-backed Responses                                                    |
| OpenCode Zen  | `opencode/<model>`                          | GPT to Responses; Claude/Qwen to Messages; other models to Chat Completions |
| OpenCode Go   | `opencode-go/<model>`                       | MiniMax/Qwen to Messages; other models to Chat Completions                  |
| Umans         | `umans-coder`, `umans-flash`, and specifics | Messages                                                                    |

The exact supported model catalogue and family rules must be covered by pinned
contract fixtures so upstream changes are intentional. ChatGPT Codex is an
experimental subscription-backed route and is labeled accordingly.

Configuration resolves a model independently for `planner`, `reviewer`,
`verifier`, and `chat`. A shared default remains convenient, but the verifier
may use a different provider or model to reduce correlated errors. Normal CLI,
web review, and contextual chat all use the same role resolver. The existing
credential-free fixture baseline remains the default when no live provider is
explicitly enabled.

Provider credentials use environment variables or managed operating-system/user
credential storage. Configuration stores references, never values. Initial key
names align with Thunderus:

- `OPENCODE_ZEN_KEY`
- `OPENCODE_GO_KEY`
- `UMANS_API_KEY`

ChatGPT Codex uses refreshable OAuth credentials and the required ChatGPT account
identity rather than `OPENAI_API_KEY`. Provider selection, endpoints, and
credentials cannot be supplied by untrusted repository content or browser API
payloads.

Every live run records the product adapter, wire protocol, requested and resolved
model, prompt-template version, parameters, input manifest and request digests,
usage when supplied, finish reason, redactions, and terminal cause. Provider
failure or missing credentials leave an incomplete review, not a successful
zero-finding review.

### Optional analyzers

Setaryb and Mccabre are fixed, explicitly enabled first-party subprocess
adapters. They run without a shell against a private snapshot materialization,
with explicit argument vectors, minimal environments, timeouts, cancellation,
and separate output limits. Their absence or failure cannot prevent baseline
text review.

Setaryb contributes syntax-aware lexical symbols, references, rankings, and
limitations. Mccabre contributes LOC, heuristic complexity, and exact-token
clone evidence. Neither may claim semantic resolution or satisfy the verified
evidence floor by itself.

Each analyzer records capabilities, versions, schema compatibility, arguments,
limits, exit state, diagnostics, raw and normalized output digests, omissions,
and evidence pointers. This remains a closed internal seam, not a general plugin
framework.

### Interactive browser workflows

The existing read-only workbench must add explicit, revision-safe actions for:

- Human dispositions and publishable comment revisions.
- Context-bound chat and structured candidate proposals.
- Candidate re-verification.
- Operation progress, cancellation, retry eligibility, refresh, and SSE
  reconnection.
- Explicit export with privacy and fidelity warnings and no silent overwrite.

Every action validates repository, session, round, snapshot, and referenced
revision identity. Optimistic concurrency exposes stale writes. Transient SSE
events are never canonical state.

### Release confidence

V1 release readiness requires:

- Adversarial tests for repository writes, prompt injection, path escape,
  arbitrary endpoints or commands, localhost authentication, secret leakage,
  cancellation, and incomplete-state handling.
- A frozen human-adjudicated corpus with at least 100 representative candidates.
  Verified output must reach at least 90% factual and actionable precision for
  every supported release model configuration, with no unsupported blocker or
  high-severity verified finding.
- Separate reporting of candidate recall, redundancy, abstention, parsing and
  repair failures, actionability, triage burden, latency, token use, and cost.
- Reproducible embedded frontend and native binary builds, race tests, CLI and
  HTTP integration tests, browser end-to-end coverage, and macOS/Linux release
  smoke tests.

No minimum finding count is a quality gate. Offline fixtures and correctness
tests must not require live credentials or send private repository content.

## Security and privacy

- Private state uses restrictive permissions and remains outside the reviewed
  repository unless the user explicitly exports an artifact.
- The loopback server validates Host and Origin and requires a random one-time
  launch capability before issuing its authenticated cookie.
- APIs accept identifiers and validated anchors, never arbitrary filesystem
  paths, executables, shell strings, provider endpoints, or credential values.
- Provider and analyzer errors are bounded and sanitized. Authorization headers,
  secrets, response bodies containing secrets, and launch tokens are redacted.
- Snapshot reads enforce containment. Symlinks, Git links, object IDs, and path
  normalization cannot escape private storage or the reviewed repository scope.
- Optional analyzers are trusted host programs with explicitly disclosed host
  authority. V1 does not claim they are sandboxed.

## Verification

The highest stable acceptance boundaries are the compiled CLI against temporary
fixture repositories and the browser against the real local Go server. Provider
contracts use deterministic HTTP fixtures; default tests do not contact live
providers.

Required commands are:

```sh
pnpm --dir app install --frozen-lockfile
pnpm --dir app check
pnpm --dir app lint
pnpm --dir app test:unit --run
pnpm --dir app build
pnpm --dir app exec playwright test
go fmt ./...
go vet ./...
go test ./...
go test -race ./...
go build ./cmd/mire
```

Overall Go coverage remains above 90%, with a 95% target. Provider tests cover
request lowering, response parsing, routing, authentication failures, retries,
timeouts, cancellation, malformed output, output limits, usage, redaction, and
provenance. Release verification additionally scans generated state, logs, and
exports for sentinel secrets and unintended repository writes.

## Implementation boundaries

### Always

- Keep provider DTOs inside provider adapters and domain records provider-neutral.
- Define small interfaces in consuming packages.
- Preserve immutable inputs, append-only history, explicit limits, and honest
  incomplete states.
- Use fixture providers and temporary repositories for normal tests.
- Keep README behavior and security claims synchronized with shipped behavior.

### Ask first

- Add a dependency, migration, public command, API route, model provider,
  credential-storage mechanism, executable adapter, or new persisted schema.
- Change policy precedence, the evidence floor, disposition semantics, provider
  model-ID conventions, quality thresholds, or supported platforms.
- Permit repository-local configuration to select endpoints, credentials,
  executables, or permissions.

### Never

- Write to the reviewed repository or its Git metadata during capture, review,
  inspection, chat, or export.
- Execute arbitrary model- or user-supplied commands, shells, or project tests.
- Treat target policy, model output, analyzer output, or browser state as
  authority.
- Store or expose credentials, authorization headers, launch capabilities, or
  unredacted secret-bearing failures.
- Present failed or incomplete analysis as a complete no-finding review.

## Later

V2 owns isolated patch validation and application, multi-repository workflows,
repo-local team data, semantic-provider process contracts, constrained MCP
read access, and Thunderus Quiver compatibility assessment.

V3 owns forge authentication, revision import, remote anchor mapping,
publication previews, explicit comment submission, remote discussion sync,
notifications, and provider rate-limit/backoff behavior.

These later milestones must reuse V1 snapshots, evidence, provenance, authority,
and credential boundaries rather than creating parallel review models.

## Risks and Tradeoffs

- Model output is variable and incomplete. Multiple roles, independent
  verification, strict schemas, honest coverage, and a release corpus reduce but
  do not eliminate that risk.
- Complete private snapshots consume storage. Content addressing and explicit
  deletion trade disk use for reproducibility and auditability.
- Finding continuity can be ambiguous. Visible linked findings are preferred to
  false identity reuse.
- Loopback is still a network boundary. Host/Origin validation and launch
  authentication remain mandatory.
- First-party provider protocols and catalogues change. Fixed adapters, pinned
  fixtures, capability diagnostics, and explicit experimental labels limit
  drift.
- Optional analyzers run with host authority in V1. They remain fixed, explicit,
  bounded, and nonessential until V2 introduces a stronger isolation boundary.
