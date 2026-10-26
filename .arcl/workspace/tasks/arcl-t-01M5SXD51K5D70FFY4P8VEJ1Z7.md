+++
id = "arcl-t-01M5SXD51K5D70FFY4P8VEJ1Z7"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Caller-supplied changeset title and description"
status = "completed"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5SXD513CRP041YBEYB7CWXF"]
evidence = "just check passes. CLI tests cover review init and patch with --title/--description-file (file and stdin), context manifest fields within --max-bytes, blank descriptions, and rejected titles and descriptions. Core tests cover round trips, older files and bounds. TUI snapshots cover the header title and the i dialog; PR 1's real title and body opened through mire patch show both. The browser shows the title and a Description dialog (Playwright screenshots)."
+++

Let the caller give a changeset a title and description, so an agent can pass pull-request context from `gh pr view` without Mire knowing about GitHub.

Acceptance criteria:
- [x] `mire review init` accepts `--title <TEXT>` and `--description-file <PATH|->`. `mire patch` accepts `--title <TEXT>` and `--description-file <PATH>`; it rejects `-` because the TUI needs standard input, so callers use process substitution.
- [x] Review files store them as optional fields, and older files without them still load.
- [x] The TUI header and the browser overview show the title, and the description is viewable.
- [x] `mire context` includes both, within its byte limits.

Verification:
- `gh pr view 1 --json title,body` passed to the flags shows the title in the TUI.
- `just check` passes.
