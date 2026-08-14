# Changelog

## [Unreleased]

### Added

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
- Bounded agent-context export, atomic validated note import, note listing, and
  JSON and Markdown review export.
- Range selection, note editing, note navigation, filtering, and status changes in the TUI.
- Native filesystem watch modes with polling fallback for Git comparisons,
  patch files, and review files while preserving review position when possible.
