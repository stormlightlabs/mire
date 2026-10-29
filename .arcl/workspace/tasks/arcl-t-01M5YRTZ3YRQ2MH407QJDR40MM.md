+++
id = "arcl-t-01M5YRTZ3YRQ2MH407QJDR40MM"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Focus mode in the web review"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
evidence = "Done in #23 at 01c8b2f: focus mode in the browser review with b/Esc, focus bar, key bar, [ ] file navigation; added and deleted files render as plain code with a gutter bar."
+++

One file at full width with the key bar, entered and left from the keyboard. See `focus-mode.png`.

Verification:
- The Playwright review flow covers the new states, and `just check` passes.
