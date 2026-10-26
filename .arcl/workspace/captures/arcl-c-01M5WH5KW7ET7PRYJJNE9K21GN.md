+++
id = "arcl-c-01M5WH5KW7ET7PRYJJNE9K21GN"
title = "Use the TUI themes in the web app"
status = "captured"
created-at = "2026-10-26T16:37:18.343Z"
+++

The browser review uses its own paper palette in `packages/app/src/app.css`, while the terminal has six themes in `crates/ui/src/theme.rs`. Give the web app the same palettes, mapped to the same roles `Theme::from_palette` uses, with Nord Light as the light default and Iceberg Dark as the dark one. `mire serve` could take `--theme` as the TUI does. The boards in `assets/design/web-redesign` show the mapping. Some pairs read below 4.5:1 on their own background and should change in both places: muted text and accent on Nord Light and Eldritch Dusk, and muted text on Catppuccin Latte.
