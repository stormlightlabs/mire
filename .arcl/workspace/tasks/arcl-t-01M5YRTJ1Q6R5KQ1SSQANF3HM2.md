+++
id = "arcl-t-01M5YRTJ1Q6R5KQ1SSQANF3HM2"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Use the TUI themes in the web app"
status = "pending"
priority = "normal"
position = 1
+++

The browser review uses its own paper palette in `packages/app/src/app.css`, while the terminal has six themes in `crates/ui/src/theme.rs`. Give the web app the same palettes, mapped to the same roles `Theme::from_palette` uses. `mire serve` already accepts the global `--theme` flag but ignores it; pass it to the browser so both surfaces match. The terminal's `auto` default picks Eldritch, while the boards in `assets/design/web-redesign` use Nord Light and Iceberg Dark, so choose one default for both. Some pairs read below 4.5:1 on their own background and should change in both places: muted text and accent on Nord Light and Eldritch Dusk, and muted text on Catppuccin Latte.
