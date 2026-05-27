# `sax`

![License](https://img.shields.io/github/license/rahmerh/sax)
[![Crates.io](https://img.shields.io/crates/v/sax)](https://crates.io/crates/sax)
[![Publish crate](https://github.com/rahmerh/sax/actions/workflows/publish.yml/badge.svg)](https://github.com/rahmerh/sax/actions/workflows/publish.yml)

A simple but *s*mart *a*rchiving and e*x*traction tool.

Will automatically detect archive type and extract correctly, you no longer need to google tar flags.

sax is developed for linux only, I have no interest into testing targets I don't personally use, but feel free to contribute.

## Installation

### From the AUR

Arch users can install the [`sax-git`](https://aur.archlinux.org/packages/sax-git)
package with an AUR helper such as [`yay`](https://github.com/Jguer/yay):

```bash
yay -S sax-git
```

### From crates.io

```bash
cargo install sax
```

## Usage

```bash
sax <archive> <out>
```

Extracts the contents of an archive into the output directory, creating it if needed.
If the archive contains a single top-level directory, sax strips it by default
and extracts that directory's contents directly into the output directory.

This can be controlled by setting your preference in the config file. 
Configuration is stored in `~/.config/sax/config.yaml` by default,
or `$XDG_CONFIG_HOME/sax/config.yaml` when `XDG_CONFIG_HOME` is set.

Alternatively, you can use `--strip` or `--no-strip` to override the configured behavior for a
single extraction.

Supported archive formats: zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2, tbz2, tar.zst, tzst, 7z, rar.

```bash
sax input.zip out/
sax backup.tar.gz restored/
sax --no-strip package.zip restored/
```

## Todo

- [x] Extracting zip archives
- [x] Extracting tar archives
- [x] Extracting 7z archives
- [x] Extracting rar archives
- [ ] Creating archives
    - [ ] Smart mode (automatically detect best archive for use case)
    - [ ] Manual mode (provide required format)
    - [ ] Override options
