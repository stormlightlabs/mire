+++
id = "arcl-t-01M5YRVM0JS5TYST55RJRWQTGS"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Record checkpoints in the review file"
status = "pending"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"]
+++

On each refresh, record a checkpoint for each file whose fingerprint changed, and keep the patch copies the design calls for within its size bound.

Verification:
- Model tests cover a change, a revert and the size bound, older review files load, and `just check` passes.
