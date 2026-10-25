+++
id = "arcl-t-01M5SXD50PB0DXVVAFPTC24B9R"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Decide the workspace layout for publishing"
status = "pending"
priority = "normal"
position = 3
+++

Decide the workspace layout before the first crates.io release. `mire` alone is fine to publish; decide whether `mire-core` and `mire-tui` are worth publishing too.

Publish a crate on its own only if something outside Mire would depend on it: the changeset and review model might serve fabrial or other tools, while the TUI is probably only useful inside Mire. crates.io requires every dependency to be published, so publishing only `mire` means folding the other crates into it.

Acceptance criteria:
- [ ] Compare publishing all three, publishing `mire` with the others folded in as modules, and other layouts. Record the decision and the reason in ROADMAP.md.
- [ ] Apply the layout.
- [ ] Each published crate has `description`, `license`, `repository`, `homepage`, `readme`, `keywords`, and `categories`, with shared fields in `[workspace.package]`. None of the crates has them today.
- [ ] `just check` runs a packaging dry run (`cargo package` or `cargo publish --dry-run`), which builds the web app first, since `mire` packages `assets/web`.

Verification:
- The packaging dry run passes in CI.
