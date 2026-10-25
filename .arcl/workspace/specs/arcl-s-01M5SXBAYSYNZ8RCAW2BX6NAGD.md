+++
id = "arcl-s-01M5SXBAYSYNZ8RCAW2BX6NAGD"
title = "Mire as the PR viewer and walkthrough surface"
status = "open"
acceptance-criteria = """
- `gh pr diff <n> | mire patch -` and its `--patch` form open in the TUI and the browser.
- An agent can write a walkthrough through `mire walkthrough apply` or MCP, and a person can step through it in the TUI and the browser.
- Walkthroughs survive source refresh with the same re-anchoring rules as notes."""
+++

Mire is the diff and review surface for fabrial and arclightning.

- View any diff, including GitHub pull requests piped in as patches from `gh`. Mire never calls GitHub or `gh` itself; the caller supplies pull-request context.
- An agent builds a walkthrough of a change through the CLI or an MCP server: ordered steps with a narrative, each anchored to hunks. Mire stores, re-anchors, and renders it.
- Mire does not call a model, and repository mapping belongs to dalil.

ROADMAP.md holds the scope.
