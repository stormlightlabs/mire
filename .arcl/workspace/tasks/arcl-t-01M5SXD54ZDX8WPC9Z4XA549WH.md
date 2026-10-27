+++
id = "arcl-t-01M5SXD54ZDX8WPC9Z4XA549WH"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Release workflow and GitHub releases"
status = "pending"
priority = "normal"
position = 12
+++

Release Mire the way fabrial releases: prebuilt binaries on GitHub releases and a Homebrew formula. Mire is not published to crates.io.

Mirror fabrial's Release PR, Release tag and dist Release workflows and its Homebrew publish job. `fab release` stays in fabrial, so a script in this repository writes the changelog section with git-cliff (`cliff.toml`) and checks the tag. Build for fabrial's two targets, `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`; adding a target later is one line in the dist config.

Acceptance criteria:
- [ ] A script in this repository writes the release's CHANGELOG.md section with git-cliff and `cliff.toml`. It fails when the tag `v<version>` already exists or `<version>` differs from the workspace version.
- [ ] A Release PR workflow, run with a version, sets the workspace version, runs that script, and pushes `storm/release-<version>` for Branch PR to open.
- [ ] The dist config builds `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`, and the release has an archive for each.
- [ ] Merging that pull request tags `v<version>` on the merge commit with the bot's token.
- [ ] The tag runs a dist-generated Release workflow that builds the web app before `cargo build`, publishes the GitHub release with the archives, the shell installer and the changelog section, and commits the `mire` formula to stormlightlabs/homebrew-tap.
- [ ] README and the installation docs cover the shell installer and `brew install stormlightlabs/tap/mire`.

Verification:
- `dist plan` passes in the Release PR workflow.
- The first release installs with the shell installer and with Homebrew.
