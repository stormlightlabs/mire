+++
id = "arcl-t-01M5YRTZ6VQRE61JGTJK0TNGC3"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Narrow web layout"
status = "completed"
priority = "normal"
position = 7
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
evidence = "Done in #28 at 2122821: the browser review works at phone width with wrapped diffs, a bottom tab bar, drawers and safe-area insets, with no sideways scroll."
+++

The diff and notes work at 390 pixels wide with wrapped lines and no horizontal page scroll. See `narrow.png`.

Verification:
- The Playwright review flow covers the new states, and `just check` passes.
