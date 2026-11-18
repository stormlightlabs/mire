+++
id = "arcl-t-01M7NZ4C01SJWJTP47XGSPZ55H"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Stop two cargo tests failing intermittently"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #31 at 535899f: metadata test ignores BrokenPipe, torn-capture test accepts Git short reads after five retries; metadata 0 of 50 failed"
+++

Each failed once in a `just check` run and passed on rerun, with no Rust changes on the branch:

- `crates/cli/tests/metadata.rs:58` (`patch_accepts_a_description_file_but_not_standard_input`) unwraps a stdin write that gets `BrokenPipe` when `mire` exits before reading its input.
- `crates/cli/tests/torn_capture.rs:84` (`a_file_replaced_during_capture_never_yields_a_torn_capture`) gets `CommandFailed` from `git diff`: "short read while indexing file.txt ... fatal: cannot hash file.txt", when the file is replaced while Git hashes it.

Acceptance criteria:
- [ ] The metadata test tolerates `BrokenPipe` on the stdin write and still checks the exit status and message.
- [ ] A capture whose `git diff` keeps failing with a short read either succeeds or fails with that error, and the torn-capture test still fails on any stored blob id that does not match its hunks.
- [ ] The docs say what happens when every capture attempt hits a short read.
- [ ] Both tests pass 50 runs in a row.
