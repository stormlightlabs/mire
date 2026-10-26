+++
id = "arcl-t-01M5SXD51K5D70FFY4P8VEJ1Z7"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Caller-supplied changeset title and description"
status = "in_progress"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5SXD513CRP041YBEYB7CWXF"]
+++

Let the caller give a changeset a title and description, so an agent can pass pull-request context from `gh pr view` without Mire knowing about GitHub.

Acceptance criteria:
- [ ] `mire patch` and `mire review init` accept `--title <TEXT>` and `--description-file <PATH|->`.
- [ ] Review files store them as optional fields, and older files without them still load.
- [ ] The TUI header and the browser overview show the title, and the description is viewable.
- [ ] `mire context` includes both, within its byte limits.

Verification:
- `gh pr view 1 --json title,body` piped into the flags shows the title in the TUI.
- `just check` passes.
