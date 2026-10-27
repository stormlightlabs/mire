+++
id = "arcl-s-01M5YRT4HC2DEDXR5S1V8ZFK4F"
title = "Changes over time"
status = "open"
acceptance-criteria = """
- A design doc under `packages/docs` defines checkpoints and viewed state, and ROADMAP.md matches it.
- The review file stores viewed state and checkpoints, and older review files still load.
- The browser and the terminal show what changed since a file was marked viewed.
- A stale note can show the code it was written on."""
+++

Show how a change moved while it was under review: what changed since you last looked, which revisions touched a file, and the code a stale note was written on. The review file does not hold this yet, so the work starts with a design that fixes the schema.

Decisions already made:

- A checkpoint is a revision where a file's fingerprint changed. The review file stores the fingerprints. Mire rebuilds the old text from Git, or from bounded patch copies when Git cannot.
- Viewed state moves into the review file, so it survives handoff and is the same in the terminal and the browser. This replaces the browser-local viewed state in ROADMAP.md.
