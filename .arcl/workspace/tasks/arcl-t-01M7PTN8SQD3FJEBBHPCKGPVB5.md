+++
id = "arcl-t-01M7PTN8SQD3FJEBBHPCKGPVB5"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Call it the browser review everywhere"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #36: the UI is the browser review in ROADMAP, error messages, the meta description, open arcl records and the 0.1.0 changelog"
+++

Use "browser review" as the one term for Mire's browser UI, in user-facing text, open planning records and the changelog. The owner asked for it everywhere.

Keep "web" where it is technical: the `packages/app` build in developer docs, recipe names such as `just web-themes`, file and directory names, code identifiers, crate names and API paths. Do not rename code, files or routes.

Acceptance criteria:
- [ ] CLI help, README, ROADMAP.md, packages/docs, the app's visible strings (titles, labels, aria-labels, page title), the bundled skill text and error messages say "browser review" where they meant the review UI the user sees.
- [ ] Generated help output, such as man pages or completions, is regenerated with the repository's tooling.
- [ ] Open arcl specs, plans, tasks and captures use the term. Completed and discarded records stay as written.
- [ ] cliff.toml rewrites say "browser review", and exact-subject preprocessors rename the merged subjects of #20, #23 and #30.
- [ ] `just check-changelog` passes and `git cliff --config cliff.toml --unreleased --tag v0.1.0 --strip all` shows "browser review" and no "web review".
- [ ] Tests and snapshots that assert on changed strings are updated on purpose.
- [ ] A case-insensitive search for "web review" and "web UI" leaves only occurrences with a stated reason.

Verification:
- `just check` and the e2e suite pass.
- A 1440x900 screenshot shows the changed visible text.
