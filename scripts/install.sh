#!/usr/bin/env bash
# Build and install from a checkout, sharing the release archive's payload.
set -euo pipefail
cd "$(dirname "$0")/.."
for arg in "$@"; do
    if [[ $arg == --uninstall ]]; then exec scripts/install-files.sh "$@"; fi
done
cargo build --locked --release -p nib
archive=$(scripts/package.sh)
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
tar -xzf "$archive" -C "$stage"
scripts/install-files.sh --payload "$stage" "$@"
