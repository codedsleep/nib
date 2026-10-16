#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export RUSTFMT="${RUSTFMT:-/usr/bin/rustfmt}"
export NIB_ALLOW_ROOTS=
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/forbidden-deps.sh
cargo build --release --workspace
target/release/nib --version
echo 'Window smoke: skipped until Phase 5'
