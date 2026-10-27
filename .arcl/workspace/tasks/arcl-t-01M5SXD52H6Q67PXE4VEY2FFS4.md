+++
id = "arcl-t-01M5SXD52H6Q67PXE4VEY2FFS4"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Walkthrough CLI for agents"
status = "completed"
priority = "high"
position = 7
blocked-by = ["arcl-t-01M5SXD522EWRYNXQ1NZY6P3TM"]
evidence = "Done in #10 at ac7c78b: mire walkthrough apply, show and clear; has_walkthrough in context; protocol 1.4; escaped text output; size-bounded review writes; CLI reference"
+++

Give agents a CLI for walkthroughs, shaped like `mire notes apply`.

Acceptance criteria:
- [ ] `mire walkthrough apply <REVIEW> --stdin` takes one JSON batch with the observed review revision and replaces the walkthrough atomically.
- [ ] Steps are located by file and line, like notes. Mire assigns IDs and fingerprints.
- [ ] `mire walkthrough show <REVIEW>` prints it as text or JSON, and `mire walkthrough clear` removes it.
- [ ] `mire context` reports whether a walkthrough exists and its revision.
- [ ] The CLI reference documents the JSON schema.

Verification:
- CLI integration tests cover apply, show, clear, invalid anchors, and revision conflicts.
- `just check` passes.
