+++
id = "arcl-t-01M5YRTZ4ZMWK77KHH7VYZ577B"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Hold live updates until applied"
status = "pending"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
+++

When a new revision arrives, show it and apply it on request instead of swapping the diff under the reader. Keep a draft note through a conflict, and show when the watch has stopped. See `live-update.png`.

Verification:
- The Playwright review flow covers the new states, and `just check` passes.
