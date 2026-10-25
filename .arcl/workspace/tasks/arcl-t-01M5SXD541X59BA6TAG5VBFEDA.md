+++
id = "arcl-t-01M5SXD541X59BA6TAG5VBFEDA"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "mire mcp stdio server"
status = "pending"
priority = "high"
position = 10
blocked-by = ["arcl-t-01M5SXD50PB0DXVVAFPTC24B9R", "arcl-t-01M5SXD513CRP041YBEYB7CWXF", "arcl-t-01M5SXD52H6Q67PXE4VEY2FFS4"]
+++

Add `mire mcp`, a stdio MCP server, so an agent can open a patch and build a walkthrough without shell glue.

Where the server lives in the crate layout follows the workspace-structure decision.

Acceptance criteria:
- [ ] Tools: `open_patch` (patch text plus an optional title and description, writing a review file), `context`, `walkthrough_apply`, `notes_apply`, and `session_focus`.
- [ ] Each tool keeps the CLI's byte limits, revision checks, and error shapes.
- [ ] Docs show `claude mcp add mire -- mire mcp` and the tool list.

Verification:
- Integration tests drive the server over stdio.
- `just check` passes.
