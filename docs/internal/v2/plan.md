---
title: "MIRE V2 plan: workspaces, durable knowledge, and safe execution"
status: "planned"
updated: "2026-07-14"
---

# MIRE V2 plan

V2 turns the single-repository V1 workbench into a multi-repository workspace
and adds the first deliberately mutating workflow: an explicitly approved patch
application after isolated generation and validation. The private V1 ledger,
snapshot model, evidence rules, and foreground single-repository workflow remain
valid.

## Objective

Enable engineers to review several repositories, retain repository-owned
knowledge when they choose, run bounded project commands away from the real
worktree, validate proposed patches, and consume genuine language-semantic
evidence without weakening MIRE's auditability or human-approval boundary.

Multi-repository use is a release requirement, not an optional enhancement.

## Dependencies and current state

V2 builds on the released contracts in [the V1 plan](../v1/plan.md):

- repository-keyed sessions, snapshots, objects, findings, evidence, and runs;
- complete immutable snapshots and private snapshot materialization;
- one application service shared by the CLI and browser;
- explicit operations, provenance, cancellation, and incomplete-analysis states;
- bounded Setaryb and Mccabre process adapters; and
- private state by default with explicit export.

V2 must migrate those contracts in place. It must not create a parallel
workspace store, finding identity scheme, or review pipeline. The V1
current-repository foreground mode remains supported.

Implementation sequencing lives in [the V2 task list](task.md). Architecture-
shaping decisions called out below must be recorded in this plan before their
implementation ticket proceeds beyond its approval checkpoint.

## User outcomes

An engineer can:

1. Discover and select multiple repositories while seeing each repository's
   sessions and active operations without cross-repository leakage.
2. Keep state private by default, or explicitly promote selected policy,
   accepted risks, learnings, and exported sessions into a version-controlled
   repository representation.
3. Run tests and reproduction commands in an isolated environment constructed
   from a frozen snapshot, with declared resource, network, environment, and
   secret access.
4. Generate and validate a patch in isolation, inspect its evidence, and apply
   it to the real worktree only through a separate explicit action.
5. Use compiler-, language-server-, or dedicated-analyzer evidence while still
   seeing whether any claim is semantic, lexical, or textual.
6. Expose selected MIRE capabilities to Thunderus or another agent host without
   making an agent protocol MIRE's internal architecture.

## Product contract

### Multi-repository workspace

- Repository identity scopes every session, snapshot, object reference, policy,
  knowledge item, operation, and query.
- Workspace discovery and selection cannot silently start analysis or enable a
  repository.
- Concurrent sessions may be visible and read concurrently. State-changing and
  model-operation concurrency must remain explicit and safely leased.
- Moving between repositories must not carry policy, model context, findings,
  chat, analyzer output, or credentials across the boundary.
- The exact process topology and discovery UX are V2 design decisions; neither a
  daemon nor remote binding is implied by this plan.

### Opt-in repository-local state

- Private application state remains the default and authoritative source until
  the user explicitly promotes or imports repository-local material.
- The repository representation is versioned, reviewable, and suitable for VCS.
  It covers review policy, accepted risks, durable learnings, and selected
  session exports without exposing credentials or private runtime state.
- Promotion, import, editing, conflict handling, retention, and deletion have
  explicit semantics and append-only audit records.
- Import never silently overwrites newer private state. Unresolved conflicts are
  visible and block ambiguous promotion or use.
- The representation's paths and exact schema require approval before
  implementation; V1 must not pre-empt that decision.

### Sandboxed execution and patch flow

- Project commands run only inside a supported sandbox or container. There is no
  transparent fallback to host execution.
- V1's Setaryb and Mccabre adapters and every new semantic provider move behind
  the same fail-closed isolation boundary. V2 does not retain an unsandboxed
  analyzer fallback.
- Runs use an immutable snapshot or an explicitly derived isolated workspace,
  explicit argument vectors, bounded time and output, cancellation, cleanup, and
  recorded tool/runtime provenance.
- Network, environment, resource, and secret access are denied or constrained by
  a reviewed execution profile. Repository content cannot widen that profile.
- Test, reproduction, and patch-validation results become evidence with both
  successes and failures retained.
- Patch generation and validation occur away from the live repository. Applying
  a validated patch is a distinct, user-initiated action that checks current
  worktree divergence and reports conflicts without partial or silent changes.
- Patch application does not imply staging, committing, pushing, or approval of
  the reviewed change.

### Semantic providers

- Semantic analyzers use a versioned process contract with capability discovery,
  declared effects, structured output, cancellation, limits, provenance, and
  explicit compatibility failures.
- The first provider must demonstrate language-aware resolution beyond lexical
  occurrence matching, such as resolved symbols, imports, types, or call edges.
- Lexical Setaryb evidence and Mccabre metric evidence keep their original labels
  and limitations. They are not promoted to semantic evidence by configuration.
- Normalization into MIRE-owned evidence types prevents a provider's schema from
  becoming the domain model.

### Thunderus convergence and MCP

- MIRE's extension boundary may converge with Thunderus Quiver only after
  Quiver is implemented, versioned, and compatible with MIRE's security and
  provenance requirements.
- Until then, maintain an explicit compatibility map and avoid speculative shared
  libraries or a general plugin framework.
- Optional MCP exposure is an outer application adapter for selected read-only
  MIRE queries and existing handoff artifacts in V2. MCP is not the CLI-to-core,
  browser-to-server, analyzer, or storage protocol and cannot widen permissions.

## Success criteria

V2 is complete when:

- two or more repositories can be used concurrently with tested storage,
  policy, operation, and UI isolation;
- the V1 single-repository workflow and released data remain usable after
  migration;
- repository-local state is opt-in, versioned, round-trippable, conflict-aware,
  and free of credentials and private launch data;
- project commands cannot run on the host when isolation is unavailable;
- Setaryb, Mccabre, and semantic providers cannot run when the approved isolation
  boundary is unavailable;
- a test or reproduction run can be attached as snapshot-bound evidence;
- a patch can be generated and validated in isolation, but reaches the real
  worktree only after explicit confirmation and a successful divergence check;
- at least one selected semantic provider demonstrates resolved relationships
  beyond lexical matching against deterministic language fixtures while
  preserving unsupported and ambiguous cases;
- the extension contract has a documented Quiver compatibility assessment, with
  real convergence only if a stable upstream contract exists; and
- optional MCP access preserves repository scope, authorization, provenance, and
  human-only patch application.

## Test and verification boundaries

The highest stable boundaries are the compiled CLI against fixture repositories,
the browser against the real local Go server, and isolated command/patch runs
against disposable sandbox fixtures. Tests must cover multi-repository leakage,
store migration, repo-local merge conflicts, sandbox escape attempts, missing
runtimes, timeouts, output limits, network/secret denial, cleanup, patch
divergence, analyzer incompatibility, and MCP authorization.

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

Release verification also requires a human review of the sandbox threat model,
repo-local schema, permission prompts, patch preview/application UX, and any
claimed Quiver compatibility.

## Security and implementation boundaries

### Always

- Preserve repository scoping at storage, service, process, and UI boundaries.
- Use explicit argument vectors and record execution inputs, outputs, limits,
  runtime identity, and cleanup status.
- Treat repository-local files, tool output, and MCP input as untrusted data.
- Make unsupported isolation or semantic capability a visible failure or
  omission.

### Ask first

- Approve the repo-local paths and schema, sandbox backends and platform matrix,
  default network/secret policy, first semantic provider, and public extension
  contract before implementation commits to them.
- Approve any widening of remote bind, background-process, filesystem, network,
  credential, or patch-application authority.

### Never

- Fall back from sandboxed execution to host command execution.
- Let a sandbox, analyzer, model, MCP client, or repository instruction apply a
  patch to the real worktree without the explicit user action.
- Treat a passing test, validated patch, semantic edge, or agent response as
  approval of a change.
- Persist credentials or private application internals in repository-local state.

## Risks and open decisions

- Sandbox/container backend, supported platforms, availability behavior, and
  resource defaults are unresolved and architecture-shaping.
- The repo-local layout, schema, merge rules, and relationship to ordinary review
  policy files require a deliberate compatibility design.
- The first semantic language/provider has not been selected; the release claim
  must match what the provider can prove.
- Concurrent multi-repository model and execution workloads may require global as
  well as repository-scoped budgets.
- Patch application against a diverged, dirty, symlinked, or submodule-containing
  worktree needs conservative failure semantics.
- Thunderus Quiver is not yet a stable dependency. V2 cannot promise direct
  interoperability until its implemented contract can be tested.

## Deferred work

- Forge import, publication, and synchronization are specified in
  [the V3 plan](../v3/plan.md).
- Remote multi-user hosting, automatic patch application, autonomous Git
  mutation, and unsupervised publication are not authorized by this plan.
- Additional semantic providers follow the first provider only after the process
  contract and evidence vocabulary hold up in real use.
