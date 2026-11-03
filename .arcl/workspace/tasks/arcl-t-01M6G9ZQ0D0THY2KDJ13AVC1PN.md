+++
id = "arcl-t-01M6G9ZQ0D0THY2KDJ13AVC1PN"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Terminal display keys in the web review"
status = "in_progress"
priority = "normal"
position = 0
+++

The terminal's focus board lists `{ }` hunk, `p P` note, `c` note on this line, `1 2` unified or split, and `w` wrap. The web review answers to `[ ]`, `j k`, `b` and `Esc`, and its focus-mode key bar lists only those. Add the display keys to the web review and its key bar. Leave out `c` until the web can create findings; `mire serve` has no endpoint for a new finding today.

Acceptance criteria:
- [ ] `}` and `{` scroll the diff to the next or previous hunk of the shown file, and stop at the last and first hunk.
- [ ] `p` and `P` move to the next and previous finding, as `j` and `k` do.
- [ ] `1` and `2` press the Unified and Split buttons. On a narrow screen, which shows one column and no Split button, they do nothing.
- [ ] `w` turns line wrapping on and off on a wide screen, and a Wrap button in the diff controls shows and changes the same state. A narrow screen always wraps.
- [ ] None of these keys fire while typing in a text field or with a dialog open, the same gating as `j k`.
- [ ] The focus-mode key bar lists the new keys, and it stays readable at phone width.
- [ ] Unit tests cover the key mapping, and e2e tests cover hunk jumps, the layout keys and wrap.
- [ ] The docs list the web review's keys.
