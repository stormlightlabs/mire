+++
id = "arcl-t-01M7NZ4BYG60YASXAY10QYRT42"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Stop the generated-files e2e racing the diff"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #31 at 535899f: finding scroll waits for the diff to render, J/K and file picks drop stale finding loads; generated e2e 0 of 120 failed"
+++

One of three full Playwright runs failed `tests/generated.e2e.ts` (`collapses generated files until the reader opens them`), and main's push-run Check failed the same way. Two races cause it:

- `scrollToFinding` in `packages/app/src/routes/+page.svelte` runs from `setTimeout(0)` after the file loads, which can be before the diff and its finding annotations render, so the finding never scrolls into view.
- `j` and `k` step from the finding that has finished loading. A `j` pressed while the finding `k` picked is still loading steps from the old finding, so the heading stays on `one.txt`.

Acceptance criteria:
- [ ] Opening a finding scrolls to its annotation once the diff has rendered it, however long the diff takes to render.
- [ ] `j` and `k` step from the finding last asked for, so keys pressed in quick succession each move one finding, and a slow finding load cannot replace a newer one.
- [ ] The generated-files e2e waits on what each key did before pressing the next.
- [ ] `tests/generated.e2e.ts` passes 30 repeats in a row.
