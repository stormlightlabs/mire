+++
id = "arcl-c-01M606Q2M0J0SCZ34SHMSDTWG0"
title = "Theme the browser diff viewer from the terminal palettes"
status = "captured"
created-at = "2026-10-28T02:51:33.888Z"
+++

The browser review's chrome now uses the terminal palettes (packages/app/src/themes.css), but the diff viewer in DiffViewer.svelte still renders with the github-light and github-dark Shiki themes, so its backgrounds and syntax colors differ from the terminal. @pierre/diffs reads --diffs-* custom properties; map the addition, deletion and syntax roles onto them.
