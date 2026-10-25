+++
id = "arcl-t-01M5SXDG5ZN2YBPY7D46G2J592"
plan = "arcl-pl-01M5SXDG5F6QV83EW8GZ6VFSSP"
title = "Rust API and server tests"
status = "pending"
priority = "normal"
position = 0
+++

Add focused tests for `mire serve`: authentication, host and origin checks, request limits, every REST resource, revision conflicts, and the event stream.

Verification:
- `cargo test -p mire` covers them, and `just check` passes.
