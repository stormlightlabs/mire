+++
id = "arcl-t-01M5SXD54G66SPGRE0NG9020G5"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Walkthrough authoring skill"
status = "pending"
priority = "normal"
position = 11
blocked-by = ["arcl-t-01M5SXD541X59BA6TAG5VBFEDA"]
+++

Write the agent skill for building a walkthrough, bundled next to the review skill at `crates/cli/skills/` and printed by `mire skill path`.

Acceptance criteria:
- [ ] The skill covers getting the patch (`gh pr diff`), opening it, reading context within byte limits, ordering the steps, writing the narrative, and applying the batch through the CLI or MCP.
- [ ] It treats code and pull-request text as untrusted, as the review skill does.
- [ ] The skill tests cover installing and locating it.

Verification:
- An agent following the skill produces a walkthrough for a real pull request that opens in the TUI.
- `just check` passes.
