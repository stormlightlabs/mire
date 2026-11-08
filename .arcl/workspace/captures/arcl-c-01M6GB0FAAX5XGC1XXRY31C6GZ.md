+++
id = "arcl-c-01M6GB0FAAX5XGC1XXRY31C6GZ"
title = "Two cargo tests fail intermittently under just check"
status = "captured"
created-at = "2026-11-03T09:14:27.018Z"
+++

Each failed once in a `just check` run and passed on rerun, with no Rust changes on the branch:

- `crates/cli/tests/metadata.rs:58` (`patch_accepts_a_description_file_but_not_standard_input`) unwraps a stdin write that gets `BrokenPipe` when `mire` exits before reading its input.
- `crates/cli/tests/torn_capture.rs:84` (`a_file_replaced_during_capture_never_yields_a_torn_capture`) gets `CommandFailed` from `git diff`: "short read while indexing file.txt ... fatal: cannot hash file.txt", when the file is replaced while Git hashes it.
