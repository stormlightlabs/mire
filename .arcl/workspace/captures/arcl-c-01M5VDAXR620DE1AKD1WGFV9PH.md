+++
id = "arcl-c-01M5VDAXR620DE1AKD1WGFV9PH"
title = "Web review icons render blank"
status = "promoted"
created-at = "2026-10-26T06:11:03.558Z"

[promoted-to]
kind = "task"
id = "arcl-t-01M5YRTHZD07J1JG5D427KNFEZ"
+++

Every icon in the browser review renders blank, including the theme toggle. The page's Content-Security-Policy sets `img-src 'self'`, which likely blocks the `data:` SVG masks UnoCSS generates for icons. Found while checking the title and description dialog; it predates that change.
