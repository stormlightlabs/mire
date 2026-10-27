+++
id = "arcl-t-01M5SXD532Q361H4WGVT0TPE37"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Render walkthroughs in the TUI"
status = "in_progress"
priority = "high"
position = 8
blocked-by = ["arcl-t-01M5SXD52H6Q67PXE4VEY2FFS4"]
+++

Render walkthroughs in the TUI: a narrative pane and step navigation that scrolls to each step's anchored hunks.

Reuse the live-session walkthrough plumbing (`mire session walkthrough start|next|previous|stop`) so an agent can drive the open TUI through the steps.

Acceptance criteria:
- [ ] A walkthrough mode shows the overview, the current step's narrative, and step progress.
- [ ] Next and previous keys move between steps and scroll to the anchors. Stale or ambiguous steps say so.
- [ ] The live-session commands step through authored walkthrough steps.
- [ ] Keybinding docs and the help screen list the new keys.

Verification:
- insta snapshots cover each walkthrough state.
- Screenshots from `just shots` show the mode.
- `just check` passes.
