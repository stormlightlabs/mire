+++
id = "arcl-t-01M5SXD522EWRYNXQ1NZY6P3TM"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Walkthrough model in the review file"
status = "completed"
priority = "high"
position = 6
blocked-by = ["arcl-t-01M5SXD4ZAB2KCM5B3EKPP1EB5", "arcl-t-01M5SXD4ZQV5YW9TF0093A640S"]
evidence = "Done in #9 at a068ad2: walkthroughs in review schema 1.5 with bounds, revision-checked writes, note-rule re-anchoring on refresh, coverage, and docs"
+++

Add walkthroughs to the review model: an optional overview plus ordered steps, each with a title, a Markdown narrative, and anchors to hunks or line ranges.

Use the fields the review-process research proposes. Reuse the note anchors, fingerprints, and re-anchoring in `crates/core/src/review/reanchor.rs`, so a refresh marks steps exact, moved, stale, or ambiguous the same way it marks notes.

Acceptance criteria:
- [ ] `mire-core` models walkthroughs with a version bump, and older review files still load.
- [ ] Writes are atomic and require the expected review revision, as notes do.
- [ ] Source-backed refresh re-anchors walkthrough steps.
- [ ] The review model docs describe walkthroughs.

Verification:
- Core tests cover round-trip, re-anchoring, and revision conflicts.
- `just check` passes.
