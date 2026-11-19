+++
id = "arcl-t-01M7P49PR473ZMJ991EYH7G9T8"
plan = "arcl-pl-01M5YRTHS95J67NZ6THREDG33F"
title = "Leave internal docs out of the 0.1.0 changelog"
status = "completed"
priority = "normal"
position = 0
evidence = "Done in #33 and #35: cliff.toml skips 15 internal or vague commits by SHA and rewrites 33 terse subjects by exact match; git-cliff fails on unconventional subjects"
+++

Mire has no release yet, so the first Release PR run writes every conventional commit since the project began into the 0.1.0 section. That list mixes user-facing changes with internal planning docs, docs-site internals and vague early subjects such as "better ui".

Clean the list in `cliff.toml`, not by hand on the release pull request, because rerunning Release PR regenerates the section.

Acceptance criteria:
- [ ] `cliff.toml` skips the internal docs, docs-site internals and vague commits the owner named, by full SHA, and none of their subjects appear in `git cliff --config cliff.toml --unreleased --tag v0.1.0 --strip all`.
- [ ] The terse early subjects are rewritten into specific lines that name the real command or feature, matched on the whole original subject, and each line matches the CLI and the docs.
- [ ] The Documentation group lists only user-facing docs: license and README, the docs site, the review workflow guide and screencaps.
- [ ] Commits after these keep today's behaviour: the PR title becomes the entry.
- [ ] A comment in `cliff.toml` says the lists are a pre-release cleanup, and that a new entry is only for a subject git-cliff rejects.
- [ ] git-cliff fails on a subject that is not a conventional commit (`require_conventional`), so `just check-changelog`, `scripts/release.sh` and the Release tag check never drop one silently.
