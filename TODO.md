# Mire implementation work

Source: [ROADMAP.md](ROADMAP.md)

Completed work: [CHANGELOG.md](CHANGELOG.md)

## Milestone 3: Offline human-agent workflow

Exit criterion: a developer can create a durable review, give its path to an
agent, and see location-based agent findings in an open TUI. The agent never
constructs anchor or content fingerprints.

### M3.1 Initialize a durable review from a Git comparison

Add `mire review init <review-file> [<revision>...] [--staged] [-- <path>...]`.
Use the same comparison semantics and limits as `mire diff`.

Acceptance criteria:

- [x] Worktree, staged, revision-range, and path-filtered comparisons create a
      valid review at revision 1 with no notes or events.
- [x] The captured changeset is byte-for-byte equivalent to `mire diff --format json`
      for the same request.
- [x] Mire creates the destination atomically and refuses to replace an
      existing file.
- [x] Success output identifies the review path, changeset fingerprint, and
      review revision in a deterministic form.
- [x] Failures leave no partial review file and use the existing Git,
      persistence, and structured-error boundaries.
- [x] CLI help and README usage show the create-open-review flow.

Verification:

```text
cargo test -p mire --test review_init
```

Smoke-test worktree, staged, range, and filtered initialization in a disposable
fixture repository, then open each result with `mire review`.

### M3.2 Complete the bounded context and note APIs

Add a high-level input type that accepts a repository-relative path, old or new
side, inclusive line range, author, provenance, severity, annotation kind, and
body. Resolve the unique containing hunk and let `mire-core` create and validate
the durable anchor.

Expose the type through:

```text
mire context <review-file> --hunk <fingerprint> --max-bytes <bytes>
mire note add <review-file> --file <path> --new-line <line> ...
mire notes apply <review-file> --stdin
mire note resolve <review-file> <note-id>
mire note dismiss <review-file> <note-id>
mire note accept-risk <review-file> <note-id>
```

Acceptance criteria:

- [x] Callers supply source locations and review metadata. Mire assigns note
      identifiers and computes the durable anchor fingerprints.
- [x] Hunk fingerprints from the manifest select one bounded hunk expansion;
      missing or duplicate fingerprints return machine-readable errors.
- [x] Old/new side flags are mutually exclusive; ranges must lie on one side of
      one hunk.
- [x] Missing, duplicate, context-only, and ambiguous locations return stable
      machine-readable errors.
- [x] `notes apply` validates every request and writes none when any request is
      invalid.
- [x] Every mutation requires the review revision it read and rejects stale
      concurrent writes.
- [x] Agent and tool commands cannot claim human provenance or author status.
- [x] Existing full-note import remains readable for compatibility, but the
      first-party workflow uses the location-based input.
- [x] Disposition commands append attributed events and preserve all existing
      note data.

Verification:

```text
cargo test -p mire-core anchor
cargo test -p mire --test note_commands
```

Exercise a mixed batch containing valid, invalid, duplicate, and ambiguous
locations and confirm that the review file does not change.

### M3.3 Ship the first-party agent skill

Bundle a provider-neutral `SKILL.md` and expose its installed location through
`mire skill path`.

Acceptance criteria:

- [x] The skill starts with `mire context <review-file>` and expands only named
      hunks, files, or patches with an explicit byte limit.
- [x] The skill uses `notes apply` for findings and the note disposition
      commands for follow-up decisions.
- [x] Findings include a concrete location, severity, annotation kind, and
      concise evidence in the body.
- [x] The skill tells the agent to re-read the review revision before mutation
      and handle revision conflicts by inspecting the new state.
- [x] Packaging tests prove that `mire skill path` points to the shipped file.
- [x] README usage covers the human-agent handoff without provider-specific
      setup instructions.

Verification:

```text
cargo test -p mire --test skill
```

Run the skill against a fixture review with one real defect and confirm that an
already-open `mire review <file> --watch` session displays the imported note.

## Milestone 4: Reviews across changing code

Exit criterion: a source-backed review refreshes after code edits and gives
every existing finding an inspectable exact, moved, stale, or ambiguous result.

### M4.1 Persist a reloadable review source

Record a source binding when `review init` captures a Git comparison. Keep the
binding separate from the normalized changeset source description.

Acceptance criteria:

- [x] The binding records the repository identity, comparison request, and path
      filters needed to repeat the native Git operation.
- [x] Repository and path validation occurs before every reload.
- [x] Moving, deleting, or replacing the repository produces a recoverable
      error instead of reading a different source.
- [x] Older review files without a binding remain readable and are reported as
      non-refreshable.
- [x] Schema fixtures cover the optional binding and unknown-field round trips.

### M4.2 Re-anchor notes conservatively

Match each existing note against a newly captured changeset. Prefer complete
anchor identity, then accept a moved match only when path and nearby content
produce one candidate.

Acceptance criteria:

- [x] Every result is classified as exact, moved, stale, or ambiguous.
- [x] Ambiguous notes never move automatically.
- [x] The original anchor, candidate anchor, and match evidence remain
      inspectable.
- [x] Re-anchoring all notes succeeds or fails as one review transaction.
- [x] Property and adversarial tests cover duplicate code, moved hunks, renames,
      whitespace-only edits, deleted lines, and changed path filters.

Verification:

```text
cargo test -p mire-core reanchor
```

### M4.3 Refresh source-backed reviews

Add an explicit review refresh command and make `mire review <file> --watch`
observe both the review file and its bound source.

Acceptance criteria:

- [x] Refresh captures the bound comparison, re-anchors all notes, advances the
      review revision, and atomically replaces the review file.
- [x] Unchanged captures do not advance the revision or rewrite the file.
- [x] External note writes and source changes coalesce without losing either
      update.
- [x] The selected file, logical row, filters, layout, and nearest finding
      survive when their identities still exist.
- [x] Source failures appear as recoverable TUI state while review-file
      corruption remains a hard persistence error.
- [x] Context export always reports the current capture and review revision.

Verification:

```text
cargo test -p mire --test review_refresh
cargo test -p mire --test watch
```

### M4.4 Verify the complete review loop

Cover the workflow as one black-box fixture rather than adding another protocol
or persistence layer.

Acceptance criteria:

- [x] A developer initializes and opens a watched review.
- [x] The bundled skill reads bounded context and applies a batch of findings.
- [x] A source edit produces exact, moved, stale, and ambiguous outcomes.
- [x] Human dispositions survive another source refresh and agent batch.
- [x] The final JSON and Markdown exports preserve authorship, provenance,
      events, prior anchors, and re-anchor outcomes.
- [x] Documentation covers this workflow from initialization through export, and
      the README links to it.

## Milestone 5: Live TUI control

Exit criterion: a local tool can discover an open Mire session, inspect its
transient presentation state, and drive navigation or a walkthrough. The
durable note protocol remains the only path for finding creation and
disposition.

### M5.1 Define and threat-model the live-session protocol

- [ ] Specify versioned operations for session listing, state inspection,
      finding focus, navigation, reload, and coordinated walkthroughs.
- [ ] Define transport, discovery, local-user authentication, authorization,
      payload limits, lifecycle cleanup, and machine-readable errors.
- [ ] Keep comment creation and disposition on the durable review transaction
      path; the live protocol cannot impersonate a human author.
- [ ] Redact secrets and session credentials from logs and diagnostics.
- [ ] Complete a separate security review before implementation begins.

### M5.2 Implement live TUI control

- [ ] List discoverable sessions and inspect bounded presentation state.
- [ ] Focus a finding or path and range, move to the previous or next finding,
      and request a source reload.
- [ ] Authenticate every request, enforce payload bounds, and return stable
      machine-readable errors.
- [ ] Remove discovery state and terminate owned tasks on every shutdown path.

Verification:

```text
cargo test -p mire --test live_session
```

Drive inspect, focus, navigation, reload, and walkthrough flows against a PTY,
then repeat the threat-model review against the finished transport.

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

## Later candidates

- additional themes, large configuration systems, embedded providers, MCP,
  provider-specific adapters, and structural diffing.
