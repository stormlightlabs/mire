+++
id = "arcl-t-01M5YRTJ1Q6R5KQ1SSQANF3HM2"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Use the TUI themes in the web app"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5SXDG7QJM6Y67FCJ8ZPNASW"]
evidence = "Done in #20 at b9d65dc: web review uses the terminal palettes generated from Rust with a drift test; one auto mapping; light terminal themes reworked; contrast and 256-colour tests; CSP fix for header icons."
+++

The browser review uses its own paper palette in `packages/app/src/app.css`, while the terminal has six themes in `crates/ui/src/theme.rs`. Give the web app the same palettes, mapped to the same roles `Theme::from_palette` uses. `mire serve` already accepts the global `--theme` flag but ignores it; pass it to the browser so both surfaces match. The terminal's `auto` default picks Eldritch, while the boards in `assets/design/web-redesign` use Nord Light and Iceberg Dark. Nord Light is the light theme, as the spec says. Choose the dark default for both surfaces: Eldritch or Iceberg Dark. Some pairs read below 4.5:1 on their own background and should change in both places: muted text and accent on Nord Light and Eldritch Dusk, and muted text on Catppuccin Latte.

Verification:
- A test checks that text, muted text and accent reach 4.5:1 on their background in every theme, in the terminal and the web app.
- The Playwright review flow opens `mire serve --theme` with a light and a dark theme and checks the page uses them.
- `just check` passes.
