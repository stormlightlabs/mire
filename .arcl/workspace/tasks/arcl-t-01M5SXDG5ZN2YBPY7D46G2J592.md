+++
id = "arcl-t-01M5SXDG5ZN2YBPY7D46G2J592"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Rust API and server tests"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #8 at 8dce621: focused unit tests for auth, host, origin, body limits, every REST resource, conflicts, locks and events, plus end-to-end tests on a real socket; fixed camelCase problem bodies and missed watcher events"
+++

Add focused tests for `mire serve`: authentication, host and origin checks, request limits, every REST resource, revision conflicts, and the event stream.

Verification:
- `cargo test -p mire` covers them, and `just check` passes.
