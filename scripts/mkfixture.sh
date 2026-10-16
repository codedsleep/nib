#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export NIB_ALLOW_ROOTS="${XDG_CACHE_HOME:-$HOME/.cache}/nib-fixture"
cargo run --quiet -p nib-vault --example mkfixture -- "$@"
