---
title: Live-session protocol
description: Local control for an open Mire terminal session.
section: Reference
group: Reference
order: 10
---

`mire session` controls an already-open interactive Mire process. It only
changes transient presentation state. Create findings and record dispositions
through `mire note` and `mire notes`; those commands retain their review
revision and authorship checks.

## Commands

```text
mire session list
mire session inspect SESSION
mire session focus SESSION --note NOTE_ID
mire session focus SESSION --file PATH --side old|new --start-line LINE [--end-line LINE]
mire session next SESSION
mire session previous SESSION
mire session reload SESSION
mire session walkthrough start|next|previous|stop SESSION
```

`list` reports session identifiers and whether each session has a reload source.
`inspect` reports a bounded state snapshot: the current file, selected finding,
scroll row, layout, filter summary, review revision, and walkthrough state.
Path bytes use JSON byte arrays so repository paths do not need UTF-8
conversion.

`focus` selects a finding by identifier or a changed file range. `next` and
`previous` move between visible findings. `reload` asks a watched session to run
its normal reload path; an unwatched session returns `reload_unavailable`.
Walkthrough actions let another local process mark a session as being driven
and advance or reverse its finding selection. They never write a review file.

Every command emits one JSON response with `schema_version` `{ "major": 1,
"minor": 0 }`. Successful responses have `status: "ok"`. Failures have
`status: "error"` and an error object with a stable `code`. Clients must reject
unknown major versions and may ignore fields added in later minor versions.

## Local transport and security review

Mire uses a Unix-domain socket on macOS and other Unix platforms. At TUI start,
it creates a per-user discovery directory below `$XDG_RUNTIME_DIR` when set, or
below the system temporary directory otherwise. The directory and each
descriptor have owner-only permissions. The socket has owner-only permissions
and a short random name in the temporary directory so its address fits the
platform socket-path limit. The descriptor contains that address and a random
capability. `mire session list` omits the capability; other session commands
read it locally and send it with every request.

The server accepts only local Unix-socket peers. It rejects a request with a
missing or incorrect capability before dispatching an operation. Requests and
responses are capped at 16 KiB, identifiers at 256 bytes, paths at 4 KiB, and
ranges at 10,000 lines. The server validates the schema version, operation, and
all input sizes before passing a request to the TUI loop. It returns
`invalid_request`, `unsupported_version`, `authentication_failed`,
`payload_too_large`, `session_not_found`, `interaction_busy`,
`reload_unavailable`, or `not_found` rather than exposing internal errors.

The socket protocol has no operation for note creation, editing, disposition,
review-file writes, shell execution, environment inspection, or arbitrary file
reads. State snapshots exclude note bodies, source text, environment values,
and the capability. Mire never writes the capability to normal output or error
messages. The descriptor and socket are removed when the TUI exits, including
terminal setup failures and unwinding. The listener is stopped and joined before
those files are removed.

The protocol assumes the local account is trusted. A process running as that
account can read owner-only files and use the capability; another account cannot
traverse the discovery directory or connect to its sockets. The protocol does
not cross a network boundary and Mire does not provide remote forwarding. Users
who share an operating-system account should treat all local sessions as shared.

This review considered socket-path replacement, stale discovery files,
credential disclosure, oversized JSON, malformed requests, control during
text entry or unsaved review editing, and shutdown races. The implementation
creates fresh random names, verifies discovery entries before using them,
removes stale entries from failed sessions, bounds reads before parsing JSON,
rejects control while the TUI is editing, and owns the listener thread through
the terminal session lifetime.
