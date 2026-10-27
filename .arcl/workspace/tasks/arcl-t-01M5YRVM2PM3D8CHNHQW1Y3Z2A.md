+++
id = "arcl-t-01M5YRVM2PM3D8CHNHQW1Y3Z2A"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Show the code a stale note was written on"
status = "pending"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5YRVM0JS5TYST55RJRWQTGS"]
+++

For a note whose anchor went stale, show the code it was written on beside the closest current code. See `notes-and-anchoring.png`.

Verification:
- Playwright and terminal snapshot tests cover it, and `just check` passes.
