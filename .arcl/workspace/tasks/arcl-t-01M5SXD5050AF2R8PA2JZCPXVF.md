+++
id = "arcl-t-01M5SXD5050AF2R8PA2JZCPXVF"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Make the watch tests reliable under load"
status = "in_progress"
priority = "normal"
position = 2
+++

The PTY tests in `crates/cli/tests/watch.rs` hang when the machine is busy: under load the TUI does not exit after the test sends `q`, and `cargo test` waits forever. `source_backed_review_watch_refreshes_the_review_file` hung locally and `patch_watch_reloads_and_recovers_after_rename_delete_and_recreate` hung in CI. Each passes when run alone.

All three tests in the file are marked `#[ignore]` so they stop blocking `just check`. Run them with `cargo test -p mire --test watch -- --ignored`.

The tests wait with fixed sleeps (600 ms, then 2.5 s) and have no bound on how long they wait for the child, so a slow reload can swallow the quit key or outlast the sleeps.

Acceptance criteria:
- [ ] The watch tests wait for observable output (the reloaded content, or the review revision) with a deadline, instead of fixed sleeps.
- [ ] Waiting for the child to exit has a timeout that fails the test with the captured output, instead of hanging.
- [ ] Find out why `q` is lost during a reload, and fix it if it is a TUI bug rather than a test bug.
- [ ] Remove the `#[ignore]` attributes from `crates/cli/tests/watch.rs`.

Verification:
- The watch tests pass ten times in a row while `cargo build --release` runs in parallel, and in CI.
