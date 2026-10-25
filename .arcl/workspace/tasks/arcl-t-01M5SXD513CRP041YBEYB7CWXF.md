+++
id = "arcl-t-01M5SXD513CRP041YBEYB7CWXF"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Open gh pull-request patches, including mbox form"
status = "pending"
priority = "high"
position = 4
blocked-by = ["arcl-t-01M5SXD4ZAB2KCM5B3EKPP1EB5"]
+++

Make GitHub pull requests open in Mire as patches from `gh`. Mire never calls `gh` or GitHub; the person or agent pipes the patch in.

`gh pr diff <n> | mire patch -` already works, because plain `gh pr diff` prints a unified diff. `gh pr diff <n> --patch` prints mbox-style commits (`From <sha> ...` headers, a commit message, then a diff per commit), and the parser rejects it with "expected a Git diff header".

Acceptance criteria:
- [ ] The patch parser accepts mbox-style input from `git format-patch --stdout` and `gh pr diff --patch`, skipping each commit's headers and message.
- [ ] Decide how several commits that touch the same file combine: show them in order, or fold them into one net diff. Record the decision in the docs.
- [ ] Fixtures cover real `gh` output with renames, binary files, mode changes, and several commits.
- [ ] The quick start and review guides document both `gh pr diff` forms.

Verification:
- `git format-patch --stdout HEAD~3..HEAD | mire patch -` opens in the TUI.
- `just check` passes.
