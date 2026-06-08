# `sax`

![License](https://img.shields.io/github/license/rahmerh/sax)
[![Crates.io](https://img.shields.io/crates/v/sax)](https://crates.io/crates/sax)
[![Publish crate](https://github.com/rahmerh/sax/actions/workflows/publish.yml/badge.svg)](https://github.com/rahmerh/sax/actions/workflows/publish.yml)

A simple but *s*mart *a*rchiving and e*x*traction tool.

Will automatically detect archive type and extract correctly, you no longer need to look up tar flags.

sax is developed for linux only, I have no interest into testing targets I don't personally use, but feel free to contribute.

sax can create and extract zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2,
tbz2, tar.zst, tzst, and 7z archives. RAR archives can only be extracted.

## Installation

### From the AUR

Arch users can install the stable [`sax`](https://aur.archlinux.org/packages/sax)
package with an AUR helper such as [`yay`](https://github.com/Jguer/yay):

```bash
yay -S sax
```

To build the latest commit from the `main` branch instead, install
[`sax-git`](https://aur.archlinux.org/packages/sax-git):

```bash
yay -S sax-git
```

VCS package updates require development-package checking, for example
`yay -Syu --devel`.

### From crates.io

```bash
cargo install sax
```

## Usage

Arguments are positional: input(s) come first and the output is always last. When
the output is omitted, sax prompts for it.

#### Extracting archives

```bash
sax <archive> <out>
```

Extracts the contents of an archive into the output directory, creating it if needed.

#### Archiving files and directories

```bash
sax <input>... <archive>
```

Creates an archive containing one or more input files or directories, creating
all required subdirectories.

### Configuration

sax stores configuration in `~/.config/sax/config.yaml` by default, or
`$XDG_CONFIG_HOME/sax/config.yaml` when `XDG_CONFIG_HOME` is set.

- `extract_prefs.strip_top_level_dir`: controls whether sax removes a single
  top-level directory from an archive before extracting into the output
  directory. Defaults to `true`.

### Optional flags

These flags override configuration settings for a single execution.

- `--strip` removes a single top-level directory.
- `--no-strip` keeps a single top-level directory.
