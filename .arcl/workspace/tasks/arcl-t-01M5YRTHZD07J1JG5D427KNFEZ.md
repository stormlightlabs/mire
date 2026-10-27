+++
id = "arcl-t-01M5YRTHZD07J1JG5D427KNFEZ"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Web review icons render blank"
status = "pending"
priority = "normal"
position = 0
+++

Every icon in the browser review renders blank, including the theme toggle. The page's Content-Security-Policy sets `img-src 'self'`, which likely blocks the `data:` SVG masks UnoCSS generates for icons. Found while checking the title and description dialog; it predates that change.

Verification:
- Every icon, including the theme toggle, shows in the browser review served by `mire serve`.
- A server test pins the Content-Security-Policy that allows them, and `just check` passes.
