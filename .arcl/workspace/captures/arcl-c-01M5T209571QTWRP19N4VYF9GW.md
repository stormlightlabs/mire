+++
id = "arcl-c-01M5T209571QTWRP19N4VYF9GW"
title = "Render walkthrough Markdown in the TUI and browser"
status = "captured"
created-at = "2026-10-25T17:33:46.023Z"
+++

Walkthrough overview and step narrative are Markdown (concepts/walkthrough-design.md). Neither surface renders Markdown today: the browser prints note bodies as plain text in packages/app/src/lib/DiffPane.svelte and FindingQueue.svelte, and the TUI only highlights .md files inside diffs. Pick a renderer for each surface, with safe HTML handling in the browser, before walkthrough narrative ships.
