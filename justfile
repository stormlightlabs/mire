# Every gate CI runs. fabrial's skills run it before pushing.
check:
    pnpm --filter @stormlightlabs/mire-app build
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    pnpm -r --if-present check
    pnpm -r --if-present lint
    pnpm -r --if-present test:unit --run

# Build the web app and the binary, then run the Playwright tests against `mire serve`
e2e:
    pnpm --filter @stormlightlabs/mire-app build
    cargo build -p mire
    pnpm --filter @stormlightlabs/mire-app test:e2e

# Format Rust and frontend sources
fmt:
    cargo fmt --all
    pnpm -r --if-present format

# Apply clippy's automatic fixes, then format
fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty
    just fmt

# Capture a terminal UI as a PNG with .github/storm/shot.sh (needs tmux and freeze)
[positional-arguments]
shot +args:
    @.github/storm/shot.sh "$@"

# Capture Mire's TUI scenarios, or the named ones, against a fixture repository
[positional-arguments]
shots *names:
    @scripts/shots.sh "$@"

# Print the changelog entries for commits since the last tag
changelog:
    git cliff --unreleased

# deploy the kit doc site & then push
deploy-docs:
    pnpm --filter @stormlightlabs/mire-docs run deploy && git push
