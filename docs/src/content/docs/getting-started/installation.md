---
title: Installation
description: Install Mire from source and confirm that the terminal viewer is ready.
section: Get started
group: Get started
order: 1
---

Mire requires Git and Rust 1.88 or newer. It currently installs from a source
checkout.

## Install from source

Clone the repository and install the CLI with Cargo:

```sh
git clone https://github.com/stormlightlabs/mire.git
cd mire
cargo install --path crates/cli --locked
```

Cargo places the `mire` executable in its binary directory, usually
`~/.cargo/bin`. Add that directory to `PATH` if your shell cannot find the
command.

## Confirm the installation

```sh
mire --version
mire --help
```

Mire needs a terminal for the interactive viewer. Commands that emit structured
JSON can run without one.

## Update Mire

Pull the newer source and repeat the install command:

```sh
git pull --ff-only
cargo install --path crates/cli --locked
```

Continue with the [quick start](/docs/getting-started/quick-start/) to open your
first changeset.

## Uninstall

```sh
cargo uninstall mire
```

Removing the executable does not remove review files you created.
