+++
id = "arcl-c-01M5ZH03F9Z98A74NAT65WNJEH"
title = "A refresh can record a torn git diff as a revision"
status = "promoted"
created-at = "2026-10-27T20:32:01.001Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5ZR757NMG6A03DYRSA82KEA"
+++

`reload_review` in `crates/cli/src/refresh.rs` trusts one `git diff` run. Git reads a changed worktree file twice, once to hash it for the `index` line and once for the hunks, so a write that lands between the two reads yields a patch whose `index` line names the new content while its hunks show the old. The file gets a new fingerprint with unchanged hunks, and Mire records that as a revision; the next refresh records the real change as another.

Atomic renames do not prevent it. Alternating a file between two contents by rename while running `git diff --full-index` in a loop gave 25 torn patches in 2007 runs. The `mire serve` browser test failed twice in 690 runs. In the one with a trace, the 2 s recovery refresh landed during the test's source write, and the header read revision 6 instead of 5.

A fix could check each text file's `index` hash against its hunks, or rerun `git diff` until two runs agree, before writing a revision.
