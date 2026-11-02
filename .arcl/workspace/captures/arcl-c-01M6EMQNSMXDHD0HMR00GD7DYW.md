+++
id = "arcl-c-01M6EMQNSMXDHD0HMR00GD7DYW"
title = "Watcher misses a review file replaced within the debounce of a server write"
status = "captured"
created-at = "2026-11-02T17:25:55.636Z"
+++

mire serve's watcher keeps its own last-seen revision, and the server's own finding writes publish without updating it. If the review file is replaced with a copy at the watcher's last-seen revision within the 150 ms debounce of a server write, the watcher sees no change and open tabs are not told. Share the announced revision between the write routes and the watcher.
