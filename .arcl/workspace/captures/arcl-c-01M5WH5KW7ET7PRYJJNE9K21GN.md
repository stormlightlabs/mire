+++
id = "arcl-c-01M5WH5KW7ET7PRYJJNE9K21GN"
title = "Use the TUI themes in the web app"
status = "promoted"
created-at = "2026-10-26T16:37:18.343Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5YRTJ1Q6R5KQ1SSQANF3HM2"
+++

The browser review uses its own paper palette in `packages/app/src/app.css`, while the terminal has six themes in `crates/ui/src/theme.rs`. Give the web app the same palettes, mapped to the same roles `Theme::from_palette` uses. `mire serve` already accepts the global `--theme` flag but ignores it; pass it to the browser so both surfaces match. The terminal's `auto` default picks Eldritch, while the boards in `assets/design/web-redesign` use Nord Light and Iceberg Dark, so choose one default for both. Some pairs read below 4.5:1 on their own background and should change in both places: muted text and accent on Nord Light and Eldritch Dusk, and muted text on Catppuccin Latte.
