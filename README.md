# sax

A simple but *s*mart *a*rchiving and e*x*traction tool.

Will automatically detect archive type and extract correctly, you no longer need to google tar flags.

sax is developed for linux only, I have no interest into testing targets I don't personally use, but feel free to contribute.

## Installation

### From source

```bash
cargo install --git https://github.com/rahmerh/sax
```

## Usage

```bash
sax <archive> <out>
sax --help
sax --version
```

Extracts the contents of an archive into the output directory, creating it if needed.

Supported archive formats: zip, tar, tar.gz, tgz, tar.xz, txz, tar.bz2, tbz2, tar.zst, tzst, 7z, rar.

```bash
sax input.zip out/
sax backup.tar.gz restored/
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
