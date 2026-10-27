+++
id = "arcl-t-01M5YRVKZRM2G4QRZQGHRDKV4N"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Store viewed state in the review file"
status = "completed"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"]
evidence = "Done in #15 at 2ccd976: viewed marks in the review file (schema 1.6), CLI file viewed/unviewed, serve routes and events, TUI m key, browser on the API; model, CLI and API tests."
+++

Record which file fingerprint each file was marked viewed at, from the CLI, the terminal and the browser, as the design says. Replace the browser-local viewed state.

Verification:
- Model, CLI and API tests cover marking, unmarking and a fingerprint change, older review files load, and `just check` passes.
