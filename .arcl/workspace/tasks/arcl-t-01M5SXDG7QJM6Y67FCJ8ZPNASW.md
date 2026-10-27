+++
id = "arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "End-to-end browser test of `mire serve`"
status = "completed"
priority = "normal"
position = 3
evidence = "Done in #18 at 793e2c4: Playwright test of the built mire serve (load, navigate, decide, viewed, export, refresh), just e2e, advisory e2e CI job; fixed four browser reload bugs it and the reviews found."
+++

Add a Playwright flow against the packaged `mire serve` binary: open a review, move through findings, record a decision, refresh, and export.

Verification:
- A CI job builds the binary and runs the flow.
