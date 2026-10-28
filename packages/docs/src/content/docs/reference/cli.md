---
title: CLI Manual
description: Command and option reference for Mire.
section: Reference
group: Reference
order: 8
---

Run `mire` with no subcommand to open the current worktree diff. Pass `--theme`
before or after a subcommand, or on its own with the default diff, to select
`auto`, `iceberg`, `eldritch`, or `catppuccin`. It applies to the terminal and
to the browser review that `mire serve` opens. See
[Select a theme](/docs/guides/review-changes/#select-a-theme) for the palettes.

## Git commands

### `mire diff`

Review unstaged and untracked worktree changes by default.

```text
mire diff [--staged] [REVISION]... [--format json] [--language LANGUAGE] [--watch] [-- PATH]...
```

`--staged` compares the index with `HEAD` and cannot be combined with revision
arguments. One or more revision arguments are passed to Git as a comparison.
Paths after `--` are repository-relative filters.

Git reads a changed worktree file twice: once to hash it for the patch's
`index` line, and once for the hunks. A write between the two reads gives a
torn patch, whose hunks do not match its blob id. For a comparison that reads
the worktree, Mire rebuilds each text file's new side from the old blob and
the hunks and checks it against the blob id. A text file that comes back
binary is also suspect, since a read cut short pads it with zeros, unless a
`diff` attribute such as `binary` marks it binary. On a torn file Mire
captures again, up to five captures in total, and it does the same when Git
fails with a short read because a file shrank while Git hashed it. If a file is still torn after
the fifth, Mire keeps that capture and drops the file's new blob id. It also
drops the id, without capturing again, when the old blob is missing or the
old or rebuilt file is larger than 64 MiB, which Mire does not read to check.
The capture succeeds either way, and checkpoints treat a file without an id as
one Git does not hold. Staged and revision-to-revision comparisons read only
stored objects, so Mire does not check them.

### `mire show`

Review one commit. The revision defaults to `HEAD`.

```text
mire show [REVISION] [--format json] [--language LANGUAGE] [--watch] [-- PATH]...
```

### `mire watch`

Open a watched Git comparison.

```text
mire watch [--staged] [REVISION]... [--language LANGUAGE] [-- PATH]...
```

## Patch and review commands

### `mire patch`

```text
mire patch PATH|- [--format json] [--language LANGUAGE] [--watch]
           [--title TEXT] [--description-file PATH]
```

`-` reads a patch from standard input and produces JSON. `--watch` works only
with a file-backed patch in an interactive terminal.

`--title` and `--description-file` give the changeset a title and description,
such as a pull request's, which the TUI shows. The title is one line of at most
1 KiB, with at least one non-space character and no control characters. The
description is plain text of at most 65,536 characters, GitHub's limit for a
pull request body, read from a file. Mire turns `\r\n` and `\r` into `\n`,
drops control characters other than newline and tab, and trims trailing
whitespace before it counts characters. A blank description counts as none.
Standard input is not accepted, because the TUI needs it to be a terminal. Use
process substitution instead, such as
`--description-file <(gh pr view --json body --jq .body)`. Structured output
leaves both out.

A patch series in mbox form, as `git format-patch --stdout` and
`gh pr diff --patch` print it, is read as one net diff: commit headers and
messages are skipped, and each file's changes across the commits are folded
together.

### `mire review`

```text
mire review REVIEW.json [--format json] [--watch]
```

Without a structured format, this opens the review in the TUI. Initialize or
refresh a source-backed review with:

```text
mire review init REVIEW.json [--staged] [REVISION]... [--title TEXT] [--description-file PATH|-] [-- PATH]...
mire review refresh REVIEW.json
mire review status REVIEW.json [--format json]
mire review export REVIEW.json --format patch [--output PATH]
```

`review refresh` repeats the recorded Git comparison, re-anchors every finding,
and atomically replaces the file. It prints `status: unchanged` without writing
when the capture fingerprint has not changed. Reviews without a source binding
cannot be refreshed.

`review status` reads and validates a review without opening the TUI. Its text
output reports the captured source, review revision, changes, finding
dispositions, and re-anchor results. Pass `--format json` for deterministic
structured output.

`review export --format patch` writes the captured text changeset without review
notes or decisions. It writes to standard output by default, or atomically
replaces `PATH` with `--output`. The patch preserves normalized text changes,
modes, renames, copies, byte paths, CRLF content, and missing-final-newline
markers. It does not preserve the original diff byte stream or Git blob IDs.
Binary changes fail before any output is written.

### `mire context`

Export bounded context from a durable review:

```text
mire context REVIEW.json [--file PATH | --hunk FINGERPRINT | --patch] [--max-bytes BYTES] [--format json]
mire context REVIEW.json --reader ID
```

The compact manifest does not require `--max-bytes`. File, hunk, and patch
expansion does. The manifest includes the review's title and description when
it has them, and they count toward `--max-bytes`. Its `has_walkthrough` field
says whether the review holds a walkthrough. Every context document carries
`review_revision`, the revision a write must name. With `--reader`, the
manifest also holds `viewed`: the reader id and that reader's marks, each with
`path`, `fingerprint`, and `revision`. A file is viewed when its mark's
fingerprint equals the file's `fingerprint` in the manifest.

## File commands

```text
mire file viewed REVIEW.json --file PATH --fingerprint FINGERPRINT --revision REVISION [--reader ID]
mire file unviewed REVIEW.json --file PATH [--reader ID]
mire file unviewed REVIEW.json --all [--reader ID]
mire file history REVIEW.json --file PATH [--revision REVISION]
```

`file viewed` records that a reader viewed a file at the fingerprint and review
revision they read, both from `mire context`. Mire looks up the file at that
revision in the review's checkpoints and rejects the mark with
`viewed_mismatch` when the file had another fingerprint there or history does
not reach that revision. `file unviewed` clears the reader's mark
for the path, whether or not the path is still in the changeset. With `--all`
it clears every mark the reader holds. Neither command advances the review
revision, so marks never cause a `revision_conflict` for another writer.

`--reader` defaults to `USER`, then `USERNAME`, then `local-human`, the author
Mire uses for notes from the terminal and the browser. A person and an agent in
one shell share `USER`, so an agent should always pass `--reader`, with one
stable id per agent rather than one per session. A reader id is 1 to 256 bytes.
A reader may hold 10,000 marks, and a review may hold marks for 100 readers.
The limits apply only when a mark adds a path or a reader. Clearing a reader's
last mark frees its slot.

Both commands print the reader's mark for the path, or `null` once cleared:

```json
{
	"schema_version": { "major": 1, "minor": 5 },
	"status": "viewed",
	"review_revision": 3,
	"reader": "review-agent",
	"path": [115, 114, 99, 47, 108, 105, 98, 46, 114, 115],
	"mark": { "path": [115, 114, 99, 47, 108, 105, 98, 46, 114, 115], "fingerprint": "9f2c…", "revision": 3 }
}
```

With `--all`, the output names the reader and the number of marks cleared in
place of `path` and `mark`: `"reader": "review-agent", "cleared": 12`.

A rejected change exits with status `8` and prints a report on standard error
with the `code` `viewed_mismatch`, `invalid_path` for a path that is not a safe
repository-relative path, `invalid_text_field` for a `--reader` that is empty
or longer than 256 bytes, or `too_many_viewed_marks` or
`too_many_viewed_readers` at a limit. When another process holds the review
lock, the command exits with status `7` and prints a plain-text error on
standard error, as the note commands do. Retry after the other write ends.

`file history` lists the revisions that changed a file, newest first. The path
need not be in the current changeset. Rows that compaction carried forward are
left out:

```json
{
	"schema_version": { "major": 1, "minor": 5 },
	"review_revision": 4,
	"path": [115, 114, 99, 47, 108, 105, 98, 46, 114, 115],
	"history": [
		{
			"revision": 4,
			"fingerprint": "9f2c…",
			"old_object": "3b18e512dba79e4c8300dd08aeb37f8e728b8dad",
			"new_object": "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
			"copy": true,
			"available": true,
			"baseline": false,
			"gap": false
		}
	]
}
```

`fingerprint` is `null` at a revision where the file left the changeset.
`copy` says whether Mire kept a patch copy of the hunks, and `available`
whether it can show them, or `null` when the Git check failed and only Git
could rebuild them. `baseline` marks the checkpoint Mire recorded for a
review file written before checkpoints, and `gap` one where an older release
refreshed the review and the file changed at a revision Mire did not record.

With `--revision`, the command prints the file's hunks at that revision, the
`checkpoint_revision` of the row it found, and `source`: `capture` when the
file has not changed since, `git` when Mire read both blobs from the bound
repository, or `copy` for the patch copy. A file with no row at that revision
fails with `checkpoint_not_found`, a row whose blobs Git no longer holds and
that has no copy with `checkpoint_unavailable`, and a `--revision` of `0` with
`invalid_revision`. Each exits with status `8` and a report on standard error.

## Note commands

```text
mire note add REVIEW.json --revision REVISION --file PATH (--old-line LINE | --new-line LINE) [--end-line LINE] --author ID --provenance agent|analyzer|interchange --producer NAME --severity SEVERITY --kind KIND --body BODY
mire notes apply REVIEW.json --stdin
mire note resolve|dismiss|accept-risk REVIEW.json NOTE_ID --revision REVISION --author ID
mire notes import REVIEW.json BATCH.json|- --revision REVISION
mire notes list REVIEW.json [--format json]
mire notes export REVIEW.json [--format json|markdown]
```

`notes apply` accepts location-based findings and creates identifiers and anchor
fingerprints. It validates the whole batch before writing. Every mutation
requires the revision returned by `mire context` or `mire notes list`; stale
writes fail with `revision_conflict`. Full-note import remains available for
compatible clients. List returns deterministic JSON. Export can produce JSON
or standalone Markdown.

## Walkthrough commands

```text
mire walkthrough apply REVIEW.json --stdin
mire walkthrough show REVIEW.json [--format json]
mire walkthrough clear REVIEW.json --revision REVISION
```

`walkthrough apply` reads one JSON batch from standard input and replaces the
whole walkthrough. Mire checks every step, resolves each location to an anchor
with the note rules, and assigns step ids and fingerprints. It writes the file
only if every step is valid and the review is still at `review_revision`. The
[review model](/docs/concepts/review-model/#walkthroughs) lists the step rules
and limits.

```json
{
	"schema_version": { "major": 1, "minor": 5 },
	"review_revision": 3,
	"overview": "Move parsing behind one entry point.",
	"author": { "id": "review-agent", "display_name": null },
	"provenance": { "kind": "agent", "producer": "codex" },
	"steps": [
		{
			"id": "parser",
			"title": "New parser entry point",
			"narrative": "Callers now go through `parse`.",
			"locations": [{ "file": "src/parse.rs", "new_line": 10, "end": 24 }]
		},
		{
			"title": "Callers",
			"narrative": "Each caller drops its own parsing.",
			"locations": [{ "file": "src/main.rs", "side": "old", "start": 5 }],
			"depends_on": ["parser"],
			"diagram": "flowchart LR\n  main --> parse"
		}
	]
}
```

| Field                  | Required | Value                                                        |
| ---------------------- | -------- | ------------------------------------------------------------ |
| `schema_version`       | yes      | Protocol version. Major `1` is accepted.                     |
| `review_revision`      | yes      | The revision read from `mire context` or `walkthrough show`. |
| `overview`             | no       | Markdown read before the first step.                         |
| `author`, `provenance` | yes      | As on notes. `human` provenance is rejected.                 |
| `steps`                | yes      | One or more steps, in reading order.                         |

Each step has a one-line `title`, a Markdown `narrative`, and one or more
`locations`. `id`, `depends_on`, and a Mermaid `diagram` are optional. A step
without an `id` gets `step-<n>`. A location takes `file` (or `path`) and either
`new_line`, `old_line`, or `side` with `start`. `end` (or `end_line`) is the
inclusive last line and defaults to the first. Any other field in the batch, a
step, or a location is an error.

On success, `apply` prints `status: "applied"`, the new `review_revision`, and
the step ids in order. A rejected batch exits with status `8`. Input that is
not a batch, such as empty input, invalid UTF-8, malformed JSON, or an unknown
or missing field, prints one `mire: invalid walkthrough batch JSON` line on
standard error. Any other rejection prints a report on standard error. Each
failure has a `code`, an `error` message, and, for a step failure, the
zero-based `step_index`:

```json
{
	"schema_version": { "major": 1, "minor": 5 },
	"status": "rejected",
	"failures": [
		{ "step_index": 1, "code": "location_not_found", "error": "source location is not present on one changed hunk" }
	]
}
```

Codes include `location_not_found`, `context_only_location`,
`ambiguous_location`, `step_without_anchors`, `duplicate_step_id`,
`forward_step_dependency`, `human_provenance_forbidden`, and
`revision_conflict`, which also reports `expected` and `actual`. A batch with a
`schema_version` major other than `1` fails with `unsupported_schema_major`,
whatever its other fields. A location
with conflicting or missing line fields is a step failure with the code
`invalid_input`.

`walkthrough show` prints the overview, each step with its anchors and their
re-anchor states, and what the walkthrough leaves out: hunks no anchor covers,
and files with no hunks. Control characters other than newline and tab print
as escapes such as `\u{1b}`, so stored text cannot rewrite the terminal. Stale and ambiguous anchors cover nothing.
`--format json` prints the same as one document with `review_revision`,
`walkthrough`, and `coverage`. Without a walkthrough, the last two are `null`. Each anchor has a `state` of `original`,
`exact`, `moved`, `stale`, or `ambiguous`, the recorded `anchor`, the
`current_anchor` (`null` when stale or ambiguous), and the `reanchor_outcome`.

`walkthrough clear` removes the walkthrough if the review is still at
`--revision`, and prints `status: "cleared"` with the new revision. A review
without a walkthrough is left as it is, with `status: "unchanged"`.

## Browser review server

```text
mire serve REVIEW.json [--port PORT] [--open]
```

Starts a loopback HTTP server that serves the review in a local browser. The
server prints a session URL containing a random secret. Open that URL in a
browser to view the review, or pass `--open` to launch automatically.

The page uses the `--theme` family, in its light or dark palette as the
browser's color scheme asks; the button in the header switches between them.
Pass the flag before or after `serve`:

```sh
mire --theme catppuccin serve review.json
```

The server proxies the embedded frontend and exposes a REST API at `/api/v1/*`.
Frontend changes during development are proxied through Vite when using
`pnpm run dev` in `packages/app`. `/api/v1/openapi.json` describes every
route.

The browser marks a file viewed with the Viewed button above its diff. Marks
live in the review file, so the terminal, the CLI, and every open tab show the
same ones. The server's reader is the local author, as for `mire file viewed`
without `--reader`. Each file in `GET /api/v1/review` has `viewed`: the
reader's mark, with `fingerprint` and `revision`, or `null`. The file is viewed
while the mark's fingerprint equals the file's `id`. A file that changed after
it was marked shows as changed since viewed.

| Route                                       | Effect                                                                      |
| ------------------------------------------- | --------------------------------------------------------------------------- |
| `PUT /api/v1/viewed?path=`                  | Marks the path viewed at the `fingerprint` and `revision` in the body.      |
| `DELETE /api/v1/viewed?path=`               | Clears the reader's mark for the path, in the changeset or not.             |
| `GET /api/v1/history?path=`                 | Lists the revisions that changed the path, newest first.                    |
| `GET /api/v1/history?path=&revision=`       | Returns the path's hunks at that revision.                                  |
| `GET /api/v1/findings/{note_id}/checkpoint` | Returns the hunks of the file at the checkpoint the finding was written on. |

`path=` takes a path's `raw` field: its bytes as lowercase hex. Every path in a
response carries `raw` next to `display` and `lossy`, so a path that is not
valid UTF-8 still round-trips. A value that is not lowercase hex or not a valid
path fails with `invalid_path` (400). A body without a 64-digit hex
`fingerprint` and a positive `revision` fails with `invalid_viewed_mark` (400).
A mark for a fingerprint the file did not have at that revision fails with
`viewed_mismatch` (409). A mark past a limit fails with
`too_many_viewed_marks` or `too_many_viewed_readers` (422), and a review another
process is writing with `review_locked` (423). Neither route changes the review
revision. Each change sends one `viewed` event on `GET /api/v1/events`, and the
server also sends one when a mark written by the CLI or the terminal changes the
file without changing its revision.

Each file in `GET /api/v1/review` also has `history`: the entries
`mire file history` prints, without `old_object` and `new_object`. Each entry
has `revision`, `fingerprint`, `available`, `copy`, `baseline` and `gap`, the
same fields `GET /api/v1/history?path=` returns. The server checks the blobs
once per review revision, and `available` is `null` when that check failed. Each finding from `GET /api/v1/findings/{note_id}`
has `checkpoint`: the `revision` and `path` of the row it was written on, or
`null`. The history and checkpoint routes return `source`,
`checkpointRevision`, `fingerprint` and the hunks as `content`, or fail with
`checkpoint_not_found` or `checkpoint_unavailable` (404). A `revision` that is
not a positive number fails with `invalid_revision` (400).

```sh
# Serve a review on the default port
mire serve review.json

# Serve on a specific port
mire serve review.json --port 3737

# Open the browser automatically
mire serve review.json --open
```

See [local development](/docs/guides/local-development/) for the full workflow
with the Vite dev server.

## Agent skill

```text
mire skill path
```

Installs the bundled provider-neutral review skill in the standard
`$HOME/.agents/skills/mire` directory when needed, then prints its `SKILL.md`
path.

## Live sessions

```text
mire session list
mire session inspect SESSION
mire session focus SESSION --note NOTE_ID
mire session focus SESSION --file PATH --side old|new --start-line LINE [--end-line LINE]
mire session next|previous|reload SESSION
mire session walkthrough start|next|previous|stop SESSION
```

These commands control presentation state in a local open TUI. They do not
create, edit, or disposition findings. See the
[live-session protocol](/docs/reference/live-session-protocol/) for response
format, authentication, limits, and transport behavior.

## Interactive controls

See the [keyboard reference](/docs/reference/keybindings/) for navigation,
search, display, review-note, editor, filter, and mouse controls.

## Exit status

Argument parsing uses exit status `2`. Input I/O, patch parsing, output,
terminal, Git, review-file, and protocol failures use distinct nonzero statuses
so scripts can separate malformed input from repository and protocol errors.
Use the JSON protocol output for machine-readable failure details where
available.
