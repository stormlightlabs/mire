+++
id = "arcl-c-01M6G3RBM3X7RE9YYNC8FB60VG"
title = "Changed-word highlights lower syntax contrast in the web diff"
status = "captured"
created-at = "2026-11-03T07:07:41.059Z"
+++

The browser diff marks changed words with a `[data-diff-span]` background: the change color (`--success` or `--danger`) at 15% opacity in light palettes and 20% in dark ones, over the row. The syntax colors reach 4.5:1 on the plain, added and removed rows, but on that highlight the lowest syntax color falls to:

| Palette | Added word | Removed word |
| --- | --- | --- |
| Iceberg Dark | 2.96 | 3.54 |
| Nord Light | 4.14 | 3.66 |
| Eldritch Minimal | 2.75 | 3.49 |
| Eldritch Dusk | 4.64 | 4.11 |
| Catppuccin Mocha | 2.71 | 3.28 |
| Catppuccin Latte | 4.13 | 3.55 |

A dark row leaves almost no room for a brighter highlight, since Iceberg Dark comments sit at 4.53:1 on an added row. Mark changed words without lightening the background, for example with an underline or an outline, or set `--diffs-bg-addition-emphasis-override` from a color `crates/ui/src/theme/web.rs` checks.
