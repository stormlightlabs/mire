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

- [ ] Add finding editing.
- [ ] Add resolve, reopen, dismiss, and accept-risk decisions.
- [ ] Enforce expected revisions on every write.
- [ ] Preserve drafts when resolving revision conflicts.
- [ ] Add finding filters and keyboard navigation.
- [ ] Store viewed-file progress in browser-local state.

## Refresh and completion

- [ ] Add explicit source refresh and re-anchoring.
- [ ] Watch source and review-file changes.
- [ ] Stream authenticated invalidation events to the SPA.
- [ ] Show refresh, watch, and degraded states.
- [ ] Add the review overview and readiness summary.
- [ ] Add the finish-review summary.
- [ ] Add Markdown, JSON, and agent-context downloads.

## Diff and interface polish

- [ ] Add split diff rendering.
- [ ] Add context expansion and hunk collapsing.
- [ ] Add responsive file and finding drawers.
- [ ] Complete keyboard, focus, screen-reader, contrast, and reduced-motion
      behavior.
- [ ] Measure large-review rendering before adding highlighting or virtualization.

## Portable patch export

- [ ] Add a deterministic Git-compatible changeset writer.
- [ ] Preserve text changes, modes, renames, copies, path bytes, and newline state.
- [ ] Add `mire review export REVIEW.json --format patch [--output PATH]`.
- [ ] Reject binary exports before writing partial output.
- [ ] Add normalized round-trip and `git apply --check` coverage.
- [ ] Document patch fidelity and unsupported binary payloads.

## Verification and release

- [ ] Add focused Rust API and server tests.
- [ ] Add TypeScript state and diff tests.
- [ ] Add Svelte browser component tests.
- [ ] Add the packaged Playwright review flow.
- [ ] Check embedded assets in CI and packaging without Node.
- [ ] Update user and security documentation.
- [ ] Run the complete Rust and frontend checks.
