+++
id = "arcl-c-01M62ZBZJ3P4CDS0V8BHWRQATP"
title = "Generated-files e2e can miss the finding scroll"
status = "captured"
created-at = "2026-10-29T04:40:53.571Z"
+++

One of three full Playwright runs failed `tests/generated.e2e.ts` (`collapses generated files until the reader opens them`); it passed alone and in 12 repeats. `scrollToFinding` in `packages/app/src/routes/+page.svelte` runs from `setTimeout(0)` after the file loads, so it can run before the diff and its annotations finish rendering, and `toBeInViewport` on the finding then fails. Scroll after the diff's post-render instead.
