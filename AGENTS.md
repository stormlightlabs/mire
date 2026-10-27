# AGENTS Guide

- Use Writing Rust & Reviewing Rust
  - Separate modules for maintainability and responsibilities/concerns
  - Symbol Order: constants, types, enums, structs, functions, then public before private
    within each category
  - Include rustdoc comments for exported & important symbols
  - no `pub(super)` or `pub(crate)`
- Use writing for documentation
  - Don't include references to milestones or tickets (internal planning language)
    ANYWHERE in source code
  - Keep packages/docs/ up to date with usage instructions, again with the writing skill
- The TUI can and should be verified with a herdr pane. Open a background tab
  and send key/mouse events. Use the current working tree as the diff to check.
- For a TUI change, run `just shots` (or `just shot` for a state it lacks) and
  report the PNG paths. Review snapshot changes with `cargo insta review` and
  commit only the ones you meant.

## Gates

```sh
just check    # web app build, fmt --check, clippy -D warnings, tests, frontend checks
```

`gates` in `.github/fabrial.toml` names this command. CI runs the same recipe
as the `check` job. Clippy warnings fail it whether or not your change caused
them.

## Plan

- Open work lives in arcl, in `.arcl/workspace`. Use the `arclightning` skill
  to pick up, hand off, and complete tasks. Put task status updates and new
  tasks in the PR they describe.
- [ROADMAP.md](ROADMAP.md) holds the product direction and scope.

## Commits and changelog

- PR titles are conventional commits (`feat: ...`, `fix: ...`, `docs: ...`).
  The squash commit uses the PR title, and git-cliff (`cliff.toml`) builds the
  changelog from those subjects on `main`.
- Leave `CHANGELOG.md` alone in feature PRs. The release process writes it.

<!-- fabrial:begin -->
## Branches and pull requests

This repository uses fabrial. `fab init` wrote this section and the files it
names, `fab update` refreshes them, and `fab check` reports where they have
drifted.

- Never commit to `main`. Push a `claude/<topic>`, `codex/<topic>` or
  `storm/<task>` branch; the Branch PR workflow opens the pull request as a
  draft from stormlight-thunderbot.
- Commits on those branches may carry the agent's attribution trailers. The
  squash commit replaces them with the PR title and number, such as
  `Add Pi dispatch (#21)`, the PR body above its
  `<!-- fabrial:footer` line, `Merged from <branch>.` and the owner's
  `Co-authored-by` line from `.github/fabrial.toml`. Attribution lines and
  session links in the body are dropped, so nothing on `main` carries AI
  attribution. Editing the title or body of a reviewed PR posts the new
  squash message. Auto-merge keeps the message it was turned on with.
- Before pushing, run the command `gates` names in `.github/fabrial.toml`.
- The owner approves the PR and merges it, with the posted squash message or
  their own edit of it. Agents never approve or merge.
- Keep the PR body to what changed and why. Put verification in a PR comment.
  Leave out attribution lines and session links.
- Agents post PR comments as stormlight-thunderbot through the Bot comment
  workflow (`.github/workflows/comment.yml`), never with their own GitHub
  access.
- The agent that opens a PR reviews it with the `storm` skill and ends with a
  `Review finished at <sha>` comment. That turns `storm/review` green for that
  head, marks the draft ready and sets the squash message in the bot's status
  comment, which it edits rather than posting anew. The owner merges, by hand
  or with auto-merge they turn on; agents and the gate never turn it on.
  `/storm:ready <sha>` from the owner, a member or a collaborator accepts
  that head as it is, and `/storm:stop` turns auto-merge off.

## Writing

Everything here is read by a person: docs, commit messages, PR titles and
bodies, comments, and replies. Keep it short and plain:

- Lead with the point, and say each thing once.
- Use short sentences, common words, and one term for one thing.
- Cut preambles, summaries of what was just said, and sign-offs.
- Name the property, the limit or the failure, not a judgment of it.
- Stay inside the length targets: a PR body around 150 words, a review comment
  200, any other comment 100.

`.claude/skills/writing-docs/` holds the full standard, and its
`references/tells.md` lists the patterns that make text read as generated.
Check a draft against both before committing or posting.
<!-- fabrial:end -->
