# Mire roadmap

## Positioning

Mire is a local code-review protocol and terminal interface for humans and
coding agents. It turns a changeset and its findings into a durable review that
can move between people, agents, and code revisions without losing attribution,
decisions, or the code each finding referred to.

The review artifact is the product. The TUI, CLI, agents, and analyzers are
clients of the same model.

```text
developer opens changed code
          ↓
agent inspects bounded context
          ↓
agent adds anchored findings
          ↓
developer reviews and dispositions them
          ↓
code changes
          ↓
Mire re-anchors each finding or marks the outcome uncertain
```

## Objective

Mire provides one local review workspace for inspecting a changeset, attaching
structured human or machine findings, and recording what happened to them. A
review retains who made each claim, which revision they inspected, and whether
the finding was resolved, dismissed, accepted as risk, moved, or made stale by
later edits.

Mire reviews every textual language through the same model. Rust is the
implementation language, not a privileged reviewed language.

## Product principles

1. **Review artifacts are durable.** Changesets, findings, decisions, and event
   history survive TUI sessions and agent handoffs.
2. **Review first.** The main view is one continuous multi-file stream. A
   sidebar navigates that stream.
3. **Read only.** Mire reads reviewed repositories and VCS metadata. It does not
   edit files, stage changes, create commits, or change repository
   configuration.
4. **One changeset model.** Git, patch input, direct files, and later VCS
   adapters normalize before rendering or annotation.
5. **Review data is separate.** A changeset describes code. A review stores
   findings and decisions. Agent context is a bounded projection for one
   consumer.
6. **Humans and tools share a protocol.** Essential review actions have
   deterministic CLI or JSON forms. The TUI uses the same validation rules.
7. **Durable anchors are Mire's responsibility.** Callers identify a path,
   side, and range. Mire resolves fingerprints and rejects missing or ambiguous
   locations.
8. **Provenance does not grant authority.** Human, agent, analyzer, and imported
   findings retain their producer identity. Machine output never becomes a
   human decision through import.
9. **Native tools own VCS semantics.** Mire invokes native Git commands without
   a shell and does not reimplement Git.
10. **Model providers stay outside Mire.** Agents use the protocol. Mire does
    not select providers, prompts, or verification models.

## Product model

The current core model has four primary concepts:

```text
Changeset   source, files, hunks, fingerprint
Anchor      side, path, line range, hunk and content fingerprints
ReviewNote  anchor, author, severity, kind, status, body, provenance
Review      captured changeset, revision, notes, and note events
```

The evolving-code workflow adds two concepts without weakening those four:

```text
SourceBinding     enough local source information to refresh a review
ReanchorOutcome   exact, moved, stale, or ambiguous
```

Important rules:

- Paths remain repository-relative byte strings internally. Display conversion
  occurs at the UI boundary.
- Line numbers alone are not durable identity. Anchors include path, side,
  range, hunk fingerprint, and selected-content fingerprint.
- Ambiguous findings never move automatically. Mire retains the original
  anchor and the evidence for every re-anchor result.
- Schemas carry explicit versions and reject unsupported major versions.
- Limits for patch size, file size, note count, and subprocess output return
  machine-readable errors.
- Review-file updates validate the whole transaction before atomic replacement.

## Intended workflow surface

The existing viewer and durable protocol provide:

```text
mire diff [<revision>...] [-- <path>...]
mire show [<revision>] [-- <path>...]
mire patch <path|->
mire review <review-file> [--watch]
mire context <review-file> [--file <path>|--hunk <id>|--patch] [--max-bytes <bytes>]
mire notes import|list|export ...
```

The next interface layer should make the durable protocol usable without
constructing Mire's internal objects:

```text
mire review init <review-file> [diff options]
mire note add <review-file> --file <path> --new-line <line> ...
mire notes apply <review-file> --stdin
mire note resolve|dismiss|accept-risk <review-file> <note-id>
mire skill path
```

Only implemented commands belong in normal CLI help. Planned commands describe
the intended interaction, not a compatibility promise.

## Delivery sequence

### Offline human-agent workflow

Make durable review creation and agent participation ordinary CLI operations:

- initialize a review directly from a Git comparison;
- accept individual and atomic-batch findings by path, side, and range while
  Mire creates the durable anchors;
- expose note dispositions through the same command boundary;
- ship a first-party agent skill that uses manifest-first, bounded context and
  the high-level note API.

Exit condition: a developer can create a review, hand its path to an agent, and
see the agent's findings appear in an open TUI without either participant
constructing anchor fingerprints.

### Reviews across changing code

Connect a durable review to the source that produced it and refresh the capture
through conservative re-anchoring:

- record a reloadable local source binding when a review is initialized;
- classify every prior finding as exact, moved, stale, or ambiguous;
- retain the prior anchor and matching evidence;
- update watched reviews when either the review file or bound source changes;
- preserve the reviewer's file, row, filters, and layout when possible.

Exit condition: the human-agent review loop survives code edits without
silently moving an uncertain finding.

### Live TUI control

Add a secured local-session protocol for transient TUI state: session
discovery, inspection of the current selection, finding focus and navigation,
reload requests, and coordinated walkthroughs.

Comment creation and disposition stay on the durable CLI protocol. Any live
control plane requires a separate threat model, local-user authentication,
bounded payloads, explicit protocol versions, and reliable session cleanup.

Exit condition: a local tool can safely discover an open Mire session, inspect
its presentation state, and drive a review walkthrough without gaining a
second path for persistent review mutations.

### Broader interoperability

After live control, widen the ways Mire can be invoked and exchange review
data:

- add pager, difftool, and direct-file modes;
- add Jujutsu and Sapling adapters behind the normalized changeset boundary;
- export SARIF and publish through optional forge adapters.

Exit condition: each adapter preserves Mire's changeset, anchor, provenance,
and review-event rules, and optional network publication never enters the core
or TUI crates.

### Review expressiveness and quality

After interoperability, add independently releasable review capabilities:

- optional confidence and evidence plus structured remediation for consumption
  by external coding agents;
- primary and related locations for findings that span several code sites;
- reviewer-quality summaries derived from provenance and dispositions;

These capabilities must reuse the changeset, anchor, provenance, and review
event rules.

## Architecture

The workspace keeps side effects at the outer boundary:

```text
crates/
  core/  mire-core: changesets, anchors, review schemas, parsing
  ui/    mire-tui: application state, rows, rendering, input
  cli/   mire: arguments, native-tool adapters, persistence, orchestration

mire -> mire-tui -> mire-core
     \------------> mire-core
```

`mire-core` has no terminal, subprocess, filesystem-watching, database, or model
provider dependency. Traits are introduced only when tests or a second
implementation need one.

## Boundaries

Always:

- preserve read-only behavior toward reviewed repositories;
- keep the core model independent of UI and provider concerns;
- validate untrusted patches, paths, schemas, and findings;
- add fixture-backed tests before widening an input or schema boundary;
- keep agent context explicit and byte-bounded.

The planned live-session transport must stay within its threat model. Ask
before adding a model provider, database, network integration beyond the
planned forge adapters, another VCS implementation, stable-schema break, or
minimum-Rust version change.

Mire never stages, rewrites, commits, or configures the reviewed repository. It
does not execute instructions from source files, patches, or imported notes.

## Later candidates

- more theme families or a large configuration system
- embedded providers, MCP, or provider-specific agent adapters
- structural diffing

## Main risks

- Re-anchoring can misplace findings. Duplicate content, moved hunks, renames,
  whitespace-only edits, and deleted lines need adversarial fixtures.
- Source bindings can become unsafe or non-portable. Refresh must validate the
  repository identity and paths before invoking native tools.
- Concurrent human and agent writes can lose decisions. Every mutation needs a
  revision precondition and atomic replacement.
- Agent reviews can create noise. Severity, kind, provenance, and human
  dispositions must remain cheap to inspect and filter.
- A live local API expands the attack surface. Its transport needs a separate
  threat model, local-user authentication, strict bounds, and reliable cleanup.
