# sax

Rust CLI that creates and extracts archives, guessing the format from the file name and whether to create
or extract from the arguments. Public, Linux only. Published to crates.io and the AUR (`sax`, `sax-git`).

## Docs

How sax is meant to behave lives in `docs/`, one file per area. Don't load them up front. Read the one for
an area when you need it, or before changing its behaviour. When behaviour changes or a rule is agreed,
update that file in the same commit. They describe sax as it is now, not its history.

- `docs/cli.md`: how arguments pick extract or create, output modes, prompts, the config file, error messages
- `docs/archives.md`: supported formats and detection, how each is extracted and created, top-level stripping, path safety, permissions, symlinks
- `docs/release.md`: CI checks, branch rules, cutting a release, crates.io and AUR publishing

## Stack

- Rust, edition 2024. CLI via `clap`, prompts via `dialoguer`, config via `serde_yaml`.
- One crate per format: `zip`, `tar`, `flate2`, `xz2`, `bzip2`, `zstd`, `sevenz-rust`, `unrar-ng`.
- Tests: unit tests inside each module (`kernal` asserts, `mockall` for the prompt, `tempfile`).

## Rules

- Linux only. Don't add code or CI for other targets.
- Nothing extracted may land outside the output directory: new extract code goes through `safe_join`
  (`src/util/fs.rs`).
- Never overwrite an existing archive when creating one.
- CI must stay green: `cargo fmt`, `cargo clippy -D warnings` and `cargo test`, all `--locked`. CI only runs on pull requests, so run them before pushing to `main`.
- README.md is the user-facing page (crates.io and the repo show it). Keep its format list and config
  section in line with `docs/`.

## Commands

- Build: `cargo build`
- Test: `cargo test --locked`
- Check like CI: `cargo fmt -- --check && cargo clippy --locked -- -D warnings`
- Try it: `cargo run -- samples/sample.tar.gz /tmp/out` (`samples/` has one archive per format)

## Backlog

Empty. Add items here when something is asked for later.
