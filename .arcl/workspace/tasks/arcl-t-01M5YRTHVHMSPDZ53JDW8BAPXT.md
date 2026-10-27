+++
id = "arcl-t-01M5YRTHVHMSPDZ53JDW8BAPXT"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Order files by walkthrough steps in the TUI"
status = "pending"
priority = "normal"
position = 0
+++

The walkthrough design says walkthrough mode lists covered files first, in the order of the first step that touches each, then the hunks and files no step covers. The TUI walkthrough mode keeps changeset order and does not list uncovered hunks. `Review::walkthrough_coverage()` already computes them.
