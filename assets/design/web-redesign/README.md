# Web review redesign

These boards propose a new layout for the browser review that `mire serve`
opens. They are a design, not the shipped app. The diff text comes from the
change that added review titles and descriptions.

The goal is a place to read changing code: what changed, what changed since
you last looked, what you have read, and what you noted. The colors are the
six terminal themes from `crates/ui/src/theme.rs`, applied with the same roles.

Some boards need data the review file does not hold yet: comparison
checkpoints, revision cells, and notes with no line. Walkthrough mode needs the
walkthrough model described in the docs under
[Walkthrough design](../../../packages/docs/src/content/docs/concepts/walkthrough-design.md).
`sources.png` marks which ideas work with today's data.

| Image | Shows |
| --- | --- |
| `reading-view.png` | Files grouped by intent, the diff with an inline finding, and the notes panel, in Nord Light |
| `reading-view-iceberg-dark.png` | The same view in Iceberg Dark |
| `walkthrough.png` | An agent-written step beside its anchored hunks from two files |
| `since-last-review.png` | Files changed since they were marked viewed, revision cells, and a checkpoint strip |
| `focus-mode.png` | One file at full width with the key bar |
| `notes-and-anchoring.png` | A stale note beside the closest current code, and the five anchor states |
| `live-update.png` | A new revision held until applied, a kept draft after a conflict, and a stopped watch |
| `finish-review.png` | What is left before handoff, and the exports |
| `narrow.png` | The diff at 390 pixels wide with wrapped lines |
| `principles.png` | The rules, type, theme roles, and status marks |
| `themes.png` | The six terminal themes on the same diff |
| `sources.png` | Which review tools each idea comes from, and what was left out |
