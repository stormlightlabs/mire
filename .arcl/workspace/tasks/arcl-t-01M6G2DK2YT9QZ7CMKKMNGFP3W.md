+++
id = "arcl-t-01M6G2DK2YT9QZ7CMKKMNGFP3W"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Syntax colours in the browser diff from the terminal palettes"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #29 at e97a3a1: browser diff takes syntax, row and changed-word colours from the terminal palettes, every syntax colour at least 4.5:1 on every row in all six palettes"
+++

The browser diff colours syntax with Shiki's `github-light` and `github-dark` themes, set in `packages/app/src/lib/DiffViewer.svelte`, not with `themes.css`. Their comment colour, `#6A737D`, reaches only 2.5 to 4.0:1 in the six palettes. Colour the browser diff's syntax from the terminal palettes instead.

Acceptance criteria:
- [ ] The browser diff's syntax colours come from the same palette roles as the terminal's syntax highlighting (`Theme::syntax` in `crates/ui/src/theme.rs`), generated into `themes.css` by `crates/ui/src/theme/web.rs` (`just web-themes`), not from Shiki's github themes.
- [ ] Every syntax colour, comments included, reaches 4.5:1 against the plain, added and removed row backgrounds in all six palettes, and the web contrast test checks it.
- [ ] Highlighting still works for the languages it does today, with no runtime network fetch.
- [ ] An e2e check confirms a comment span's computed colour matches the palette variable in one light and one dark theme.
