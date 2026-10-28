+++
id = "arcl-c-01M606Q2N0VBYGE8X7HVYAH69Y"
title = "Browser review icons do not render under the page's CSP"
status = "captured"
created-at = "2026-10-28T02:51:33.920Z"
+++

In mire serve, every UnoCSS lucide icon (theme toggle, Refresh source, Finish review) shows as empty space. The icons are CSS mask-image data: URIs, and the Content-Security-Policy sets img-src 'self', which likely blocks them. Seen in Chromium screenshots while working on themes.
