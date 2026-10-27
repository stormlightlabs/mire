+++
id = "arcl-c-01M5WV335306N4HTQZ1ZJ462K4"
title = "Markdown note export leaves out note bodies"
status = "promoted"
created-at = "2026-10-26T19:30:41.443Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5YRTHXRK8Q039NWYVTNSMZZ"
+++

`mire notes export --format markdown` and the browser's Markdown download list each note's severity, kind, status, author and locations, but never its body, so the export loses the finding itself. `notes_markdown` in `crates/cli/src/protocol.rs` builds each section without `note.body()`. Found while adding server tests.
