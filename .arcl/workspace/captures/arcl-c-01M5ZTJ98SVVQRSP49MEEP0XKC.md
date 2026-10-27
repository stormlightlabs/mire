+++
id = "arcl-c-01M5ZTJ98SVVQRSP49MEEP0XKC"
title = "A staged revision comparison loses --cached on refresh"
status = "captured"
created-at = "2026-10-27T23:19:13.945Z"
+++

`diff_source` in `crates/cli/src/git.rs` builds `GitOperation::Diff` for `mire review init --staged REV` without recording `staged`, and `request_from_operation` rebuilds it with `staged: false`. The first capture runs `git diff --cached REV`; every refresh runs `git diff REV`, which compares against the worktree instead of the index.
