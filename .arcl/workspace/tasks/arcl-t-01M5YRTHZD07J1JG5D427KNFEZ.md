+++
id = "arcl-t-01M5YRTHZD07J1JG5D427KNFEZ"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Web review icons render blank"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #20. Chromium logged `img-src 'self'` violations for the UnoCSS icons' `data:` SVG masks. The page policy now sets `img-src 'self' data:`; `the_page_policy_allows_only_data_images_beyond_its_own_origin` pins it, and `tests/icons.e2e.ts` checks that each header icon draws pixels and that no CSP violation is logged. It failed before the change and passes after."
+++

Every icon in the browser review renders blank, including the theme toggle. The page's Content-Security-Policy sets `img-src 'self'`, which likely blocks the `data:` SVG masks UnoCSS generates for icons. Found while checking the title and description dialog; it predates that change.

Verification:
- Every icon, including the theme toggle, shows in the browser review served by `mire serve`.
- A server test pins the Content-Security-Policy that allows them, and `just check` passes.
