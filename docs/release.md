# Release

How CI checks changes and how a version reaches crates.io and the AUR.
Code: `.github/workflows/`, `packaging/aur/`, `Cargo.toml`

## Where it lives
- Repo and CI: GitHub, `https://github.com/rahmerh/sax`, GitHub Actions on `ubuntu-latest`.
- Repo secrets the workflows need, both kept in Proton Pass (vault Personal):
  - `CARGO_REGISTRY_TOKEN`: item "Crates.io github actions token" (field API Key, scope publish-update
    for `sax`, has an expiry date). An expired token fails `publish-crate` with `403 Forbidden`.
  - `AUR_SSH_PRIVATE_KEY`: item "AUR" (SSH key for AUR account `rahmerh`, also served locally through the
    Proton Pass SSH agent).

## CI (`.github/workflows/ci.yml`, pull requests into `main`)
- The branch must be rebased onto `main` and hold no merge commits.
- `cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked`.
- Pushes straight to `main` don't run CI; check locally first (commands in `CLAUDE.md`).

## Cutting a release (`.github/workflows/publish.yml`)
1. Bump `version` in `Cargo.toml` (`cargo build` updates `Cargo.lock`), commit as `Release version X.Y.Z`.
2. Tag `vX.Y.Z` and push the tag. Only tags matching `v*.*.*` publish.
3. `verify` checks the tag matches the Cargo version and runs tests. Then, in parallel:
   - `publish-crate`: `cargo publish --locked`.
   - `publish-aur`: pushes `packaging/aur/sax/PKGBUILD` to AUR `sax` with `pkgver` and `sha256sums` filled
     in from GitHub's tag tarball. `.SRCINFO` is made in an `archlinux:base-devel` container.

## sax-git (`.github/workflows/publish-aur-git.yml`)
- On a push to `main` that changes `packaging/aur/sax-git/` or the workflow itself, or by hand
  (`workflow_dispatch`), pushes that PKGBUILD to AUR `sax-git` with
  `pkgver=<cargo version>.r<commit count>.g<short hash>`.

## Decisions
- `pkgver` and `sha256sums` in the repo's PKGBUILDs are stale on purpose: CI fills them in when it
  publishes, so don't "fix" them by hand.
