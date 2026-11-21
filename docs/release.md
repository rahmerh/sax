# Release

How CI checks `main` and how a tag becomes a release on crates.io and the AUR.
Code: `.github/workflows/`, `packaging/set-version.sh`, `packaging/aur/`

## Where it lives
- Repo and CI: GitHub, `https://github.com/rahmerh/sax`, GitHub Actions on `ubuntu-latest`.
- All work happens on `main`; there are no release branches or pull requests.
- Repo secrets the workflows need, both kept in Proton Pass (vault Personal):
  - `CARGO_REGISTRY_TOKEN`: item "Crates.io github actions token" (field API Key, scope publish-update
    for `sax`, has an expiry date). An expired token fails `publish-crate` with `403 Forbidden`.
  - `AUR_SSH_PRIVATE_KEY`: item "AUR" (SSH key for AUR account `rahmerh`, also served locally through the
    Proton Pass SSH agent).

## Versions
- The release tag is the only source of the version. The repo's `Cargo.toml` and `Cargo.lock` stay at
  `0.0.0`, so a local build reports `sax 0.0.0`.
- `packaging/set-version.sh X.Y.Z` writes the version into `Cargo.toml` and sax's `Cargo.lock` entry, so
  `--locked` builds still work. It rejects anything that isn't `X.Y.Z`. Used by the publish workflow and
  both PKGBUILDs; never commit its result.

## CI (`.github/workflows/ci.yml`)
- Every push to `main` (or by hand): `cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`,
  `cargo test --locked`.

## Cutting a release (`.github/workflows/publish.yml`)
1. `git tag vX.Y.Z` on a commit on `main`, then `git push origin vX.Y.Z`. Nothing to bump or commit first.
2. `verify` fails unless the tagged commit is on `main`, then sets the version and runs the tests.
3. Then, in parallel:
   - `publish-crate`: sets the version, `cargo publish --locked --allow-dirty` (dirty because of the
     version write).
   - `publish-aur`: pushes `packaging/aur/sax/PKGBUILD` to AUR `sax` with `pkgver` and `sha256sums` filled
     in from GitHub's tag tarball. Its `prepare()` runs `set-version.sh "$pkgver"`. `.SRCINFO` is made in an
     `archlinux:base-devel` container.
- A version can be published to crates.io only once. If `publish-crate` fails, fix the cause and re-run
  that job (`gh run rerun <id> --failed`) instead of re-tagging.

## sax-git (`.github/workflows/publish-aur-git.yml`)
- Runs on a push to `main` that changes `packaging/aur/sax-git/` or the workflow itself, or by hand
  (`workflow_dispatch`). Pushes that PKGBUILD to AUR `sax-git`.
- `pkgver` is `<last tag>.r<commits since it>.g<short hash>` (from `git describe`), and `prepare()` sets
  the build's version to the last tag.

## Decisions
- `pkgver` and `sha256sums` in the repo's PKGBUILDs are stale on purpose: CI fills them in when it
  publishes, so don't "fix" them by hand.
- Version from the tag, not from `Cargo.toml`: a release is one `git tag`, with no bump commit to forget
  or get out of step.
