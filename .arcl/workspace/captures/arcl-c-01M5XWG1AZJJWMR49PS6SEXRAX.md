+++
id = "arcl-c-01M5XWG1AZJJWMR49PS6SEXRAX"
title = "Markdown note export writes control characters raw"
status = "promoted"
created-at = "2026-10-27T05:14:28.575Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5YRTHYJNT3ABSD29MEFVPV2"
+++

`mire notes export --format markdown` writes note bodies and other agent-supplied text without escaping control characters, so ESC, OSC or CR sequences reach a terminal or file as stored. `walkthrough show` now escapes them with `escape_controls` in `crates/cli/src/terminal_text.rs`; the Markdown export could reuse it. Found while reviewing the walkthrough CLI.
