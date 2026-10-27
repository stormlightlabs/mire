+++
id = "arcl-t-01M5YRVB5XW3TXHV1HDFZ3Z00Y"
plan = "arcl-pl-01M5YRTHSYB0P33XC2VD8FFK2D"
title = "Design checkpoints and viewed state"
status = "completed"
priority = "normal"
position = 1
evidence = "Done in #14 at a1eb63f: design page for checkpoints and viewed state, ROADMAP.md updated; approved by merge."
+++

Write a design page under `packages/docs/src/content/docs/concepts/` that fixes the review-file schema for viewed state and checkpoints, using the decisions in the spec. Cover how fingerprints are stored, when Mire rebuilds old text from Git and when it keeps a patch copy, the size bound on those copies, how older review files load, and what the API returns. Update the Viewed and Reviewed items and the client-state paragraph in ROADMAP.md to match.

The design also decides how a note links to the checkpoint it was written on. An anchor stores only the hunk and content fingerprints, so either the anchor gains a checkpoint reference or Mire finds the checkpoint from those fingerprints. It also says what notes written before checkpoints get.

ROADMAP.md says viewed progress is browser-local in "Human decisions" and in the TypeScript test plan ("local viewed-state namespacing"). Change both to viewed state in the review file.

Verification:
- The owner approves the design in its pull request.
