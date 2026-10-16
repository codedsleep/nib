#!/usr/bin/env bash
# Package an already-built release binary in the same layout used by installers.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
arch=${1:-$(uname -m)}
[[ $arch == x86_64 || $arch == aarch64 ]] || { echo "unsupported architecture: $arch" >&2; exit 2; }
command -v scdoc >/dev/null || { echo 'Install scdoc to build the man page' >&2; exit 1; }
python3 scripts/generate-docs.py --check
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
install -Dm755 target/release/nib "$stage/bin/nib"
strip "$stage/bin/nib"
install -Dm644 data/io.github.codedsleep.Nib.desktop "$stage/share/applications/io.github.codedsleep.Nib.desktop"
install -Dm644 data/io.github.codedsleep.Nib.svg "$stage/share/icons/hicolor/scalable/apps/io.github.codedsleep.Nib.svg"
mkdir -p "$stage/share/man/man1" "$stage/share/gtksourceview-5/styles"
scdoc <data/nib.1.scd >"$stage/share/man/man1/nib.1"
install -m644 data/style-schemes/*.xml "$stage/share/gtksourceview-5/styles/"
install -m755 scripts/install-files.sh "$stage/install-files.sh"
install -m644 LICENSE README.md CHANGELOG.md "$stage/"
mkdir -p target/dist
tar -czf "target/dist/nib-$version-$arch-linux.tar.gz" -C "$stage" .
printf '%s\n' "target/dist/nib-$version-$arch-linux.tar.gz"
