+++
id = "arcl-t-01M5SXD4ZAB2KCM5B3EKPP1EB5"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Visual verification and snapshot coverage"
status = "in_progress"
priority = "high"
position = 0
+++

Capture the TUI as images locally and widen snapshot test coverage, so UI changes carry visual evidence that reviewers can check.

Use fabrial's `.github/storm/shot.sh` (tmux + freeze). It shipped after fabrial 0.3.3, so run `fab update` once a release includes it, or copy it in until then.

Acceptance criteria:
- [x] `scripts/shots.sh` lists scenarios (name, size, mire command, keys) against a deterministic fixture repository, covering unified, split, narrow, help, collapsed sidebar, note overlay, live walkthrough, and `NO_COLOR`.
- [x] `just shot <scenario>` and `just shots` write PNG and ANSI files.
- [x] As in fabrial, the implementer and reviewers run the captures locally and report the PNG paths. There is no CI job for them.
- [x] insta `TestBackend` snapshots cover the sidebar and tree, split and unified layouts, intraline highlights, notes and findings, footer actions, walkthrough status, collapsed files and hunks, and empty and error states.
- [x] insta JSON snapshots cover `mire context`, `mire review status`, and `mire session inspect`. Their output holds no absolute paths or random values, so nothing needs redacting.
- [x] The local development guide covers `just shots` and `cargo insta review`.

Verification:
- `just shots` produces PNGs locally, and `cargo insta test --workspace` is clean.
- A deliberate UI change makes a snapshot fail.
