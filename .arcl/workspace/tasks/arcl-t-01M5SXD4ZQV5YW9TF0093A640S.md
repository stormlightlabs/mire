+++
id = "arcl-t-01M5SXD4ZQV5YW9TF0093A640S"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Research review tools for walkthrough design"
status = "pending"
priority = "high"
position = 1
+++

Study how Greptile, hunk, and CodeRabbit present a change for review, and decide what Mire's walkthroughs should borrow before the walkthrough model is designed.

Look at walkthrough structure, change ordering and grouping, when summaries and diagrams help, how findings are presented, and how a reviewer moves through a pull request. Repository mapping and codebase indexing are out of scope; they belong to dalil, and Mire uses only the context an agent supplies.

Acceptance criteria:
- [ ] A concepts page in `packages/docs` records what to adopt, adapt, and skip for each tool, with sources.
- [ ] It proposes the walkthrough fields (overview, steps, narrative, anchors, ordering) the model task will implement.
- [ ] The ROADMAP "Useful CodeRabbit ideas" table is updated or replaced to match.

Verification:
- `just check` passes, including the docs build checks.
