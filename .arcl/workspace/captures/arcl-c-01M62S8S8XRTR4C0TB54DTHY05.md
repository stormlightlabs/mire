+++
id = "arcl-c-01M62S8S8XRTR4C0TB54DTHY05"
title = "Comment colour contrast in the web diff"
status = "captured"
created-at = "2026-10-29T02:54:17.373Z"
+++

The web diff colours syntax with the Shiki `github-light` and `github-dark` themes (set in `packages/app/src/lib/DiffViewer.svelte`), not with `themes.css`. Their comment colour is `#6A737D` in both. Measured in Chromium:

| Palette | On an added row | On the plain background |
| --- | --- | --- |
| Nord Light | 3.59 | 3.95 |
| Iceberg Dark | 2.84 | 3.92 |
| Eldritch Dusk | 3.48 | 3.83 |
| Eldritch Minimal | 2.55 | 3.62 |
| Catppuccin Latte | 3.58 | 3.96 |
| Catppuccin Mocha | 2.58 | 3.65 |

All are below 4.5:1. Give the web diff syntax themes generated from the terminal palettes in `crates/ui/src/theme.rs`, or pick Shiki themes whose comments clear 4.5:1 on each palette's surface and addition background.
