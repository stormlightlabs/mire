+++
id = "arcl-t-01M5ZR757NMG6A03DYRSA82KEA"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Never store blob ids from torn git diff captures"
status = "completed"
priority = "high"
position = 0
evidence = "Done in #19 at e206073: worktree captures verify each new blob id by rebuilding and hashing; still-torn, over-cap or binary-turned files keep their capture without the id; no captures fail."
+++

`reload_review` in `crates/cli/src/refresh.rs` trusts one `git diff` run. Git reads a changed worktree file twice, once to hash it for the `index` line and once for the hunks, so a write between the two reads gives a patch whose `index` line names the new content while its hunks show the old. Mire then records a revision whose fingerprint changed with unchanged hunks, and the checkpoint row stores a blob id that does not match its hunks, so rebuilding that revision from Git shows the wrong text.

Alternating a file between two contents by rename while running `git diff --full-index` in a loop gave 25 torn patches in 2007 runs. The `mire serve` browser test failed twice in 690 runs from it.

Acceptance criteria:
- [ ] Mire never stores a blob id it could not check against the file's hunks. It captures again, a bounded number of times, while a file is torn, then keeps the capture without that file's new blob id rather than failing.
- [ ] Init, refresh and the watch reload all use the check.

Verification:
- A test that writes a torn patch (or drives the race) shows the refresh rejects it, and a looped capture against a file changing by rename records no torn revision.
- The browser test passes 100 runs in a row, and `just check` passes.
