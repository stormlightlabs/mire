---
title: Man pages and shell completions
description: Install Mire’s generated manual page and shell completion scripts.
section: Guides
group: Guides
order: 6
---

Building the CLI writes `assets/man/mire.1` and completion scripts in
`assets/comp/` in the source checkout. Run this when those files are absent:

```sh
cargo build -p mire
```

The generated directories are ignored by Git. Rebuild after updating Mire to
regenerate them.

## Man page

Copy the manual page to a `man1` directory on your `MANPATH`:

```sh
mkdir -p "$HOME/.local/share/man/man1"
install -m 644 assets/man/mire.1 "$HOME/.local/share/man/man1/"
```

If `~/.local/share/man` is not already on `MANPATH`, add it in your shell
profile before running `man mire`:

```sh
export MANPATH="$HOME/.local/share/man:$MANPATH"
```

## Bash

```sh
mkdir -p "$HOME/.local/share/bash-completion/completions"
install -m 644 assets/comp/mire.bash "$HOME/.local/share/bash-completion/completions/mire"
```

Bash completion loaders usually read that directory. Otherwise, source the
installed file from `.bashrc`.

## Zsh

```sh
mkdir -p "$HOME/.zfunc"
install -m 644 assets/comp/_mire "$HOME/.zfunc/"
```

Add the following before `compinit` in `.zshrc`:

```zsh
fpath=("$HOME/.zfunc" $fpath)
autoload -Uz compinit
compinit
```

### Oh My Zsh

Copy the completion to Oh My Zsh's custom completions directory:

```sh
mkdir -p "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/completions"
install -m 644 assets/comp/_mire "${ZSH_CUSTOM:-$HOME/.oh-my-zsh/custom}/completions/"
```

Open a new shell. Oh My Zsh adds that directory to `fpath` before it runs
`compinit`.

## Fish

```sh
mkdir -p "$HOME/.config/fish/completions"
install -m 644 assets/comp/mire.fish "$HOME/.config/fish/completions/"
```

Fish loads completion files from that directory in new shells.

## PowerShell

```powershell
$completionDir = Join-Path $HOME ".config/powershell/Completions"
New-Item -ItemType Directory -Force $completionDir
Copy-Item assets/comp/_mire.ps1 (Join-Path $completionDir "mire.ps1")
```

Add this line to your PowerShell profile to load the completion:

```powershell
. (Join-Path $HOME ".config/powershell/Completions/mire.ps1")
```
