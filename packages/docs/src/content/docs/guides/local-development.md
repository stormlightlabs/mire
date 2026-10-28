---
title: Local development
description: Run the Mire API server and browser frontend together for UI development.
section: Guides
group: Guides
order: 10
---

The browser review surface has two parts: the Rust API server (`mire serve`) and
the Svelte frontend (`packages/app`). During development, Vite proxies API
requests from the frontend to the Rust server.

## Prerequisites

- Rust through rustup. `rust-toolchain.toml` pins the version CI uses, so
  clippy reports the same lints locally and in CI.
- Node.js 22 or newer
- pnpm 11 or newer
- [just](https://github.com/casey/just), to run the gates

## Run every check

```sh
pnpm install
just check
```

`just check` builds the web app into `crates/cli/assets/web`, then runs
`cargo fmt --check`, clippy with warnings denied, the Rust tests, and the
frontend type checks, lint, and unit tests. CI runs the same recipe, and a pull
request needs it to pass. `just fmt` formats Rust and frontend sources, and
`just fix` applies clippy's automatic fixes first.

## Track and deliver work

Open work lives in [arclightning](https://github.com/stormlightlabs/arclightning)
under `.arcl/workspace`. With `arcl` installed, `arcl ready` lists the tasks that
can start now and `arcl context <id>` prints one task's details.

Changes go through [fabrial](https://github.com/stormlightlabs/fabrial): push a
`claude/`, `codex/` or `storm/` branch, and the pull request goes through a
standard review and an adversarial review before the owner merges it. Pull
request titles are conventional commits, because the squash commit uses the
title and [git-cliff](https://git-cliff.org) builds the changelog from those
subjects. `just changelog` previews the entries since the last tag.

## Snapshot tests

The TUI tests render each state into a text buffer and compare it with an
[insta](https://insta.rs) snapshot under `crates/ui/src/snapshots`. The CLI
tests compare JSON output with snapshots under `crates/cli/tests/snapshots`.
When a change alters what Mire draws or prints, the tests fail and insta
writes the new output next to the old one as a `.snap.new` file.

Review the changes with [cargo-insta](https://insta.rs/docs/cli/):

```sh
cargo install cargo-insta
cargo insta test --workspace --review
```

Accept a snapshot only when the new output is what you meant, and commit it
with the change.

## Capture the TUI as images

`just shots` builds Mire, creates a fixture repository with a staged change
and a review holding two findings, and captures each scenario as a PNG and an
ANSI text file:

```sh
just shots                 # every scenario
just shots split note      # the named scenarios
scripts/shots.sh --list    # the scenario names
```

The scenarios cover the unified, split, and narrow layouts, help, a hidden
sidebar, a collapsed file, a focused finding, a live walkthrough, and
`NO_COLOR`. Each run prints the PNG paths. The files go to `$FAB_SHOT_DIR`,
resolved against the current directory, or else to
`$TMPDIR/fabrial-shots/<branch>`, named after the checkout's branch. The script
refuses an output directory inside the repository. The live walkthrough uses a
private `XDG_RUNTIME_DIR`, so other open `mire review` sessions do not affect
it.

`just shot <name> <cols> <rows> <command...>` captures one command of your own
through `.github/storm/shot.sh`. Pass `--wait-for <text>` to wait for text on
the screen, and `--keys <key>` to press keys before the capture.

The captures need tmux, [freeze](https://github.com/charmbracelet/freeze), and
jq. Where freeze crashes while writing a PNG, install `rsvg-convert` (the
`librsvg2-bin` package on Debian and Ubuntu); freeze uses it when it is on the
`PATH`.

When a change alters the TUI, the implementer and both reviewers capture the
affected states and report the PNG paths in the pull request, as fabrial's
`storm` skill describes. CI does not run the captures.

## Start the servers

Open two terminals from the repository root.

**Terminal 1 — Rust API server:**

```sh
cargo run -p mire -- serve examples/de47985-847ffe1.json --port 3737
```

The server prints a session URL:

```
Mire review server: http://127.0.0.1:3737/#<session-secret>
```

**Terminal 2 — Vite dev server:**

```sh
pnpm --filter @stormlightlabs/mire-app run dev
```

This starts the frontend on `http://localhost:5173`.

## Open the review

Open `http://localhost:5173`, then paste the complete session URL printed by
`mire serve` into the connection form. You can paste the secret after `#`
instead if you prefer.

The development app keeps the secret for the current browser tab, so page
reloads and Vite updates reconnect without another paste. When you restart
`mire serve`, paste the newly printed URL into the form.

## How the proxy works

The Vite config in `packages/app/vite.config.ts` proxies `/api` requests to the
Rust server:

```ts
const mireServerOrigin = process.env.MIRE_SERVER_ORIGIN ?? 'http://127.0.0.1:3737';

server: {
  proxy: {
    '/api': {
      target: mireServerOrigin,
      changeOrigin: true
    }
  }
}
```

If you change the `--port` on `mire serve`, set `MIRE_SERVER_ORIGIN` to match:

```sh
MIRE_SERVER_ORIGIN=http://127.0.0.1:4000 pnpm --filter @stormlightlabs/mire-app run dev
```

## Build the frontend

When you are ready to test a production build:

```sh
pnpm --filter @stormlightlabs/mire-app run build
```

This writes the static site to `crates/cli/assets/web`, which the Rust binary
embeds with `include_dir`. The next `cargo build` or `cargo run` picks up the
new assets.

To preview the production build locally:

```sh
pnpm --filter @stormlightlabs/mire-app run preview
```

This serves the built assets on `http://localhost:4173`. The preview server has
no API proxy — it only works when embedded in the `mire serve` binary.

## Run tests

```sh
# Unit tests
pnpm --filter @stormlightlabs/mire-app run test:unit

# Type checking
pnpm --filter @stormlightlabs/mire-app run check

# Browser end-to-end tests
just e2e
```

`just e2e` builds the web app and the `mire` binary, then runs the Playwright
tests in `packages/app/tests`. Each test creates a temporary Git repository
with a review, starts `mire serve` on it, and drives Chromium through loading
the review, moving between files and findings, resolving a finding, marking a
file viewed, exporting notes, and refreshing after a source edit. The test
stops the server and removes the repository when it ends, pass or fail.

The tests run `target/debug/mire`; set `MIRE_BIN` to test another build. They
need Playwright's Chromium, which you install once:

```sh
pnpm --filter @stormlightlabs/mire-app exec playwright install chromium
```

To use a Chromium that is already installed, set `MIRE_E2E_CHROMIUM` to its
executable. `just check` leaves these tests out to stay fast; CI runs them in
the `e2e` job of the Check workflow.

## Release Mire

Run the Release PR workflow from the Actions tab with the new version, such as
`0.2.0`. It sets the workspace version, writes the version's section of
`CHANGELOG.md` with `scripts/release.sh`, and pushes `storm/release-<version>`
for review. Edit the changelog lines in that pull request before merging it.
The first release, before any `v*` tag exists, can keep the current version.

The merge tags `v<version>`. The tag runs dist's Release workflow, which builds
the archives for each target, publishes the GitHub release with the changelog
section and the shell installer, and commits the `mire` formula to
`stormlightlabs/homebrew-tap`.

`just release <version>` runs the changelog script locally. It needs
[git-cliff](https://git-cliff.org) and fails when `v<version>` is already
tagged, `CHANGELOG.md` already has its section, or the version differs from
the one in `Cargo.toml`. After changing
`dist-workspace.toml` or `.github/build-setup.yml`, run `dist generate` to
rewrite `.github/workflows/release.yml`.

## Troubleshooting

### "This review URL is missing its session secret"

Paste the full session URL from the `mire serve` output into the connection
form. The form also accepts the secret after `#` by itself.

### Port already in use

If port 3737 is occupied, pass a different port to `mire serve`:

```sh
cargo run -p mire -- serve review.json --port 4000
```

Then set the matching origin for Vite:

```sh
MIRE_SERVER_ORIGIN=http://127.0.0.1:4000 pnpm --filter @stormlightlabs/mire-app run dev
```

If port 5173 is occupied, Vite picks the next available port automatically.
Check the terminal output for the actual URL.

### API requests return 401

The session secret is per-process. If you restart `mire serve`, the old secret
is invalid. Paste the new URL from the terminal into the connection form.

### "Review unavailable" after editing the review file

If another process (like `mire review refresh`) modifies the review file, the
server detects the revision change and returns a conflict. Click **Reload
review** in the browser, or refresh the page.

### Frontend changes not appearing

Vite hot-reloads frontend changes automatically. If changes are not reflected:

1. Check the Vite terminal for compilation errors.
2. Restart the Vite dev server.

### CSS or font changes not appearing

The embedded fonts and styles are served from `packages/app/src/app.css`. After
changes, Vite hot-reloads them in dev mode. For production builds, run
`pnpm --filter @stormlightlabs/mire-app run build` to update the embedded
assets.
