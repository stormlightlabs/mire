# Mire implementation tickets

Source: [ROADMAP.md](ROADMAP.md)

Work one ticket per fresh implementation context. A ticket enters the frontier
when every named blocker is complete. Do not start later milestones by bypassing
an incomplete model or fixture contract.

## Milestone 0: Workspace foundation

Exit criterion: Cargo builds and tests all three starter crates without
third-party dependencies.

### M0.1 Scaffold the Cargo workspace

**Blocked by:** None - can start immediately

**Status:** Complete

Create `crates/core`, `crates/ui`, and `crates/cli`, with packages named
`mire-core`, `mire-tui`, and `mire`. Centralize the edition, minimum Rust
version, package version, and lint policy in the workspace.

Acceptance criteria:

- [x] The workspace uses Rust 2024 and resolver 3.
- [x] All three crates inherit workspace package and lint settings.
- [x] Each crate has a starter target in `src`; the CLI builds the `mire` binary.
- [x] No third-party dependency is added.

Verification:

- `cargo metadata --no-deps --format-version 1`
- `cargo check --workspace --all-targets`
- `cargo test --workspace`

## Milestone 1: Read-only review viewer

Exit criterion: `mire diff`, `mire show`, and `mire patch` open reliable,
navigable reviews and never modify the reviewed repository.

### M1.1 Lock the changeset contract and fixture corpus

**Blocked by:** M0.1

**Status:** Complete

Define the minimum `Changeset`, `FileDiff`, `Hunk`, line, source, and fingerprint
types. Add raw patch and fixture-repository cases before selecting a patch
parser.

Acceptance criteria:

- [x] The model represents byte paths, old/new sides, file status, modes,
  renames, binary markers, and missing-final-newline markers.
- [x] JSON has a schema version and deterministic ordering.
- [x] Fixtures cover every VCS and text edge case listed in the roadmap.
- [x] Invalid and oversized input has explicit expected errors.
- [x] A short decision in the ticket records whether an audited parser crate or
  a narrow local parser best passes the corpus.

Decision: use `unidiff` for Git and unified patch parsing in M1.2. Its parser
works on text, so Mire rejects invalid UTF-8 with a stable encoding error before
parsing instead of accepting replacement characters. Use `similar` for later
direct-file and intraline comparisons. Add each dependency in the ticket that
first calls it; M1.1 adds only Serde and `thiserror` for its schema and errors.

Verification:

- `cargo test -p mire-core`
- `cargo test -p mire --test fixtures`

### M1.2 Make patch input inspectable end to end

**Blocked by:** M1.1

**Status:** Complete

Implement `mire patch <path|-> --format json` so a patch becomes the normalized
changeset without entering a terminal UI.

Acceptance criteria:

- [x] File and stdin inputs produce the same canonical JSON.
- [x] Parse, encoding, limit, and I/O failures use stable non-zero exits and
  actionable stderr.
- [x] Mire never interprets patch content as arguments, paths to read, or shell
  instructions.
- [x] Golden tests cover the complete patch fixture matrix.

Verification:

- `cargo test -p mire --test patch`
- Pipe a real `git diff --no-color` into the debug binary and inspect JSON.

### M1.3 Load Git worktrees and revisions without mutation

**Blocked by:** M1.2

Implement Git-backed `diff` and `show` by invoking native Git commands without a
shell. Include untracked files for the default worktree review.

Acceptance criteria:

- [ ] Default, staged, two-dot, three-dot, path-filtered, and commit reviews match
  Git's patch semantics.
- [ ] Untracked, deleted, binary, rename, submodule, and mode changes survive
  normalization.
- [ ] Arguments remain separate OS strings and subprocess output is bounded.
- [ ] A before/after repository fingerprint proves fixtures were not modified.
- [ ] Missing Git, invalid revisions, bare repositories, and non-repositories
  have tested errors.

Verification:

- `cargo test -p mire --test git`
- Compare fixture JSON with the corresponding native Git commands.

### M1.4 Render one virtualized review stream

**Blocked by:** M1.2

Create the TUI shell and render every file and hunk in a continuous unified
stream. Keep rendering state separate from the core model.

Acceptance criteria:

- [ ] Empty, loading, ready, and error states are visible and deterministic.
- [ ] Files, hunk headers, line numbers, additions, deletions, context, binary
  files, and missing-newline markers render correctly.
- [ ] Scrolling does not build widgets for the full off-screen changeset on each
  frame.
- [ ] Terminal restoration runs after success, error, and panic paths.
- [ ] Snapshot tests cover narrow, ordinary, and wide terminals.

Verification:

- `cargo test -p mire-tui`
- `cargo test -p mire --test pty`
- Review a checked-in large patch in a real terminal.

### M1.5 Add sidebar navigation and shared layouts

**Blocked by:** M1.4

Add file/hunk navigation and unified, split, and automatic layouts driven by one
row-generation model.

Acceptance criteria:

- [ ] Selecting a sidebar file jumps within the continuous stream.
- [ ] Keyboard and mouse cover the same primary navigation actions.
- [ ] Resize preserves the logical selection and scroll anchor.
- [ ] Split and unified rows align additions, deletions, and context without
  losing source line identity.
- [ ] Help lists every active binding and conflicts are tested.

Verification:

- `cargo test -p mire-tui layout navigation`
- `cargo test -p mire --test pty`
- Manually switch layouts while reviewing a multi-file change.

### M1.6 Add review readability controls

**Blocked by:** M1.5

Add multi-language syntax and intraline highlighting, search, context expansion,
line wrapping, and theme behavior with bounded caches.

Acceptance criteria:

- [ ] Inkjet is built with an explicit language feature set rather than its
  all-languages default.
- [ ] Detection uses paths and shebangs, supports a user override, and treats
  unknown files as plain text.
- [ ] Highlighting failure falls back to readable plain diff rows without
  changing anchors.
- [ ] Search moves through matches across file boundaries.
- [ ] Context and wrapping preserve anchors and navigation targets.
- [ ] Long lines and large files stay responsive under recorded fixture
  measurements.
- [ ] Light, dark, low-color, and `NO_COLOR` output remain legible.

Verification:

- `cargo test -p mire-tui`
- Run the large-stream benchmark and record startup, memory, and frame results.
- Complete a real-terminal smoke review of Rust, TypeScript, Python, Markdown,
  an extensionless script, and an unknown format.

## Milestone 2: Durable human, agent, and tool notes

Exit criterion: humans, batch agents, and tools can exchange one versioned review
file without losing anchors, attribution, provenance, or decisions.

### M2.1 Define notes, anchors, and review-file recovery

**Blocked by:** M1.3

Add review notes, statuses, authors, provenance, note events, and atomic JSON
review files around a captured changeset revision.

Acceptance criteria:

- [ ] Anchors include path, side, line range, hunk fingerprint, and content
  fingerprint.
- [ ] Status is one of open, resolved, dismissed, or accepted-risk.
- [ ] Review writes validate first and atomically replace the destination.
- [ ] Interrupted or invalid writes leave the last valid review recoverable.
- [ ] Unsupported schema majors fail without rewriting the file.

Verification:

- `cargo test -p mire-core review`
- `cargo test -p mire --test review_files`

### M2.2 Import and export reviews without a TUI

**Blocked by:** M2.1

Add CLI commands for context export, batch note import, note listing, and
JSON/Markdown export.

Acceptance criteria:

- [ ] Context defaults to a compact file/hunk manifest; raw patches and
  full files require explicit bounded requests.
- [ ] Batch imports are all-or-nothing and report every invalid anchor.
- [ ] Human, agent, analyzer, and interchange provenance remain distinct.
- [ ] JSON is deterministic and schema-versioned; Markdown is readable without
  Mire.
- [ ] Every essential action has structured output and stable exit behavior.

Verification:

- `cargo test -p mire --test review_protocol`
- Round-trip a mixed human/agent/tool fixture through both formats.

### M2.3 Review and disposition notes in the TUI

**Blocked by:** M2.2, M1.6

Add range selection, a note editor, note navigation, filters, and status changes
to the review stream.

Acceptance criteria:

- [ ] A human can create, edit, resolve, dismiss, reopen, and accept risk on a
  note without changing source files.
- [ ] Filters cover author kind, status, severity, annotation kind, and file.
- [ ] Notes remain adjacent to code in unified and split layouts.
- [ ] Keyboard and mouse parity applies to primary note actions.
- [ ] Save failure keeps unsaved text available for recovery.

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

M1.2 is ready to start. It consumes the changeset and fixture contracts locked
by M1.1.
