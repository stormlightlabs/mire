+++
id = "arcl-t-01M5SXD54ZDX8WPC9Z4XA549WH"
plan = "arcl-pl-01M5SXBH6FB0R59YSEPYTNFTV4"
title = "Release workflow and first crates.io publish"
status = "pending"
priority = "normal"
position = 12
blocked-by = ["arcl-t-01M5SXD50PB0DXVVAFPTC24B9R"]
+++

Automate releases and make the first crates.io publish.

Adapt fabrial's `release.yml` and `release-tag.yml`: a release pull request bumps the version and writes the changelog section, and merging it tags `v<version>`, creates the GitHub release, and publishes to crates.io. Use git-cliff (`cliff.toml`) in place of fabrial's `changelog.sh`.

Acceptance criteria:
- [ ] A release workflow opens the release pull request with the version bump and the git-cliff section prepended to CHANGELOG.md.
- [ ] Merging it tags the release, creates the GitHub release, and runs `cargo publish` in dependency order with `CARGO_REGISTRY_TOKEN`.
- [ ] The existing hand-written Unreleased entries are kept, either as the first release's notes or above the generated section.
- [ ] Installation docs cover `cargo install mire`.

Verification:
- A dry run of the workflow on a branch.
- The first release is installable with `cargo install mire`.
