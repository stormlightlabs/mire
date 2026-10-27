+++
id = "arcl-s-01M5YRT4GEBZ4XSD2A752FAW3Z"
title = "Reading-first web review"
status = "open"
acceptance-criteria = """
- The web app uses the six terminal themes from `crates/ui/src/theme.rs`, with Nord Light as the light theme.
- The file list groups files by walkthrough step when the review has a walkthrough, and by directory otherwise. Generated files are marked and collapsed.
- Focus mode, live-update handling, the finish-review panel and the narrow layout match their boards.
- Playwright covers each new state."""
+++

Rebuild the browser review that `mire serve` opens around reading: what changed, what you have read, and what you noted. The boards in `assets/design/web-redesign/` show the layout. This spec covers the parts that work on the data the review file holds today. Changes between revisions are a separate spec.
