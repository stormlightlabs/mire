---
title: Watch mode
description: Reload an open review when its Git source, patch, or review file changes.
section: Guides
group: Guides
order: 5
---

Watch mode keeps an interactive review current while another person or process
changes its source.

## Watch Git or a patch file

Add `--watch` to `diff`, `show`, or a file-backed `patch` command:

```sh
mire diff main...HEAD --watch
mire show HEAD --watch
mire patch changes.diff --watch
```

The dedicated `watch` command is shorthand for a watched Git comparison:

```sh
mire watch main...HEAD -- src
```

Watch mode requires an interactive terminal. A patch read from standard input
cannot be watched because there is no file to reload.

## Preserve review position

After a refresh, Mire keeps the selected file, nearby logical row, selected
finding, layout, and review filters when the new capture still contains their
identities. If a row no longer exists, Mire chooses the nearest useful location
rather than resetting the session to the top.

## Watch a review file

Use `--watch` when another local process may import notes or when code may
change under a source-backed review:

```sh
mire review review.json --watch
```

Mire watches both the review file and the Git source recorded by `review init`.
A source edit captures the comparison again, classifies every finding, and
atomically updates the review file.

External note writes are read before that transaction, and revision conflicts
are retried on a later watch cycle.

Mire waits until the in-terminal editor is clean before applying an external
update, so an incoming refresh does not replace unsaved note text.

## Recover from source errors

Native filesystem notifications are debounced. Mire falls back to polling when
the platform watcher is unavailable and performs periodic reloads to recover
missed events.

If a watched source disappears or becomes temporarily invalid, Mire shows the error
and continues watching. The session reloads after the source becomes valid again.

An invalid review file is a persistence error and closes the session instead
of treating corrupted durable state as a temporary source failure.
