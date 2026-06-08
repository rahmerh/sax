# `sax` 🎷

![License](https://img.shields.io/github/license/rahmerh/sax)
[![Crates.io](https://img.shields.io/crates/v/sax)](https://crates.io/crates/sax)
[![Publish crate](https://github.com/rahmerh/sax/actions/workflows/publish.yml/badge.svg)](https://github.com/rahmerh/sax/actions/workflows/publish.yml)

A simple but smart archiving and extraction tool.

Will automatically detect archive type and extract correctly, you no longer need to look up tar flags.

sax is developed for linux only, I have no interest into testing targets I don't personally use, but feel free to contribute.

sax can create and extract zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2,
tbz2, tar.zst, tzst, and 7z archives. RAR archives can only be extracted.

## Installation

### From the AUR

Arch users can install the stable [`sax`](https://aur.archlinux.org/packages/sax) or the latest commit [`sax-git`](https://aur.archlinux.org/packages/sax-git)
package with an AUR helper such as [`yay`](https://github.com/Jguer/yay):

```bash
yay -S sax
```

### From crates.io

```bash
cargo install sax
```

## Usage

#### Extracting archives

```bash
sax archive.tzst out/
```

Extracts the contents of an archive into the output directory, creating it if needed.

#### Archiving files and directories

```bash
sax folder/ file.txt ... archive.tzst
```

Creates an archive containing one or more input files or directories, creating
all required subdirectories.

### Configuration

sax stores configuration in `~/.config/sax/config.yaml` by default, or
`$XDG_CONFIG_HOME/sax/config.yaml` when `XDG_CONFIG_HOME` is set.

- `extract_prefs.strip_top_level_dir`: controls whether sax removes a single
  top-level directory from an archive before extracting into the output
  directory. Defaults to `true`.
- `extract_prefs.output_mode`: controls how the extraction output directory is
  selected. Defaults to `prompt`.
  - `auto`: derives a directory beside the archive from its name.
  - `prompt`: uses an output directory argument when provided, otherwise asks
    interactively.
  - `manual`: requires an output directory argument.

### Optional flags

These flags override configuration settings for a single execution.

- `--strip` removes a single top-level directory.
- `--no-strip` keeps a single top-level directory.
