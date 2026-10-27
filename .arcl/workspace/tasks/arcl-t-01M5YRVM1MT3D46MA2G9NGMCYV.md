+++
id = "arcl-t-01M5YRVM1MT3D46MA2G9NGMCYV"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Show changes since last viewed"
status = "pending"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5YRTZ2YGAYZBXH6Y3YDXJ8J", "arcl-t-01M5YRVKZRM2G4QRZQGHRDKV4N", "arcl-t-01M5YRVM0JS5TYST55RJRWQTGS"]
+++

Show which files changed since they were marked viewed, the revision cells for each file and the checkpoint strip, in the browser and the terminal. See `since-last-review.png`.

Verification:
- Playwright and terminal snapshot tests cover it, and `just check` passes.
