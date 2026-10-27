+++
id = "arcl-c-01M5XZ0JAJ6SMCXE6VW9CQ712N"
title = "Order files by walkthrough steps in the TUI"
status = "promoted"
created-at = "2026-10-27T05:58:27.410Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5YRTHVHMSPDZ53JDW8BAPXT"
+++

The walkthrough design says walkthrough mode lists covered files first, in the order of the first step that touches each, then the hunks and files no step covers. The TUI walkthrough mode keeps changeset order and does not list uncovered hunks. `Review::walkthrough_coverage()` already computes them.
