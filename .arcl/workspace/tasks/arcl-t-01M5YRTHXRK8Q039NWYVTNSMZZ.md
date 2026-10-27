+++
id = "arcl-t-01M5YRTHXRK8Q039NWYVTNSMZZ"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Markdown note export leaves out note bodies"
status = "pending"
priority = "normal"
position = 0
+++

`mire notes export --format markdown` and the browser's Markdown download list each note's severity, kind, status, author and locations, but never its body, so the export loses the finding itself. `notes_markdown` in `crates/cli/src/protocol.rs` builds each section without `note.body()`. Found while adding server tests.

Verification:
- A CLI test exports a note with a body to Markdown and finds the body in its section, and the browser download uses the same output.
- `just check` passes.
