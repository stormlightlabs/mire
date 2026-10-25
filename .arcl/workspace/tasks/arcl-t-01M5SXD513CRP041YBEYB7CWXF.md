+++
id = "arcl-t-01M5SXD513CRP041YBEYB7CWXF"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Open gh pull-request patches, including mbox form"
status = "completed"
priority = "high"
position = 4
blocked-by = ["arcl-t-01M5SXD4ZAB2KCM5B3EKPP1EB5"]
evidence = "just check passes. Series tests apply the folded diff to the base with git apply and match the final tree (renames, mode change, deletion, added-then-deleted, missing final newline). GitHub's four-commit .patch for PR 2 folds to 28 files, one entry each, matching git diff. mire patch <(git format-patch --stdout HEAD~3..HEAD) opens 103 files in the TUI, matching git diff --stat."
+++

Make GitHub pull requests open in Mire as patches from `gh`. Mire never calls `gh` or GitHub; the person or agent pipes the patch in.

`gh pr diff <n> | mire patch -` already works, because plain `gh pr diff` prints a unified diff. `gh pr diff <n> --patch` prints mbox-style commits (`From <sha> ...` headers, a commit message, then a diff per commit), and the parser rejects it with "expected a Git diff header".

Acceptance criteria:
- [x] The patch parser accepts mbox-style input from `git format-patch --stdout` and `gh pr diff --patch`, skipping each commit's headers and message.
- [x] Decide how several commits that touch the same file combine: show them in order, or fold them into one net diff. Record the decision in the docs. The owner chose folding into one net diff.
- [x] Fixtures cover real `gh` output with renames, binary files, mode changes, and several commits. A real four-commit GitHub `.patch` covers several commits; `git format-patch` series built in the tests cover renames, binary files, and mode changes.
- [x] The quick start and review guides document both `gh pr diff` forms.

Verification:
- `mire patch <(git format-patch --stdout HEAD~3..HEAD)` opens in the TUI. A piped patch produces JSON, because the viewer needs standard input.
- `just check` passes.
