---
title: Installation
description: Install Mire from a release or from source and confirm that it runs.
section: Get started
group: Get started
order: 1
---

Mire needs Git. Prebuilt binaries cover macOS on Apple silicon
(`aarch64-apple-darwin`) and Linux on x86-64 (`x86_64-unknown-linux-gnu`).
Each GitHub release publishes them.

## Install a release

The shell installer downloads the binary for your platform into
`~/.cargo/bin`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/mire/releases/latest/download/mire-installer.sh | sh
```

With Homebrew:

```sh
brew install stormlightlabs/tap/mire
```

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) fetches the same
release archive. Mire is not on crates.io, so pass the repository:

```sh
cargo binstall --git https://github.com/stormlightlabs/mire mire
```

Add `~/.cargo/bin` to `PATH` if your shell cannot find `mire` after the shell
installer or cargo-binstall.

## Install from source

Building from source needs Rust 1.88 or newer, Node 22 and pnpm. The binary
embeds the web app, so build that first:

```sh
git clone https://github.com/stormlightlabs/mire.git
cd mire
pnpm install
pnpm --filter @stormlightlabs/mire-app build
cargo install --path crates/cli --locked
```

Cargo places the `mire` executable in its binary directory, usually
`~/.cargo/bin`.

## Confirm the installation

```sh
mire --version
mire --help
```

Mire needs a terminal for the interactive viewer. Commands that emit structured
JSON can run without one.

For generated man pages and shell completions, see [Man pages and shell
completions](/docs/guides/man-pages-and-completions/).

## Update Mire

Run the shell installer or `cargo binstall` again, or
`brew upgrade stormlightlabs/tap/mire`. For a source install, pull and repeat
the build:

```sh
git pull --ff-only
pnpm --filter @stormlightlabs/mire-app build
cargo install --path crates/cli --locked
```

Continue with the [quick start](/docs/getting-started/quick-start/) to open your
first changeset.

## Uninstall

Remove `~/.cargo/bin/mire` after the shell installer, or run
`cargo uninstall mire` after cargo-binstall or a source install. With Homebrew:

```sh
brew uninstall mire
```

Removing the executable does not remove review files you created.
