+++
id = "arcl-t-01M7PV7CCJ6XXJMZZMFCC2T0HA"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Update fabrial to 0.9.2"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #37 at 4fb077c: fabrial 0.9.2 installed with no drift; fab-* skills, Branch PR open, Tasks section in AGENTS.md"
+++

Update the fabrial-managed files from 0.7.1 to 0.9.2 with `fab update`, keeping this repository's settings.

Acceptance criteria:

- [ ] `.github/fabrial.toml` records version 0.9.2 and keeps `owner`, `trailer` and `gates`
- [ ] `fab check` reports no drift after the update
- [ ] Files the release no longer installs are removed the way fabrial documents
- [ ] Branch PR, Bot comment and the Review gate read the App secrets this repository already has
- [ ] Repository-specific sections of `AGENTS.md` survive
- [ ] `just check` passes
