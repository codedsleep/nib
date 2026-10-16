#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export RUSTFMT="${RUSTFMT:-/usr/bin/rustfmt}"
export NIB_ALLOW_ROOTS=
python3 scripts/generate-docs.py --check
bash -n install.sh scripts/install*.sh scripts/package.sh scripts/test-install.sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
if [[ ! -f ${XDG_CACHE_HOME:-$HOME/.cache}/nib-fixture/.fixture-marker ]]; then scripts/mkfixture.sh --seed 7; fi
cargo test --workspace
scripts/forbidden-deps.sh
RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -D warnings" cargo doc --no-deps -p nib-core -p nib-vault
cargo build --release --workspace
target/release/nib --version
scripts/test-install.sh
NIB_TRACE=1 cargo run --quiet --release -p nib-vault --example trace -- "${XDG_CACHE_HOME:-$HOME/.cache}/nib-fixture"
scripts/agent/smoke.sh
