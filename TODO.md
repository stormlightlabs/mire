# Mire implementation tickets

Source: [ROADMAP.md](ROADMAP.md)

## Milestone 0: Workspace foundation

Exit criterion: Cargo builds and tests all three starter crates without
third-party dependencies.

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

Exit criterion: humans, batch agents, and tools can exchange one versioned review
file without losing anchors, attribution, provenance, or decisions.

### M2.1 Define notes, anchors, and review-file recovery

**Blocked by:** M1.3

**Status:** Complete

Add review notes, statuses, authors, provenance, note events, and atomic JSON
review files around a captured changeset revision.

Acceptance criteria:

- [x] Anchors include path, side, line range, hunk fingerprint, and content
      fingerprint.
- [x] Status is one of open, resolved, dismissed, or accepted-risk.
- [x] Review writes validate first and atomically replace the destination.
- [x] Interrupted or invalid writes leave the last valid review recoverable.
- [x] Unsupported schema majors fail without rewriting the file.

Review files retain the captured changeset, a positive review revision, current
notes, and an ordered status-event history. Writers validate and serialize the
whole review before creating a sibling temporary file. They sync that file,
rename it over the destination, and sync the parent directory. If a process
stops before the rename, the previous destination remains authoritative; a
leftover `.mire-write-*` sibling may be removed after the destination loads
successfully.

Verification:

- `cargo test -p mire-core review`
- `cargo test -p mire --test review_files`

### M2.2 Import and export reviews without a TUI

**Blocked by:** M2.1

**Status:** Complete

Add CLI commands for context export, batch note import, note listing, and
JSON/Markdown export.

Acceptance criteria:

- [x] Context defaults to a compact file/hunk manifest; raw patches and
      full files require explicit bounded requests.
- [x] Batch imports are all-or-nothing and report every invalid anchor.
- [x] Human, agent, analyzer, and interchange provenance remain distinct.
- [x] JSON is deterministic and schema-versioned; Markdown is readable without
      Mire.
- [x] Every essential action has structured output and stable exit behavior.

`mire context <review>` emits file, hunk, and note identities without source
lines. `--patch` and `--file <path>` expose captured content only when paired
with `--max-bytes`. `mire notes import` validates the complete batch before an
atomic review-file replacement and returns every rejected note with a stable
error code. `mire notes list` and `mire notes export` emit a versioned,
deterministic JSON envelope; export also supports standalone Markdown.

Verification:

- `cargo test -p mire --test review_protocol`
- Round-trip a mixed human/agent/tool fixture through both formats.

### M2.3 Review and disposition notes in the TUI

**Blocked by:** M2.2, M1.6

**Status:** Complete

Add range selection, a note editor, note navigation, filters, and status changes
to the review stream.

Acceptance criteria:

- [x] A human can create, edit, resolve, dismiss, reopen, and accept risk on a
      note without changing source files.
- [x] Filters cover author kind, status, severity, annotation kind, and file.
- [x] Notes remain adjacent to code in unified and split layouts.
- [x] Keyboard and mouse parity applies to primary note actions.
- [x] Save failure keeps unsaved text available for recovery.

`mire review <review-file>` now opens an editable review stream. Range selection,
note editing, note navigation, status decisions, and facet filters all operate on
the captured changeset. Each accepted action advances the review revision and
uses the existing atomic writer. Failed saves leave the editor and in-memory
review intact so the reviewer can retry without retyping the note.

Verification:

- `cargo test -p mire-tui notes`
- `cargo test -p mire --test pty_notes`
- Complete and reload a mixed-author review manually.

## Milestone 3: Changing worktrees and live agents

Exit criterion: an open review can refresh safely, and a local agent can inspect
and annotate it through an authenticated, documented interface.

### M3.1 Watch changes without losing the review position

**Blocked by:** M1.6, M2.1

Add debounced filesystem observation with polling fallback for Git-backed and
direct-file reviews.

Acceptance criteria:

- [ ] Bursty changes coalesce into one reload and missed events are recovered.
- [ ] The selected file, logical row, filters, and layout survive when possible.
- [ ] Removed or invalid repositories show a recoverable error state.
- [ ] Watch mode terminates cleanly and does not leave background tasks.

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
