+++
id = "arcl-t-01M5YRTZ2YGAYZBXH6Y3YDXJ8J"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Group the web file list by walkthrough step"
status = "pending"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5SXD53HCRRPABDBP9Y1X4SA", "arcl-t-01M5YRTZ225XVFMBHW73X7CX6X"]
+++

When the review has a walkthrough, group the browser file list under its steps in order, with files no step names last. Otherwise group by directory. Generated files sit in their own collapsed group. See `reading-view.png`.

Verification:
- Playwright covers the new states, and `just check` passes.
