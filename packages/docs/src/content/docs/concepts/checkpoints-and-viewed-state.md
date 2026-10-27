---
title: Checkpoints and viewed state
description: See how the review file records what changed between revisions and which files each reader has viewed.
section: Concepts
group: Concepts
order: 9
---

A review lives through several refreshes. Each refresh replaces the captured
changeset, so today the review file cannot say what a file looked like at an
earlier revision, what changed since a reader last looked at it, or what code a
stale note was written on. This page fixes the schema that answers those
questions. It is a design: the fields below are not written yet.

Two records answer them:

- A checkpoint records a revision where a file's fingerprint changed.
- A viewed mark records the file fingerprint a reader last marked viewed.

Both live in the review file, so they survive handoff and read the same in the
terminal, the browser and the CLI.

## Checkpoints

A file's fingerprint is the `fingerprint` of its `FileDiff`: a hash of the
file's block in the captured patch. It changes when either side of the file
changes, when the file is renamed, and when its mode changes. A refresh that
leaves every file fingerprint alone already leaves the revision alone.

The review gains a `checkpoints` list. Each entry names a revision and holds
one row per file whose fingerprint changed at that revision:

```json
{
	"revision": 4,
	"files": [
		{
			"path": "src/lib.rs",
			"fingerprint": "9f2c...",
			"old_object": "3b18e512dba79e4c8300dd08aeb37f8e728b8dad",
			"new_object": "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
			"copy": { "hunks": [] }
		},
		{ "path": "src/old.rs", "fingerprint": null, "copy": "skipped" }
	]
}
```

- `path` is the file's canonical path: the new path, or the old path for a
  deleted file.
- `fingerprint` is the file's new fingerprint, or `null` when the file left the
  changeset at that revision.
- `old_object` and `new_object` are the Git blob ids of the file's two sides,
  taken from `FileSide.object` as described below. Either is absent when the
  patch did not carry a full-length id.
- `copy` holds the file's hunks at that revision when Mire kept a patch copy,
  or `"skipped"` when it did not.
- `carried`, when `true`, marks a row that compaction moved forward: the file
  had this state at the revision but did not change there.

The review's first revision records a checkpoint with every file. Later
refreshes record only the files whose fingerprint changed. To find a file at
revision R, take the latest checkpoint at or before R that names its path.
Revisions that only add notes, edit a walkthrough or change the title record no
checkpoint.

### Blob ids

The parser reads the `index` line today only for its mode and drops the blob
ids. It will keep them in a new `FileSide` field, `object`, as a hex string.
Mire asks Git for `--full-index`, so Git sources carry full ids. A patch file
may carry abbreviated ids, which can match more than one object, so the parser
keeps only ids of 40 hex characters (SHA-1) or 64 (SHA-256) and drops the rest.

The existing `FileSide.fingerprint` does not fit. It is a `Fingerprint`, a
fixed 32-byte value, while a SHA-1 id is 20 bytes. It is also documented as a
fingerprint of the side's content, not a Git object id.

### Rebuilding old text

To show a file as it was at a checkpoint, Mire tries these in order:

1. Read both blobs from the bound repository with `git cat-file`. This works
   for revision comparisons and for staged changes, whose blobs Git has
   written.
2. Use the patch copy. A copy holds only the file's hunks, so Mire can show the
   changed lines and their context, but not the whole file.
3. Report the checkpoint as unavailable. The UI still shows that the file
   changed at that revision.

An unstaged worktree comparison is the case Git cannot cover. Git hashes the
worktree side to print the `index` line but does not store the blob, and the
next edit replaces the file on disk. When a refresh records a checkpoint, Mire
checks the new blob with `git cat-file -e`. If Git does not hold it, Mire keeps
a patch copy. Mire never writes objects into the reviewed repository.

A blob can also disappear later, when Git prunes an unreachable object. Mire
then falls back to the copy if it kept one, and otherwise reports the
checkpoint as unavailable.

### Limits

All history, checkpoint rows and patch copies together, shares one budget. Both
limits count serialized JSON bytes:

| Bound                                       | Limit  |
| ------------------------------------------- | ------ |
| One patch copy                              | 1 MiB  |
| History: `checkpoints` and `pinned` (below) | 16 MiB |

A file whose hunks exceed 1 MiB gets `"copy": "skipped"`. At the budget, a
refresh still records its checkpoint, with `"copy": "skipped"` on every row,
and compaction frees space for later copies.

A review file may not pass 128 MiB (`DEFAULT_MAX_REVIEW_FILE_BYTES`), and that
write check is what bounds the changeset, notes and walkthrough today. The
64 MiB patch limit (`DEFAULT_MAX_PATCH_BYTES`) bounds raw input, but the review
stores bytes as JSON number arrays, so a captured changeset serializes to about
five times its patch size. The note and walkthrough limits (10,000 notes of up
to 1 MiB each, and 1 MiB per walkthrough text) are per item and do not add up to
a total. History takes a fixed 16 MiB, which leaves 112 MiB for the rest. When
a write would pass 128 MiB, Mire compacts history until the file fits or only
pinned rows remain, and fails the write only then.

### Compaction

Compaction folds the oldest checkpoint into the next one. Each row of the
oldest checkpoint whose path the next checkpoint does not name moves forward
with `"carried": true`. The oldest checkpoint's copies are dropped, except
pinned ones. Mire repeats this until history fits the budget.

A note pins the row its `checkpoint` names. Before a fold, Mire moves the
pinned rows of the oldest checkpoint, with their copies, into a `pinned` list
kept under their own revision. Pinned copies are never dropped. When pinned
rows alone fill the budget, refreshes stop keeping new copies, and writes still
succeed.

After compaction the lookup above stays correct for any revision from the
oldest kept checkpoint on: a carried row holds the latest state at or before
that revision for every path the folded checkpoints named. Earlier revisions
have no answer, and the UI shows history from the oldest kept revision.

## Viewed state

The review gains a `viewed` map from a reader's id to the files that reader
marked viewed:

```json
{
	"viewed": {
		"ana": [{ "path": "src/lib.rs", "fingerprint": "9f2c...", "revision": 4 }]
	}
}
```

A file is viewed for a reader when the mark's fingerprint equals the file's
current fingerprint. When they differ, the file has changed since that reader
viewed it, and the checkpoints after the mark's revision say which revisions
changed it. A mark for a path that left the changeset stays until the reader
clears it, so the file shows as viewed again if it comes back unchanged.

The reader is the local author Mire already uses for notes written from the
terminal and the browser: `USER`, then `USERNAME`, with `local-human` as the
fallback. The CLI takes `--reader` to name another reader, so an agent can keep
its own progress without touching a person's.

Marking a file viewed names the fingerprint the reader saw, not the revision
they read. A mark therefore needs no revision check: if the file changed in the
meantime, the mark records the older fingerprint and the file shows as changed.

A mark does not advance the review revision. The revision tracks the changeset,
notes and walkthrough, and a reader's progress should not turn another writer's
pending note into a `revision_conflict`.

That makes a mark invisible to the revision check, so every write must carry
`viewed` over from disk. Today `write_review_atomic_if_revision` checks the
revision under the lock and then writes the caller's whole snapshot. A refresh
or a note write that read the file before a mark landed would erase the mark.
The implementing task changes the write path:

- Every write re-reads the file under the review file lock and copies `viewed`
  from the file on disk into the review it writes.
- Only mark operations change `viewed`. They read, change and write the file
  under the same lock, and replace it atomically.

`mire serve` sends a `viewed` event so other browser tabs update.

A reader may hold at most one mark per path, and a review at most 10,000 marks.

## Notes and checkpoints

An anchor gains an optional `checkpoint`: the revision and path of the
checkpoint row that held the anchor's file when the note was written. Mire sets
it when a note is added and when an edit moves the note to a new location.
Re-anchoring never changes it: a `moved` note still points at the code it was
written on, even after a rename.

When a note goes `stale`, Mire looks up that row, in `pinned` first and then in
`checkpoints`, and shows the code the note was written on next to the closest
current code.

Mire does not derive the checkpoint from the anchor's hunk fingerprint. Two
revisions can share a hunk fingerprint, and the stored revision is cheaper to
read and cannot be ambiguous.

Notes written before checkpoints have no `checkpoint`. For them the UI shows
only the stored anchor: its path, side and line range.

## Older and newer review files

A review file without `checkpoints` loads with a baseline: Mire records, in
memory, a checkpoint at the current revision with every current file, marked
`"baseline": true`. History before the baseline is unknown, and the UI says so.
The next write saves the baseline. A review without `viewed` has no marks.

A Mire release from before this change keeps both fields when it rewrites the
file, because the review keeps unknown fields, but it does not update them on
refresh. When a newer Mire loads a file whose current fingerprints differ from
its latest checkpoint, it records a checkpoint at the current revision with
`"gap": true`. The UI shows that the file changed somewhere between the two
revisions without naming one.

The review schema moves to minor version 6. The major version stays at 1, so a
release that reads 1.5 still reads the file.

## API

`mire serve` extends its responses. `{file_id}` is the file's current
fingerprint, so it cannot name a file that left the changeset or an older
fingerprint. History and viewed routes therefore take the file's path in a
`path` query parameter, percent-encoded from its raw bytes.

| Route                                       | Change                                                                |
| ------------------------------------------- | --------------------------------------------------------------------- |
| `GET /api/v1/review`                        | Each file gains `viewed` (the reader's mark or `null`) and `history`. |
| `PUT /api/v1/viewed?path=`                  | Marks the path viewed at the `fingerprint` in the body.               |
| `DELETE /api/v1/viewed?path=`               | Clears the reader's mark for the path.                                |
| `GET /api/v1/history?path=`                 | The path's checkpoint rows, newest first.                             |
| `GET /api/v1/history?path=&revision=`       | The path's hunks at that revision, or `checkpoint_unavailable`.       |
| `GET /api/v1/findings/{note_id}`            | The anchor gains `checkpoint`.                                        |
| `GET /api/v1/findings/{note_id}/checkpoint` | The code the note was written on, or `checkpoint_unavailable`.        |
| `GET /api/v1/events`                        | Sends `viewed` events.                                                |

`history` lists the revisions that changed the file, newest first, each with
`available` set when Mire can rebuild its text. Carried rows are left out. The
browser builds the revision cells and the checkpoint strip from it. The
server's reader is the local author; the browser cannot name another reader.

The CLI gains `mire file viewed`, `mire file unviewed` and `mire file
history`, and the JSON protocol gains matching operations. `mire context`
includes the reader's viewed marks when asked with `--reader`.

The browser drops its local viewed list. On first load after the change it
offers to move the marks it holds into the review file, once, for the files
whose fingerprint still matches.
