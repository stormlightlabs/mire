---
title: Keybinds
description: Kb & mouse controls for navigating the TUI.
section: Reference
group: Reference
order: 9
---

Press `?` in the TUI to show or hide the built-in keybinding panel. Keys listed
for the note editor, filters, and search apply while that mode is open and take
precedence over the general controls.

## Navigate the review

| Key             | Action                                                            |
| --------------- | ----------------------------------------------------------------- |
| `q`             | Quit Mire                                                         |
| `Esc`           | Cancel active UI state or return sidebar focus to the review      |
| `?`             | Show or hide keybinding help                                      |
| `Tab`           | Switch focus between the file sidebar and review                  |
| `b`             | Show or hide the file sidebar                                     |
| `j` / `Down`    | Scroll down, or select the next file when the sidebar has focus   |
| `k` / `Up`      | Scroll up, or select the previous file when the sidebar has focus |
| `PgDn` / `PgUp` | Move down or up by one page                                       |
| `g` / `Home`    | Jump to the first row                                             |
| `G` / `End`     | Jump to the last row                                              |
| `]` / `[`       | Jump to the next or previous file                                 |
| `}` / `{`       | Jump to the next or previous hunk                                 |
| `Enter` / `Space` | Collapse or expand the file or hunk under the review cursor      |
| `Ctrl-P`        | Open the changed-file picker                                      |

## Search and display

| Key       | Action                                    |
| --------- | ----------------------------------------- |
| `/`       | Start a search                            |
| `n` / `N` | Jump to the next or previous search match |
| `+` / `-` | Show more or less unchanged context       |
| `w`       | Toggle line wrapping                      |
| `1`       | Use the unified layout                    |
| `2`       | Use the split layout                      |
| `3`       | Choose the layout automatically           |

While entering a search, type to update the query. `Backspace` removes the
previous character, `Enter` runs the search, and `Esc` cancels input.

## Jump to a changed file

Press `Ctrl-P` to filter changed paths as you type. Results show the file
status, changed-line counts, and finding progress. Use `j`/`k`, the arrow keys,
or the mouse to select a result, then press `Enter` or click it to jump. `Esc`
closes the picker.

## Work with review notes

These controls are available when you open a durable review with
`mire review REVIEW.json`.

| Key       | Action                                                     |
| --------- | ---------------------------------------------------------- |
| `v`       | Start or clear a source range                              |
| `c`       | Create a note on the current source line or selected range |
| `e`       | Edit the selected note                                     |
| `p` / `P` | Jump to the next or previous visible note                  |
| `r`       | Resolve the selected note                                  |
| `d`       | Dismiss the selected note                                  |
| `o`       | Reopen the selected note                                   |
| `a`       | Accept the risk recorded by the selected note              |
| `f`       | Open note filters                                          |
| `Ctrl-S`  | Save pending review changes or retry a failed save         |

### Note editor

| Key                     | Action                          |
| ----------------------- | ------------------------------- |
| `Enter`                 | Save the note                   |
| `Esc`                   | Cancel editing                  |
| `Backspace`             | Delete the previous character   |
| `Tab`                   | Select the next severity        |
| `Shift-Tab`             | Select the next annotation kind |
| Any printable character | Add text to the note body       |

### Note filters

| Key             | Action                    |
| --------------- | ------------------------- |
| `a`             | Cycle author type         |
| `s`             | Cycle status              |
| `v`             | Cycle severity            |
| `k`             | Cycle annotation kind     |
| `i`             | Cycle file                |
| `c`             | Clear all filters         |
| `Enter` / `Esc` | Close the filter controls |

## Using the mouse

Scroll the wheel over the review or sidebar to move through it. Left-click a
file or review row to select it. Click a file or hunk disclosure glyph to
collapse or expand its source rows. In review files, right-click a source row to
create or edit a note, and use the buttons on a note row to edit it or change
its status.
