+++
id = "arcl-t-01M5YRTHYJNT3ABSD29MEFVPV2"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Markdown note export writes control characters raw"
status = "pending"
priority = "normal"
position = 0
+++

`mire notes export --format markdown` writes note bodies and other agent-supplied text without escaping control characters, so ESC, OSC or CR sequences reach a terminal or file as stored. `walkthrough show` now escapes them with `escape_controls` in `crates/cli/src/terminal_text.rs`; the Markdown export could reuse it. Found while reviewing the walkthrough CLI.
