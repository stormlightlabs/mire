# Mire implementation tickets

Source: [ROADMAP.md](ROADMAP.md)

## Milestone 0: Workspace foundation

Cargo builds and tests all three starter crates without third-party dependencies.

### M0.1 Scaffold the Cargo workspace

Created `crates/core`, `crates/ui`, and `crates/cli`, with packages named
`mire-core`, `mire-tui`, and `mire`. Centralize the edition, minimum Rust
version, package version, and lint policy in the workspace.

## Milestone 1: Read-only review viewer

Exit criterion: `mire diff`, `mire show`, and `mire patch` open reliable,
navigable reviews and never modify the reviewed repository.

### M1.1 Lock the changeset contract and fixture corpus

Defined the minimum `Changeset`, `FileDiff`, `Hunk`, line, source, and fingerprint
types. Add raw patch and fixture-repository cases before selecting a patch
parser.

### M1.2 Make patch input inspectable end to end

Implemented `mire patch <path|-> --format json` so a patch becomes the normalized
changeset without entering a terminal UI.

### M1.3 Load Git worktrees and revisions without mutation

Implemented Git-backed `diff` and `show` by invoking native Git commands without a
shell. Include untracked files for the default worktree review.

### M1.4 Render one virtualized review stream

Created the TUI shell and render every file and hunk in a continuous unified
stream. Keep rendering state separate from the core model.

### M1.5 Add sidebar navigation and shared layouts

Added file/hunk navigation and unified, split, and automatic layouts driven by one
row-generation model.

### M1.6 Add review readability controls

Added multi-language syntax and intraline highlighting, search, context expansion,
line wrapping, and theme behavior with bounded caches.

### M1.7 Select a built-in review theme

Added end-to-end selection for the Iceberg, Eldritch, and Catppuccin families.
Resolve each family to a dark or light palette with the terminal detector Mire
already uses.

## Milestone 2: Durable human, agent, and tool notes

humans, batch agents, and tools can exchange one versioned review file without losing
anchors, attribution, provenance, or decisions.

### M2.1 Define notes, anchors, and review-file recovery

Added review notes, statuses, authors, provenance, note events, and atomic JSON
review files around a captured changeset revision.

### M2.2 Import and export reviews without a TUI

Added CLI commands for context export, batch note import, note listing, and
JSON/Markdown export.

### M2.3 Review and disposition notes in the TUI

Added range selection, a note editor, note navigation, filters, and status changes
to the review stream.

## Milestone 3: Changing worktrees and live agents

Exit criterion: an open review can refresh safely, and a local agent can inspect
and annotate it through an authenticated, documented interface.

### M3.1 Watch changes without losing the review position

**Blocked by:** M1.6, M2.1

**Status:** Complete

Add debounced filesystem observation with polling fallback for Git-backed and
direct-file reviews.

Acceptance criteria:

- [x] Bursty changes coalesce into one reload and missed events are recovered.
- [x] The selected file, logical row, filters, and layout survive when possible.
- [x] Removed or invalid repositories show a recoverable error state.
- [x] Watch mode terminates cleanly and does not leave background tasks.

`mire watch` follows a Git worktree or revision comparison. The `diff`, `show`,
file-backed `patch`, and durable `review` commands also accept `--watch` for
callers that want to retain their existing command shape. Mire uses the
platform watcher selected by `notify`, falls back to `PollWatcher` when native
observation cannot start, debounces event bursts, and performs a periodic
recovery reload for missed events. File-backed modes observe the parent
directory so atomic replacements, renames, deletions, and recreation remain
visible.

Reloads restore presentation state by file path and logical row offset. Invalid
or missing sources replace the stream with a recoverable error while the
watcher continues running. The terminal session owns the watcher, so normal
quit and unwind paths drop its background resources before returning.

Verification:

- `cargo test -p mire --test watch`
- Edit, rename, delete, and recreate fixture files during a PTY review.

### M3.2 Re-anchor notes conservatively

**Blocked by:** M3.1, M2.3

Match notes to a new revision using exact anchors first and nearby content only
when the result is unique.

Acceptance criteria:

- [ ] Exact, moved, stale, and ambiguous outcomes are explicit states.
- [ ] Ambiguous notes never move automatically.
- [ ] Original anchor and re-anchor evidence remain inspectable.
- [ ] Property and adversarial tests cover duplicate code, moved hunks, renames,
      whitespace-only edits, and deleted lines.

Verification:

- `cargo test -p mire-core reanchor`
- Manually reload a review containing every anchor outcome.

### M3.3 Expose a secured live-session protocol

**Blocked by:** M2.2, M3.2

Let a local process list sessions, inspect bounded context, navigate, reload, and
apply agent notes while the TUI is open.

Acceptance criteria:

- [ ] The protocol has an explicit version and machine-readable errors.
- [ ] Endpoints bind only to loopback, authenticate each request, bound payloads,
      and redact secrets from logs.
- [ ] Batch note application uses the same validation as offline import.
- [ ] Agents cannot create human-attributed notes or bypass stale-anchor checks.
- [ ] Session shutdown removes discovery data and terminates owned tasks.

Verification:

- `cargo test -p mire --test live_session`
- Drive inspect, navigate, reload, and batch comment flows against a PTY session.
- Complete a separate threat-model review before release.

## Milestone 4: Broader interoperability

Exit criterion: each added adapter reuses the shared changeset and review
contracts without weakening read-only or provenance rules.

### M4.1 Add pager, difftool, and direct-file modes

**Blocked by:** M1.6, M3.1

Support pager-safe patch viewing, Git difftool invocation, and direct comparison
of two files.

Acceptance criteria:

- [ ] Redirected output, non-interactive terminals, and quit behavior match the
      selected mode.
- [ ] Difftool operation handles one file without assuming a repository-wide
      patch.
- [ ] Direct-file watch mode refreshes both paths.

Verification:

- `cargo test -p mire --test integration_modes`
- Smoke-test as a Git pager and difftool in a disposable fixture repository.

### M4.2 Add Jujutsu and Sapling adapters

**Blocked by:** M1.3, M4.1

Detect or explicitly select Jujutsu and Sapling, invoke their native Git-format
diff commands, and normalize the result.

Acceptance criteria:

- [ ] Native revsets and path filters remain separate subprocess arguments.
- [ ] Explicit VCS selection overrides detection.
- [ ] Missing tools and unsupported output fail without falling back silently.
- [ ] Shared patch fixtures prove model parity with Git-backed input.

Verification:

- `cargo test -p mire --test vcs_adapters`
- Run adapter smoke tests where each tool is available.

### M4.3 Export SARIF and publish through optional adapters

**Blocked by:** M2.2

Export applicable tool annotations and review notes to SARIF. Design forge
publication as a separate, explicit adapter after the local review contract is
stable.

Acceptance criteria:

- [ ] SARIF validates against its schema and preserves locations, severity,
      rules, and provenance.
- [ ] Unsupported note fields produce a visible warning rather than silent loss.
- [ ] Network publication is opt-in, previewable, idempotent, and never part of
      core or TUI crates.

Verification:

- `cargo test -p mire --test sarif`
- Validate fixture exports with an independent SARIF validator.

## Current frontier

M3.1 can start now that durable note interaction is complete.
