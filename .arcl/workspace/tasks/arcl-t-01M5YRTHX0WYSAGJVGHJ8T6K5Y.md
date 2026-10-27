+++
id = "arcl-t-01M5YRTHX0WYSAGJVGHJ8T6K5Y"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Wrap only the visible walkthrough pane lines when drawing"
status = "pending"
priority = "normal"
position = 0
+++

Layout now counts pane lines only up to the pane height, but drawing still wraps every step title to count the '… N more lines' marker. On an overview with 5,000 steps a frame takes about 35 ms in release. Wrap only the rows the pane shows, or cache the wrapped lines per walkthrough, position and width. This fits with scrolling the pane (arcl-c-01M5XZ0JB3RDTPAQETZB7R7QGW).
