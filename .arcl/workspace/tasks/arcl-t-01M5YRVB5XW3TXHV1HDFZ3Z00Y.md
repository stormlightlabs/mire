+++
id = "arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Design checkpoints and viewed state"
status = "pending"
priority = "normal"
position = 1
+++

Write a design page under `packages/docs/src/content/docs/concepts/` that fixes the review-file schema for viewed state and checkpoints, using the decisions in the spec. Cover how fingerprints are stored, when Mire rebuilds old text from Git and when it keeps a patch copy, the size bound on those copies, how older review files load, and what the API returns. Update the Viewed and Reviewed items and the client-state paragraph in ROADMAP.md to match.

Verification:
- The owner approves the design in its pull request.
