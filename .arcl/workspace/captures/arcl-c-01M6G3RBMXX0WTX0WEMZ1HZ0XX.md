+++
id = "arcl-c-01M6G3RBMXX0WTX0WEMZ1HZ0XX"
title = "Light palettes draw comments in the text color"
status = "captured"
created-at = "2026-11-03T07:07:41.085Z"
+++

In Nord Light, Eldritch Dusk and Catppuccin Latte, `Theme::from_palette` in `crates/ui/src/theme.rs` sets the comment role to `palette.text`, so comments look the same as code in the terminal and, now that the browser takes the same role, in the browser diff too. The dark palettes use `overlay1`, a dimmer color. Pick a light comment color that reads as secondary and still reaches 4.5:1 on the plain, added and removed rows.
