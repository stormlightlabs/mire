# Mire implementation work

Source: [ROADMAP.md](ROADMAP.md)

## Milestone 6: Broader interoperability

Exit criterion: Mire works as a pager, difftool, direct-file reviewer, or
supported VCS client and can exchange findings through SARIF without weakening
the core review rules.

### M6.1 Add pager, difftool, and direct-file modes

- [ ] Redirected output, noninteractive input, and quit behavior match the
      selected mode.
- [ ] Difftool mode accepts one file pair without assuming a repository-wide
      patch.
- [ ] Direct-file mode watches both input paths and preserves TUI state across
      reloads.

Verification:

```text
cargo test -p mire --test integration_modes
```

Smoke-test pager and difftool invocation in a disposable fixture repository.

### M6.2 Add Jujutsu and Sapling adapters

- [ ] Pass native revision selectors and path filters as separate subprocess
      arguments.
- [ ] Support explicit VCS selection and deterministic repository detection.
- [ ] Missing or unsupported tools fail clearly without silently falling back
      to Git.
- [ ] Shared patch fixtures produce changesets equivalent to Git where the
      compared content is the same.

Verification:

```text
cargo test -p mire --test vcs_adapters
```

Smoke-test each adapter where its native tool is available.

### M6.3 Export SARIF and publish through optional forge adapters

- [ ] SARIF validates against its schema and preserves locations, severity,
      rule identity, and provenance.
- [ ] Unsupported review fields produce visible warnings instead of silent
      data loss.
- [ ] Network publication is opt-in, previewable, idempotent, and isolated from
      `mire-core` and `mire-tui`.

Verification:

```text
cargo test -p mire --test sarif
```

Validate exported fixtures with an independent SARIF validator.

## Milestone 7: Review expressiveness and quality

After Milestone 6:

- [ ] Add optional confidence, evidence, and structured remediation for
      suggestions.
- [ ] Add related locations for findings that span multiple code sites.
- [ ] Derive reviewer-quality summaries from provenance and dispositions.

## Milestone 8: Polish

Exit criterion: Location, selection, findings, review progress, and live state
are apparent in the TUI, long reviews are quick to navigate, and the primary CLI
paths require little command discovery.

### M8.1 Make actions contextual and `Esc` predictable

- [x] The footer shows actions for the active mode and item under the cursor,
      including source, range selection, finding, search, and editor states.
- [x] Narrow terminals retain the most relevant actions without truncating the
      active-mode cue.
- [x] `Esc` closes or cancels the active UI state in order: editor, filter,
      search, range selection, help, then sidebar focus.
- [x] `q` remains the quit command and `Esc` does not quit from the base state.

Verification:

```text
cargo test -p mire-tui
```

Exercise each mode and narrow-width footer in a Herdr pane.

### M8.2 Make findings and source state spatially distinct

- [x] Finding rows lead with status, severity, kind, author, and body instead of
      rendering every action as persistent toolbar chrome.
- [x] Finding actions appear when the finding is selected and remain available
      through keyboard and mouse input.
- [x] The gutter gives cursor position, active range, and finding anchors
      distinct visual treatments in unified and split layouts.
- [x] Open, resolved, dismissed, and accepted-risk findings remain identifiable
      without relying on color alone.

Verification:

```text
cargo test -p mire-tui
```

Inspect focused and unfocused findings, multiline ranges, and narrow layouts in
a Herdr pane.

### M8.3 Support multiline note editing and paste

Blocked by: None - can start immediately.

- [ ] `Enter` inserts a newline and `Ctrl-Enter` saves the note.
- [ ] Bracketed paste inserts the complete pasted text without interpreting it
      as key commands.
- [ ] `Tab` and `Shift-Tab` move focus among the body, severity, and kind fields;
      field controls change only the focused value.
- [ ] Editing, validation failures, retry, and cancellation preserve the user's
      full text and classification choices.

Verification:

```text
cargo test -p mire-tui
cargo test -p mire --test pty_notes
```

Paste a multiline comment and edit every field in a Herdr pane.

### M8.4 Show review progress and live state

Blocked by: None - can start immediately.

- [ ] The review stream shows a proportional scroll indicator and a compact
      position summary for the current file and overall review.
- [ ] Durable reviews show the number of open findings.
- [ ] Watch mode distinguishes watching, successful refresh, and refresh failure
      without discarding the preserved review position.
- [ ] Live walkthroughs show that they control navigation, current walkthrough
      progress, and the action that returns control to the user.

Verification:

```text
cargo test -p mire-tui
cargo test -p mire --test watch
cargo test -p mire --test live_session
```

Observe refresh and walkthrough transitions in a Herdr pane.

### M8.5 Collapse files and hunks

Blocked by: None - can start immediately.

- [ ] File and hunk disclosure glyphs reflect expanded and collapsed state.
- [ ] `Enter`, `Space`, and the disclosure hit target toggle the structural row
      under the cursor.
- [ ] Collapsed rows report hidden lines and preserve anchored findings.
- [ ] Navigation, search, filters, reload, and layout changes preserve collapse
      state where the underlying file or hunk still exists.

Verification:

```text
cargo test -p mire-tui
```

Collapse files and hunks containing findings, then navigate and reload the
review in a Herdr pane.

### M8.6 Add fast file jump and review-aware file navigation

Blocked by: None - can start immediately.

- [ ] A keyboard-opened file picker filters changed paths incrementally and
      jumps to the selected file.
- [ ] Picker results show file status, change counts, and open-finding counts.
- [ ] Sidebar rows show open and completed finding counts, with the highest open
      severity visible when space permits.
- [ ] The picker and sidebar remain usable with non-UTF-8 paths, large file
      counts, narrow terminals, and mouse input.

Verification:

```text
cargo test -p mire-tui
```

Navigate a large fixture review by picker, sidebar, keyboard, and mouse in a
Herdr pane.

### M8.7 Make the zero-argument CLI useful

Blocked by: None - can start immediately.

- [ ] Running `mire` with no subcommand opens the current worktree diff with the
      same behavior as `mire diff`.
- [ ] Explicit subcommands and noninteractive output retain their current
      parsing and exit behavior.
- [ ] Path arguments carry Clap path hints so the generated completion scripts
      offer filesystem completion where appropriate.

Verification:

```text
cargo test -p mire --test git
cargo test -p mire --test pty
```

Compare `mire` and `mire diff` in a disposable fixture repository.

### M8.8 Report review status without opening the TUI

Blocked by: None - can start immediately.

- [ ] `mire review status <review-file>` reports source, revision, file and
      change totals, finding dispositions, and re-anchor outcomes.
- [ ] Human output is concise and deterministic structured output is available
      to scripts and agents.
- [ ] Status uses the review-file validation and limits already applied by other
      review commands.

Verification:

```text
cargo test -p mire --test review_files
cargo test -p mire --test review_protocol
```

## Later candidates

- additional themes, large configuration systems, embedded providers, MCP,
  provider-specific adapters, and structural diffing;
- external-editor note composition and a general command palette if the focused
  file picker does not cover the common navigation need.
