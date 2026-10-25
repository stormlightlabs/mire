+++
id = "arcl-t-01M5SXD53HCRRPABDBP9Y1X4SA"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Render walkthroughs in the browser"
status = "pending"
priority = "normal"
position = 9
blocked-by = ["arcl-t-01M5SXD52H6Q67PXE4VEY2FFS4"]
+++

Render walkthroughs in the browser review surface with the same data and step order as the TUI.

Acceptance criteria:
- [ ] The REST API exposes the walkthrough with Utoipa schemas.
- [ ] The app shows the overview and steps, and selecting a step scrolls the diff to its anchors.
- [ ] Keyboard navigation between steps, and stale or ambiguous anchor states.
- [ ] Refresh and watch events update the walkthrough.

Verification:
- Rust API tests and Svelte component tests cover it.
- `just check` passes.
