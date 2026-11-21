#!/usr/bin/env bash
# Writes a release version into Cargo.toml and sax's entry in Cargo.lock, so
# `--locked` builds keep working. The release tag is the only source of the
# version; the repo itself carries 0.0.0.
set -euo pipefail

version="${1:-}"
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "usage: $0 X.Y.Z (got '${version}')" >&2
  exit 1
fi

cd "$(dirname "$0")/.."
sed -i "0,/^version = \".*\"/s//version = \"$version\"/" Cargo.toml
sed -i "/^name = \"sax\"$/{n;s/^version = \".*\"/version = \"$version\"/}" Cargo.lock

grep -q "^version = \"$version\"" Cargo.toml
grep -A1 '^name = "sax"$' Cargo.lock | grep -q "^version = \"$version\""
