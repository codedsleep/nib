#!/usr/bin/env bash
# Private-repository installer. gh handles authentication; no curl fallback.
set -euo pipefail
repo=codedsleep/markdown-trekfm-rust
prefix=$HOME/.local version="" yes=0 uninstall=0
while (($#)); do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --version) version=${2:?--version needs a tag}; shift 2 ;;
        --uninstall) uninstall=1; shift ;;
        -y) yes=1; shift ;;
        -h|--help) echo 'Usage: install.sh [-y] [--prefix DIR] [--version vX.Y.Z] [--uninstall]'; exit 0 ;;
        *) echo "install: unknown option $1" >&2; exit 2 ;;
    esac
done
[[ $prefix == /* && $prefix != / ]] || { echo 'install: --prefix must be an absolute directory other than /' >&2; exit 2; }
if ((!yes)); then
    [[ -t 0 ]] || { echo 'install: use -y when running without a terminal' >&2; exit 2; }
    read -r -p "Install/uninstall nib in $prefix? [y/N] " answer
    [[ $answer == y || $answer == Y ]] || exit 1
fi
if ((uninstall)); then
    # Kept identical to scripts/install-files.sh so uninstall needs no download.
    for file in bin/nib share/applications/io.github.codedsleep.Nib.desktop \
        share/icons/hicolor/scalable/apps/io.github.codedsleep.Nib.svg share/man/man1/nib.1 \
        share/gtksourceview-5/styles/nib-nord.xml share/gtksourceview-5/styles/nib-light.xml; do
        rm -f -- "$prefix/$file"
    done
    if [[ -d $prefix/share/applications ]]; then
        if command -v update-desktop-database >/dev/null; then update-desktop-database "$prefix/share/applications"; fi
        if [[ -z $(find "$prefix/share/applications" -name '*.desktop' -print -quit) ]]; then
            rm -f -- "$prefix/share/applications/mimeinfo.cache"
        fi
    fi
    if [[ -d $prefix/share/icons/hicolor && -z $(find "$prefix/share/icons/hicolor" ! -type d ! -name icon-theme.cache -print -quit) ]]; then
        rm -f -- "$prefix/share/icons/hicolor/icon-theme.cache"
    fi
    for dir in share/icons/hicolor/scalable/apps share/icons/hicolor/scalable share/icons/hicolor share/icons share/applications share/man/man1 share/man share/gtksourceview-5/styles share/gtksourceview-5 share bin; do
        rmdir -- "$prefix/$dir" 2>/dev/null || true
    done
    rmdir -- "$prefix" 2>/dev/null || true
    echo "nib: uninstalled from $prefix (config and state kept)"
    exit 0
fi
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'install: this release supports Linux x86_64; build from source on other architectures' >&2; exit 1; }
if ! command -v gh >/dev/null || ! gh auth status >/dev/null 2>&1; then
    echo 'install: install gh, then run gh auth login with access to the private repository' >&2; exit 1
fi
if ! ldconfig -p 2>/dev/null | grep 'libgtksourceview-5.so.0' >/dev/null; then
    echo 'install: GtkSourceView 5 runtime missing; run: sudo dnf install gtksourceview5 gtk4' >&2
    exit 1
fi
if [[ -n $version && ! $version =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$ ]]; then
    echo 'install: --version must be a release tag such as v0.1.0' >&2; exit 2
fi
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
args=(--repo "$repo" --dir "$tmp" --pattern 'nib-*-x86_64-linux.tar.gz' --pattern SHA256SUMS)
[[ -z $version ]] || args=("$version" "${args[@]}")
gh release download "${args[@]}"
shopt -s nullglob
archives=("$tmp"/nib-*-x86_64-linux.tar.gz)
((${#archives[@]} == 1)) || { echo 'install: expected exactly one archive' >&2; exit 1; }
archive=${archives[0]##*/}
# Verify only our selected architecture; do not require other release assets.
checksum=$(awk -v file="$archive" '$2 == file || $2 == "*" file {print $1}' "$tmp/SHA256SUMS")
[[ $checksum =~ ^[a-fA-F0-9]{64}$ ]] || { echo 'install: missing or ambiguous archive checksum' >&2; exit 1; }
(cd "$tmp"; printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --strict -)
mkdir "$tmp/payload"
tar -xzf "${archives[0]}" -C "$tmp/payload" --no-same-owner --no-same-permissions
bash "$tmp/payload/install-files.sh" -y --prefix "$prefix" --payload "$tmp/payload"
"$prefix/bin/nib" --version
