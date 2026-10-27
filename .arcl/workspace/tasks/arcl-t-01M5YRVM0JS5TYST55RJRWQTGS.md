+++
id = "arcl-t-01M5YRVM0JS5TYST55RJRWQTGS"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Record checkpoints in the review file"
status = "completed"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"]
evidence = "Done in #16 at 0fb2cd9: checkpoints, blob ids, bounded patch copies and compaction in the review file; mire file history and history API routes; viewed marks at older revisions; model, Git, CLI and API tests."
+++

On each refresh, record a checkpoint for each file whose fingerprint changed, and keep the patch copies the design calls for within its size bound.

Verification:
- Model tests cover a change, a revert and the size bound, older review files load, and `just check` passes.
