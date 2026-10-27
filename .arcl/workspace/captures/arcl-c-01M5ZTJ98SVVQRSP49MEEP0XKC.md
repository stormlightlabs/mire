+++
id = "arcl-c-01M5ZTJ98SVVQRSP49MEEP0XKC"
title = "A staged revision comparison loses --cached on refresh"
status = "discarded"
created-at = "2026-10-27T23:19:13.945Z"
+++

`diff_source` in `crates/cli/src/git.rs` builds `GitOperation::Diff` for `mire review init --staged REV` without recording `staged`, and `request_from_operation` rebuilds it with `staged: false`. The first capture runs `git diff --cached REV`; every refresh runs `git diff REV`, which compares against the worktree instead of the index.

Discarded: the case cannot occur, because `--staged` conflicts with `REVISION` in `DiffArgs`, `ReviewInitArgs` and `WatchArgs` (`crates/cli/src/cli.rs`), so `mire diff --staged REV` and `mire review init --staged FILE REV` exit 2 before any capture.
