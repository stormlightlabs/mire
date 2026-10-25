+++
id = "arcl-t-01M5SXD5050AF2R8PA2JZCPXVF"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Make the watch tests reliable under load"
status = "pending"
priority = "normal"
position = 2
+++

`source_backed_review_watch_refreshes_the_review_file` in `crates/cli/tests/watch.rs` hangs when the machine is busy: under load the TUI does not exit after the test sends `q`, and `cargo test` waits forever. It passes when run alone.

The watch tests wait with fixed sleeps (600 ms, then 2.5 s) and do not bound how long they wait for the child, so a slow reload can swallow the quit key or outlast the sleeps.

Acceptance criteria:
- [ ] The watch tests wait for observable output (the reloaded content, or the review revision) with a deadline, instead of fixed sleeps.
- [ ] Waiting for the child to exit has a timeout that fails the test with the captured output, instead of hanging.
- [ ] Find out why `q` is lost during a source-backed reload, and fix it if it is a TUI bug rather than a test bug.

Verification:
- The watch tests pass ten times in a row while `cargo build --release` runs in parallel.
