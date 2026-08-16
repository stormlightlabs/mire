# Mire web review roadmap

## Outcome

`mire serve REVIEW.json` opens a local browser review surface backed by the same
durable review file as the terminal UI and CLI commands. The browser is a client,
not a second source of truth: note edits, decisions, refreshes, re-anchoring, and
revision conflicts retain Mire's existing semantics.

The first useful release should let a person understand a change, move through
its findings, record decisions, refresh the source, and export the result without
needing a hosted service or sending code off the machine.

## Product boundary

The web app is for human review of an existing Mire review. It is not a hosted PR
bot, an agent runner, or a replacement for the terminal UI.

In scope:

- An embedded SvelteKit single-page app served by the `mire` binary.
- A loopback-only Axum REST API described with Utoipa and instrumented with
  `tracing`.
- File and finding navigation, unified and split diffs, durable anchor state,
  note editing, and note decisions.
- Source-backed refresh, external review-file change detection, and clear
  optimistic-concurrency recovery.
- A deterministic review overview, readiness summary, and existing Mire export
  formats.
- Responsive keyboard- and screen-reader-usable interaction.

Out of scope for this effort:

- GitHub, GitLab, or cloud account integration.
- Pull-request approvals, branch protection, or remote status checks.
- Generating findings, summaries, fixes, or chat responses with an AI model.
- Executing suggested fixes or launching coding agents.
- Multi-user collaboration or access from another machine.
- A general-purpose REST API daemon.

## Review experience

The desktop app uses three coordinated regions:

- The file list answers where work remains and makes large changes navigable.
- The diff keeps findings attached to the evidence instead of reducing review to
  a detached issue list.
- The finding queue provides a deliberate next-item workflow across files.
- Severity, intent, author, provenance, and exact/moved anchor state expose Mire's
  existing review information well.
- Resolve, dismiss, and accept-risk map directly to durable Mire decisions.
- Revision, watch status, refresh, and export keep the review lifecycle visible.
- Unified/split modes and collapsed context support both narrow and wide reading.

Several controls need explicit product semantics before implementation:

- **Viewed:** Keep this as browser-local progress, keyed by a server-issued opaque
  review identity and file fingerprint. It is useful orientation, but is not yet
  part of the durable review schema.
- **Finish review:** Present a readiness summary and export choices. Do not write
  a new global status or imply a remote approval.
- **Reply:** The review model has editable findings and decision events, not
  discussion threads. Replace Reply with Edit in the first release. Add threads
  only after designing a durable event model for CLI and web consumers together.
- **Category:** Use Mire's annotation intent (`comment`, `defect`, `suggestion`,
  or `question`) rather than introducing a web-only category taxonomy.
- **Reviewed:** Derive this label from browser-local viewed state and open finding
  counts. Do not persist a second file status.

The implementation must also cover these states:

- initial loading, no files, no findings, binary files, and invalid review files
- missing or unsupported source bindings and refresh failures
- stale or ambiguous anchors that cannot scroll to a current line
- revision conflicts caused by another CLI, agent, or browser tab
- disconnected event streams and server shutdown
- narrow-screen file and finding drawers rather than hidden sidebars
- visible focus, disabled, pending, success, and error states that do not rely on
  color alone.

## Useful CodeRabbit ideas

CodeRabbit is most useful here as interaction research, not as a feature target.

| Idea | Mire adaptation | Timing |
| --- | --- | --- |
| Structured overview before inline findings | Deterministic source, change, finding, and re-anchor summary | First complete review loop |
| Finding types and severity | Existing annotation intent and severity, with filters and accessible labels | Initial finding experience |
| Incremental review of new pushes | Source-backed refresh plus durable note re-anchoring | Refresh/watch phase |
| Review status and pre-merge checks | Local readiness summary for open findings, unsafe anchors, and unviewed files | Completion phase |
| Guided change order instead of alphabetical files | Optional logical file groups when Mire has real relationship metadata | Later exploration |
| Contextual navigation from summary to line | Finding queue selects the file, scrolls to the anchor, and focuses the inline card | Initial finding experience |

Ideas not to copy now include PR automation, reviewer assignment, generated
labels, chat, one-click fixes, AI summaries, and model-selected review profiles.
They require remote or generative capabilities outside this local human-review
surface. Estimated effort and change diagrams should also wait until Mire can
derive them honestly rather than presenting guesses as review facts.

## User flow

1. The user creates or receives a Mire review file.
2. They run `mire serve path/to/review.json`.
3. Mire validates the review, binds `127.0.0.1` on an available port, prints a
   session URL, and serves until interrupted.
4. The SPA reads the URL-fragment secret into tab memory, removes the fragment
   from the visible URL, and loads the review overview.
5. The user filters files or findings, reads the diff, and follows inline
   findings to their durable anchors.
6. Edits and decisions include the revision the user observed. Mire atomically
   writes only if that revision is still current.
7. Source or review-file changes update the UI. Conflicts preserve unsaved input
   and ask the user to reload before retrying.
8. Finish review shows what remains and offers Markdown, JSON, and agent-context
   exports without inventing a remote approval state.

## Architecture

### Process and module boundaries

- Keep `mire-core` as the review domain and validation layer.
- Add `serve` modules inside the CLI crate for command startup, HTTP state,
  authentication, API DTOs, handlers, event watching, and embedded assets. This
  reuses the CLI's existing review-file and Git boundaries without exposing new
  workspace-wide APIs prematurely.
- Add the SvelteKit source under `crates/cli/web/`. Use SvelteKit's static adapter,
  disable SSR at the root layout, and emit `200.html` as the SPA fallback for
  Axum.
- Generate the production web bundle into a dedicated CLI asset directory and
  embed it in the binary. Release and CI checks must prove the checked-in bundle
  matches the frontend source so `cargo install` never requires Node.
- Build a Tokio runtime only for `mire serve`. Existing synchronous commands do
  not need to become async.
- Run Git and review-file blocking work through an owned blocking boundary. Every
  watcher task must have shutdown, error reporting, and a joined owner.

No database is needed. Each request reads the current validated review before a
mutation, and every write continues to use the existing atomic lock and expected
revision check.

### Server command

Initial grammar:

```text
mire serve <REVIEW> [--port <PORT>] [--open]
```

- Bind only `127.0.0.1`. Do not add a public host flag.
- Default to port `0` so the operating system chooses a free port.
- Print the URL after the listener and review are ready.
- Keep browser launching opt-in with `--open`. Print the URL for headless
  environments.
- Exit cleanly on Ctrl-C, stop watchers, and allow in-flight writes to finish.

### HTTP and asset stack

- Axum handles routing and extraction.
- `utoipa` and `utoipa-axum` define schemas while registering routes. Serve the
  generated document at `/api/v1/openapi.json`. A bundled Swagger UI is not
  required for the product.
- `tower-http` provides request tracing and response headers. Traces include a
  request ID, method, route template, status, and latency, but never authorization,
  review contents, note bodies, or the session secret.
- Embedded hashed assets receive immutable cache headers. The SPA entry point
  receives `no-cache`, and unknown non-API routes fall back to it.
- The app uses self-hosted fonts and icons only. A strict Content Security Policy
  allows assets and API connections from the same origin.

### Loopback security

Loopback binding alone is not authorization because another web page can attempt
requests to local services.

- Generate a high-entropy secret for each server process.
- Put the secret in the URL fragment, which is not sent in HTTP requests. The SPA
  reads and removes it, retaining it only for the tab session.
- Require `Authorization: Bearer` for every API request, including the event
  stream and OpenAPI document. Static assets contain no review data.
- Reject unexpected `Host` values, do not enable CORS, and require the exact
  server origin for state-changing requests.
- Use JSON request bodies and size limits. Never accept filesystem paths from API
  requests. The review path is fixed when the process starts.
- Do not put the secret in query parameters, cookies, logs, DOM content, or
  persisted browser storage.

### REST resources

The API is versioned under `/api/v1`. DTOs are explicit web contracts rather
than serialized `Review` internals.

| Method and path | Purpose |
| --- | --- |
| `GET /review` | Review identity, revision, source summary, totals, readiness, and file/finding summaries |
| `GET /files/{file_id}` | One file's metadata, semantic hunks and lines, and anchored finding summaries |
| `GET /findings/{note_id}` | Complete finding detail, provenance, decision state, and anchor outcome |
| `PATCH /findings/{note_id}` | Edit body, severity, and annotation intent at an expected revision |
| `POST /findings/{note_id}/decision` | Resolve, reopen, dismiss, or accept risk at an expected revision |
| `POST /refresh` | Refresh a source-backed review and return the resulting revision and re-anchor summary |
| `GET /events` | Authenticated server-sent events for review changes, refresh results, and shutdown |
| `GET /exports/notes.{json,md}` | Download the existing deterministic note exports |
| `GET /exports/context.json` | Download bounded agent context using existing export rules |
| `GET /openapi.json` | Download the generated OpenAPI document |

Every successful mutation returns the new revision and the changed resource.
Expected conflicts return HTTP 409 with the actual revision. Validation, missing
resources, unsupported refreshes, locked files, and internal failures use one
documented problem-details shape with stable Mire error codes.

Use the file fingerprint as the opaque `file_id`. Paths and source lines remain
byte-preserving in core; web DTOs expose display text plus a `lossy` flag. The
client must never send display paths back as identity. Binary content is a typed
file response, not an empty text diff.

### Browser application

Use one SvelteKit route and small components around explicit state modules:

- review shell and top bar
- file navigator and mobile file drawer
- review overview/readiness panel
- unified/split diff with context gaps
- inline finding card and finding editor
- finding queue and mobile finding drawer
- conflict, refresh, export, empty, and error dialogs

Keep server data separate from ephemeral UI state. The server revision, files,
findings, and anchors come from API responses. Active panes, filters, diff mode,
expanded context, and viewed fingerprints are client state. Persist only harmless
preferences and viewed fingerprints in local storage, namespaced by a stable,
server-issued opaque review identity; never persist the session secret or draft
note text. Derive that identity from the canonical review-file identity without
exposing the path itself.

Build the diff from semantic hunk and line DTOs rather than parsing patch text in
the browser. Start with bounded DOM rendering and measure real large reviews
before adding virtualization. Split alignment and context expansion need plain
TypeScript tests because they are presentation logic shared by several components.

Use semantic HTML, CSS custom-property tokens, component-scoped styles, and grid
for the desktop shell. At narrow widths, the diff remains primary while Files and
Findings open as focus-managed drawers. Severity and diff kind use text or symbols
in addition to color. Respect reduced motion and horizontal code scrolling.

### Refresh and event model

- Watch both the review file and a source-backed repository using the existing
  debounce and reload behavior as the reference.
- Coalesce noisy filesystem notifications and serialize refresh attempts.
- Broadcast only invalidation events and compact status, not complete review
  documents. Clients refetch current resources after an event.
- Use authenticated `fetch` streaming for server-sent events so the bearer secret
  never appears in a URL.
- A source refresh re-anchors through `mire-core` and writes with the observed
  revision. A conflict triggers a reread and client invalidation, never a blind
  overwrite.
- Watch failures leave the last valid review usable and visible as degraded state.

## Delivery phases

### 1. Secure read-only review

Deliver a working `mire serve`, authenticated loopback API, embedded SPA shell,
overview, file list, unified diff, inline findings, and finding navigation.

Exit when a packaged binary can open representative text, binary, empty, and
non-UTF-8-path fixtures without Node installed, and no API data is available
without the session secret.

### 2. Human decisions

Add finding editing, all existing decisions including reopen, revision-conflict
recovery, filters, keyboard navigation, and browser-local viewed progress.

Exit when web and CLI mutations interleave without lost updates and a conflict
preserves the user's draft.

### 3. Live review lifecycle

Add explicit refresh, review/source watching, event-driven invalidation, readiness,
finish-review guidance, and existing exports.

Exit when a source change re-anchors findings, external review edits appear in
the browser, watcher errors recover, and shutdown leaves no orphan task or partial
write.

### 4. Diff and responsive polish

Add split layout, context expansion, syntax highlighting if bundle and rendering
costs remain acceptable, responsive drawers, accessibility verification, and
large-review performance work justified by measurements.

Exit when the core review flow works at desktop and narrow widths, by keyboard and
screen reader, with a documented size budget and no regression in packaged CLI
startup.

## Verification strategy

- Rust unit tests cover DTO conversion, authorization, host/origin checks, error
  mapping, file identity, refresh serialization, and shutdown.
- Axum integration tests use real HTTP requests against an ephemeral loopback
  listener and temporary review files. They cover unauthorized access, conditional
  writes, conflicts, invalid input, asset fallback, and security headers.
- TypeScript unit tests cover filters, readiness, split pairing, context expansion,
  event reduction, and local viewed-state namespacing.
- Svelte browser component tests use accessible locators for file/finding
  navigation, decisions, conflict dialogs, drawers, focus restoration, and empty
  states.
- A small Playwright end-to-end suite starts the production `mire serve` binary
  and proves the critical load, navigate, decide, refresh, and export path.
- Existing core, CLI, and TUI suites remain unchanged evidence that the web path
  did not fork review semantics.

## Success measures

- A user reaches the first changed line from `mire serve` without configuration.
- Every durable browser action is immediately visible through existing CLI JSON
  commands and survives restart.
- Concurrent CLI, agent, and browser writes never silently overwrite one another.
- Refresh communicates exact, moved, stale, and ambiguous anchor outcomes.
- The shipped binary serves the app without Node, network access, or external
  assets.
- The critical review loop is usable at 360 CSS pixels and with keyboard-only
  navigation.

## Research references

Consulted on 2026-08-16:

- [CodeRabbit review walkthrough](https://docs.coderabbit.ai/pr-reviews/walkthroughs)
- [CodeRabbit review feedback and workflow](https://docs.coderabbit.ai/guides/code-review-overview)
- [CodeRabbit incremental review controls](https://docs.coderabbit.ai/configuration/auto-review)
- [CodeRabbit pre-merge checks](https://docs.coderabbit.ai/pr-reviews/pre-merge-checks)
- [CodeRabbit Change Stack](https://docs.coderabbit.ai/changelog)
- [SvelteKit single-page apps](https://svelte.dev/docs/kit/single-page-apps)
- [SvelteKit static adapter](https://svelte.dev/docs/kit/adapter-static)
- [Axum](https://docs.rs/axum/latest/axum/)
- [Utoipa Axum integration](https://docs.rs/utoipa-axum/latest/utoipa_axum/)
- [Tower HTTP tracing](https://docs.rs/tower-http/latest/tower_http/trace/)
