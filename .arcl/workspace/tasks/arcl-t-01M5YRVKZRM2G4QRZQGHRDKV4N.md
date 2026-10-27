+++
id = "arcl-t-01M5YRVKZRM2G4QRZQGHRDKV4N"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Store viewed state in the review file"
status = "in_progress"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"]
+++

Record which file fingerprint each file was marked viewed at, from the CLI, the terminal and the browser, as the design says. Replace the browser-local viewed state.

Verification:
- Model, CLI and API tests cover marking, unmarking and a fingerprint change, older review files load, and `just check` passes.
