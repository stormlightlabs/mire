# Mire web review TODO

Implementation details and completion criteria live in [ROADMAP.md](ROADMAP.md).
Work from top to bottom unless dependencies allow otherwise.

## Secure review shell

- [x] Add `mire serve <REVIEW> [--port <PORT>] [--open]`.
- [x] Add the loopback Axum server and clean shutdown.
- [x] Add bearer authentication, host/origin checks, request limits, and security
      headers.
- [x] Register the REST API and OpenAPI schemas with Utoipa.
- [x] Add redacted HTTP tracing.
- [x] Create the static SvelteKit SPA and embed its production assets in the CLI.
- [x] Load the review overview from the API.

## Files and findings

- [x] Add file summaries and semantic diff responses.
- [x] Build file search, filtering, and navigation.
- [x] Render unified text diffs and binary, empty, and error states.
- [x] Add finding summaries and detail responses.
- [x] Render inline findings and the finding queue.
- [x] Navigate exact and moved findings to their anchors.
- [x] Handle stale and ambiguous findings.

## Review actions

- [x] Add finding editing.
- [x] Add resolve, reopen, dismiss, and accept-risk decisions.
- [x] Enforce expected revisions on every write.
- [x] Preserve drafts when resolving revision conflicts.
- [x] Add finding filters and keyboard navigation.
- [x] Store viewed-file progress in browser-local state.

## Refresh and completion

- [x] Add explicit source refresh and re-anchoring.
- [x] Watch source and review-file changes.
- [x] Stream authenticated invalidation events to the SPA.
- [x] Show refresh, watch, and degraded states.
- [x] Add the review overview and readiness summary.
- [x] Add the finish-review summary.
- [x] Add Markdown, JSON, and agent-context downloads.

## Diff and interface polish

- [ ] Add split diff rendering.
- [ ] Add context expansion and hunk collapsing.
- [ ] Add responsive file and finding drawers.
- [ ] Complete keyboard, focus, screen-reader, contrast, and reduced-motion
      behavior.
- [ ] Measure large-review rendering before adding highlighting or virtualization.
- [ ] Add dark mode with toggle

## Portable patch export

- [x] Add a deterministic Git-compatible changeset writer.
- [x] Preserve text changes, modes, renames, copies, path bytes, and newline state.
- [x] Add `mire review export REVIEW.json --format patch [--output PATH]`.
- [x] Reject binary exports before writing partial output.
- [x] Add normalized round-trip and `git apply --check` coverage.
- [x] Document patch fidelity and unsupported binary payloads.

## Verification and release

- [ ] Add focused Rust API and server tests.
- [ ] Add TypeScript state and diff tests.
- [ ] Add Svelte browser component tests.
- [ ] Add the packaged Playwright review flow.
- [ ] Check embedded assets in CI and packaging without Node.
- [ ] Update user and security documentation.
- [ ] Run the complete Rust and frontend checks.

## Parking Lot

- [ ] Add comments to review findings
