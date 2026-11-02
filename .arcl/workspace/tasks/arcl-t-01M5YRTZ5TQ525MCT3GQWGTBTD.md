+++
id = "arcl-t-01M5YRTZ5TQ525MCT3GQWGTBTD"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Finish-review panel"
status = "completed"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
evidence = "Done in #27 at 647136f: the browser finish-review panel lists what is left before handoff and offers four exports; it writes nothing and implies no approval."
+++

Show what is left before handoff (open findings, unsafe anchors, unviewed files) and the export choices. It writes no status and implies no approval. See `finish-review.png`.

Verification:
- The Playwright review flow covers the new states, and `just check` passes.
