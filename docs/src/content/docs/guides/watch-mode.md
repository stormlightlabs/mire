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

After a refresh, Mire keeps the selected file, nearby logical row, layout, and
review filters when the new changeset still contains them. If the exact row no
longer exists, Mire chooses the nearest useful location rather than resetting
the session to the top.

## Watch a review file

Use `--watch` when another local process may import notes or otherwise replace a
review file:

```sh
mire review review.json --watch
```

Mire waits until the in-terminal editor is clean before applying an external
update, so an incoming refresh does not replace unsaved note text.

## Recover from source errors

Native filesystem notifications are debounced. Mire falls back to polling when
the platform watcher is unavailable and performs periodic reloads to recover
missed events.

If a watched file or repository disappears or becomes temporarily invalid,
Mire shows the error and continues watching. The session reloads after the
source becomes valid again.
