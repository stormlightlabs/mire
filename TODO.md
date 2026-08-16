# Mire web review implementation work

Work in order unless a task says otherwise. Each task should leave a usable
vertical slice, update relevant user documentation, and fit in one fresh coding
conversation. Do not begin later polish while a required review-file behavior is
unverified.

## 1. Serve an authenticated embedded review shell

**Depends on:** nothing

Add `mire serve <REVIEW> [--port <PORT>] [--open]`. Validate the review before
binding, create the Tokio runtime only for this command, bind `127.0.0.1` with an
ephemeral port by default, print the ready URL, and shut down cleanly on Ctrl-C.

Create the static SvelteKit SPA under `crates/cli/web/`, add a deterministic
production asset build, embed the generated assets in the CLI, and serve the SPA
fallback with correct MIME and cache headers. Add the minimal review header and
loading/error shell before implementing the complete review surface.

Generate a per-process secret, deliver it in the URL fragment, remove it from the
visible URL, and keep it in tab memory only. Protect `/api/v1/review` with bearer
authentication, strict Host/origin rules, no CORS, request size limits, CSP, and
redacted request tracing. Return a real overview derived from the opened review.
Include a stable opaque review identity suitable for namespacing harmless local
preferences without exposing the review's filesystem path.

Keep the route registration and DTO schema in Utoipa from the start, including the
authenticated `/api/v1/openapi.json` route.

Acceptance:

- `mire serve` rejects an invalid review without leaving a listener running.
- A valid review loads from a packaged binary without Node or network access.
- Static assets load without authentication, but no review data or OpenAPI schema
  is available with a missing or incorrect secret.
- Unknown browser routes receive the SPA fallback; unknown API routes remain 404.
- Traces contain route/status/latency and no secret, note body, diff content, or
  authorization header.
- Ctrl-C joins the server cleanly.

Verify with focused CLI grammar tests, Axum request tests, the frontend production
build/check, and a packaging smoke test.

## 2. Navigate real files and unified diffs

**Depends on:** 1

Extend the review endpoint with file summaries and add
`GET /api/v1/files/{file_id}`. Use the file fingerprint as identity. Return typed
file status, additions/deletions, semantic hunks and lines, missing-newline state,
and binary markers. Expose display-only path and line text with a lossy flag; never
accept those strings back as identity.

Build the responsive review shell, searchable/filterable file navigator, file
header, unified diff, hunk headers, collapsed context markers, and loading, empty,
binary, not-found, and malformed-response states. Selecting a file must replace
the actual diff, restore a sensible scroll position, and update browser history
without requiring another SvelteKit route.

Acceptance:

- Added, modified, deleted, renamed, copied, binary, empty, and non-UTF-8-path
  fixtures produce honest UI states.
- Line numbers and addition/deletion/context roles match the core changeset.
- File IDs remain valid only for the current review revision; a stale ID returns a
  documented conflict or not-found response rather than the wrong file.
- Search and status filters work together and have an accessible clear action.
- A narrow viewport keeps the diff visible and opens Files in a focus-managed
  drawer.

Verify DTO conversion with Rust fixtures, filters with TypeScript unit tests, and
file navigation with browser component tests using role/label locators.

## 3. Connect the finding queue to durable anchors

**Depends on:** 2

Add finding summaries to the overview and implement
`GET /api/v1/findings/{note_id}`. Include severity, annotation intent, status,
author, provenance, original/current location, and exact/moved/stale/ambiguous
re-anchor outcome without flattening those states into display strings.

Render findings inline after their anchored ranges and in a filterable queue.
Selecting a queue item changes file when needed, expands its hunk, scrolls to the
current anchor, and focuses the inline card. Stale and ambiguous findings navigate
to a clear fallback detail instead of pretending a current line exists. Add next
and previous finding commands without stealing keystrokes from inputs.

Acceptance:

- Queue counts and filters agree with the review file for every decision and
  anchor state.
- Exact and moved findings select the correct file, hunk, side, and range.
- Stale and ambiguous findings remain actionable and explain why line navigation
  is unavailable.
- Severity and state are understandable without color.
- Desktop queue selection and the narrow-screen Findings drawer restore focus
  correctly.

Verify mapping and anchor lookup in Rust, queue reduction in TypeScript, and the
cross-file selection flow in browser component tests.

## 4. Record finding edits and decisions without lost updates

**Depends on:** 3

Implement `PATCH /api/v1/findings/{note_id}` for body, severity, and annotation
intent, plus `POST /api/v1/findings/{note_id}/decision` for resolve, reopen,
dismiss, and accept-risk. Derive the human author at the server boundary and reuse
the core review methods and atomic expected-revision write.

Every request carries the observed revision. Return the new revision and changed
finding on success. Map validation failures, missing findings, lock contention,
and revision conflicts to the documented problem-details schema and stable Mire
error codes.

Add inline editing and explicit decision controls. Disable duplicate submission,
announce pending/success/failure state, and preserve a dirty draft when a 409
requires reloading current data. Do not add replies or web-only decision history.

Acceptance:

- Each real change increments the review revision once; no-op edits do not.
- Events and author attribution match existing CLI/TUI behavior.
- A CLI mutation between browser read and write produces 409 and never overwrites
  the CLI result.
- Reloading after a conflict keeps the user's draft available for comparison and
  retry.
- Decision controls support reopening and are fully keyboard operable.

Verify core/API errors and concurrent writes with temporary review files, then
cover edit, decide, no-op, validation, and conflict flows in component tests.

## 5. Add explicit refresh and live invalidation

**Depends on:** 4

Implement `POST /api/v1/refresh` using the existing source binding, Git loading,
re-anchoring, and atomic revision rules. Run blocking filesystem/Git work outside
async executor threads and serialize overlapping refreshes.

Watch the review file and bound source using the current debounce behavior as the
reference. Add authenticated `GET /api/v1/events` streaming through `fetch`, with
compact events for review invalidation, refresh progress/failure, degraded watch
state, and shutdown. Own and join watcher tasks; coalesce noisy events and refetch
resources rather than broadcasting whole reviews.

Connect Refresh and watch status in the top bar. Preserve the last valid review
when a refresh fails, and reconnect an interrupted event stream with bounded
backoff while the page remains active.

Acceptance:

- Explicit and watched refreshes produce the same re-anchor result as
  `mire review refresh`.
- External CLI/agent review-file edits invalidate all open tabs.
- Overlapping source notifications cannot create overlapping writes.
- A refresh conflict rereads current state and notifies clients; it never retries
  a stale write blindly.
- Watch failures are visible and recoverable, and shutdown leaves no watcher or
  blocking task alive.

Verify refresh/error/conflict paths with integration fixtures and deterministic
watcher tests; use one browser test for event-driven UI invalidation.

## 6. Add local review progress, readiness, and exports

**Depends on:** 5

Store viewed file fingerprints as browser-local preferences namespaced by stable
review identity. Clear entries for fingerprints no longer present after refresh.
Derive file labels and progress from viewed state plus open finding counts without
changing the review schema.

Add a deterministic overview/readiness panel inspired by CodeRabbit's useful
review summary: source, changed files and lines, findings by severity/status,
unsafe anchor outcomes, and viewed progress. Do not generate prose, effort scores,
or approval claims.

Make Finish review open a summary of unresolved findings, stale/ambiguous anchors,
and unviewed files. Allow finishing the interaction without writing a global
status. Implement authenticated downloads for existing Markdown/JSON note exports
and bounded agent context, with filenames and content types suitable for browsers.

Acceptance:

- Viewed state survives reload in the same browser but never changes review JSON
  or appears as another user's durable decision.
- Refresh removes obsolete fingerprint state and does not transfer viewed state to
  changed content accidentally.
- Readiness counts agree with the current review and update after decisions/events.
- Finish review explains remaining work and never says a PR is approved.
- Browser downloads byte-match existing deterministic CLI exports for the same
  inputs.

Verify progress/readiness as pure TypeScript, export parity in Rust, and the
finish/export flow with browser component tests.

## 7. Add split diff and context controls

**Depends on:** 3; may run after 4 while 5 and 6 proceed

Implement tested TypeScript transformation from semantic hunk lines to aligned
old/new rows, including unequal runs, context, missing-newline markers, and note
placement. Add Unified/Split controls, automatic narrow-width fallback, expand
context, collapse hunk, horizontal scrolling, and remembered layout preference.

Evaluate syntax highlighting against representative large reviews. Add it only if
it preserves exact source text, works line-wise for supported languages, remains
accessible without color, and stays within an explicit bundle/rendering budget.
Plain code with diff coloring is an acceptable first release.

Acceptance:

- Unified and split views preserve source coordinates and navigate to the same
  finding anchors.
- Unequal deletion/addition runs align deterministically without dropping lines.
- Context expansion cannot fetch or reveal content outside the captured review.
- Split mode falls back cleanly when cells become unreadable.
- Any syntax highlighter is lazy, self-contained, measured, and removable without
  changing diff semantics.

Verify pairing/context logic with unit fixtures and anchor equivalence with
browser component tests. Record bundle and representative rendering measurements
before accepting a highlighting dependency or virtualization work.

## 8. Complete responsive and accessibility behavior

**Depends on:** 4 and 7

Complete the review surface across desktop and narrow screens.
Use semantic regions and headings, tokenized colors/type/spacing, component-owned
styles, and grid/container queries. Keep the diff primary; Files and Findings
become labeled drawers with focus trap, Escape close, focus restoration, and
background inertness.

Define keyboard navigation for next/previous finding, next/previous file, open
drawer, close dialog, and focus diff. Do not trigger shortcuts while an editable
control owns the key. Add visible focus, reduced-motion behavior, live-region
announcements, minimum target sizes, and non-color status cues.

Acceptance:

- The load, navigate, inspect, decide, conflict, refresh, finish, and export flows
  work at 360 CSS pixels without hidden required controls.
- Landmarks, headings, names, descriptions, dialog behavior, and focus order are
  coherent with keyboard and a screen reader.
- Diff content remains selectable and horizontally scrollable without moving the
  whole application unexpectedly.
- Normal text and interactive states meet WCAG AA contrast; severity is never
  conveyed by color alone.
- Reduced-motion users do not receive smooth scrolling or decorative animation.

Verify with real-browser component tests for layout/focus behavior, an automated
accessibility scan, and manual keyboard/screen-reader checks documented in the PR.

## 9. Prove the packaged end-to-end review loop

**Depends on:** 5, 6, and 8

Add a small Playwright suite that launches the production `mire serve` binary
against temporary review fixtures and follows user-visible behavior. Cover load,
file and finding navigation, one decision, one conflict/reload, source refresh,
and one export. Keep API and component cases in their faster suites rather than
duplicating every edge case end to end.

Add deterministic frontend commands to the repository's normal checks, verify the
embedded asset bundle is current, and exercise packaging without a Node runtime.
Update README, installation/quick-start guidance, CLI reference, review guide, and
security notes with the final command and behavior.

Acceptance:

- The critical Playwright flow passes against the actual Rust listener and
  embedded production assets.
- CI fails when frontend source and embedded assets diverge.
- `cargo package` or the project's release-equivalent smoke check serves the app
  on a machine with no frontend toolchain.
- Existing Rust formatting, Clippy, core/CLI/TUI tests, and frontend checks pass.
- Documentation states loopback/authentication behavior, browser-local viewed
  state, conflict recovery, refresh limits, and how to stop the server.

Verify with the focused new suites first, then run the complete documented Rust
and frontend checks once for the finished deliverable.

## Later exploration

Do not schedule these until usage demonstrates the need:

- durable per-review viewed or completed state shared across CLI and web;
- threaded finding discussion and reply events;
- logical change groups or a guided change stack backed by explicit metadata;
- patch suggestions and safe handoff to an external coding agent;
- remote repository/provider integration;
- large-review virtualization beyond measured performance limits.
