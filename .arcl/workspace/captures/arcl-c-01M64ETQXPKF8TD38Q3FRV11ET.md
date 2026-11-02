+++
id = "arcl-c-01M64ETQXPKF8TD38Q3FRV11ET"
title = "Text contrast after 256-colour rounding in every palette"
status = "captured"
created-at = "2026-10-29T18:30:20.342Z"
+++

The contrast test in `crates/ui/src/view/tests/contrast.rs` checks each drawn cell in truecolor only. Rounding each pair with `xterm256_rounded` first gives 17 pairs below target across the palettes, for example:

- Iceberg Dark and Eldritch Minimal: the border `#5f5f87` on `#1c1c1c` is 2.82:1, below 3:1.
- Iceberg Dark: muted text and red on the selection (`#3a3a3a`) are 4.25:1 and 4.17:1.
- Eldritch Dusk: `#5f5faf` on several light surfaces is 3.90 to 4.41:1.
- Catppuccin Latte: text on the add and remove emphasis is 4.46:1 and 3.73:1.

Fix the colours (per-depth palettes in `crates/ui/src/theme.rs` allow it), then round pairs in that test at `ColorDepth::Xterm256` and drop the Catppuccin Latte exception in `diff_text_reaches_aa_contrast_on_headings_and_changed_rows`.
