+++
id = "arcl-c-01M5Z8AWPSVSRPEPYYAVHPV1MN"
title = "Watchers can record a half-written source file as a revision"
status = "discarded"
created-at = "2026-10-27T18:00:37.337Z"
+++

A watched review (`mire serve`, `mire review --watch`) refreshes once a file has been quiet for 150 ms, and the 2 s recovery reload in `crates/cli/src/watch.rs` runs even while events from a write are still settling. A writer that truncates a file and pauses before writing, as `fs::write` does under load, gets the empty file recorded as its own revision, and the finished file becomes the next one. A stalled truncate-then-write in the serve test fixture reproduced it: revision 3 instead of 2. Deferring the recovery reload while a debounce is pending would close the recovery path, but it removes the backstop for a tree that changes without a pause, so it needs a decision.
