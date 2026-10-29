+++
id = "arcl-t-01M61C8P3VS9PPGS98GTRH8NMR"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Separate truecolor and 256-colour colours for Iceberg Dark"
status = "in_progress"
priority = "normal"
position = 0
+++

Iceberg Dark now draws a navy header and grey-olive added rows, because one set of colours has to work in truecolor and in tmux's 256-colour mapping. The owner wants the original Iceberg colours in truecolor terminals and the adjusted ones only where colours are reduced to 256.

Acceptance criteria:
- [ ] Truecolor terminals get Iceberg Dark's original header and add-row colours.
- [ ] 256-colour terminals keep colours that pass the contrast test after mapping (`crates/ui/src/theme/xterm256.rs`).
- [ ] The contrast test covers both sets.
- [ ] Shots show both, checked by eye.
