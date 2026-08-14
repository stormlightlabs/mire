# Changelog

## [Unreleased]

### Added

#### 2026-08-14

- Reviews initialized from a Git comparison with `mire review init`.
- Location-based note creation and atomic batches that assign durable anchors,
  require the observed review revision, and preserve authorship and provenance.
- Source-backed review refresh with conservative exact, moved, stale, and
  ambiguous re-anchoring, including watched review updates and JSON or Markdown
  exports of re-anchor outcomes and note events.
- A bundled provider-neutral review skill, installed and located with `mire skill path`.
- Local live-session controls for inspecting and navigating an open TUI,
  requesting reloads, and coordinating walkthroughs.
- Side-qualified Git locations in TUI range labels and new-note headers, using
  `a/` and `b/` to distinguish old and new diff lines.

#### 2026-07

- A normalized, versioned changeset model with stable fingerprints and JSON output.
- Patch input from files or standard input, plus read-only Git worktree,
  revision, and path-filtered comparisons through native Git commands.
- A virtualized continuous multi-file TUI with a file and hunk sidebar and
  unified, split, and responsive layouts.
- Navigation, search, context expansion, line wrapping, syntax and intraline
  highlighting, and bounded presentation caches.
- Selectable Iceberg, Eldritch, and Catppuccin theme families with light, dark,
  limited-color, and color-free behavior.
- Atomic review files with anchored notes, authorship, provenance,
  event history, human dispositions, and recovery from failed writes.
- Bounded agent-context export, hunk expansion, location-based note creation,
  atomic batches, revision-checked dispositions, full-note import, note
  listing, and JSON and Markdown review export.
- Range selection, note editing, note navigation, filtering, and status changes in the TUI.
- Native filesystem watch modes with polling fallback for Git comparisons,
  patch files, and review files while preserving review position when possible.
