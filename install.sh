#!/usr/bin/env bash
# nib installer — downloads a prebuilt nib and installs it for this user.
#
#   curl -fsSL https://raw.githubusercontent.com/codedsleep/nib/master/install.sh | bash
#
# Options (pass after `-s --` when piping: `... | bash -s -- --version v0.2.1`):
#   --prefix DIR     install under DIR instead of ~/.local
#   --version TAG    install a specific release (default: the latest)
#   -y               never prompt
#   --uninstall      remove an installed nib (config, sessions and notes stay)
#   -h, --help       print this and exit
#
# Installs the binary, a launcher, the icon, the man page, nib's style schemes
# and its Markdown language. The binary links against the system's GTK 4
# (4.14 or later) and GtkSourceView 5 (5.10 or later).
set -euo pipefail
repo=codedsleep/nib
# NIB_INSTALL_BASE_URL replaces the release download URL (used by scripts/test-install.sh).
base_url=${NIB_INSTALL_BASE_URL:-}
prefix=$HOME/.local version="" yes=0 uninstall=0
while (($#)); do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --version) version=${2:?--version needs a tag}; shift 2 ;;
        --uninstall) uninstall=1; shift ;;
        -y|--yes) yes=1; shift ;;
        -h|--help) echo 'Usage: install.sh [-y] [--prefix DIR] [--version vX.Y.Z] [--uninstall]'; exit 0 ;;
        *) echo "install: unknown option $1" >&2; exit 2 ;;
    esac
done
[[ $prefix == /* && $prefix != / ]] || { echo 'install: --prefix must be an absolute directory other than /' >&2; exit 2; }
if ((!yes)); then
    # Ask on the terminal, not stdin: when piped from curl, stdin is this script.
    if ! { exec 3<>/dev/tty; } 2>/dev/null; then
        echo 'install: use -y when running without a terminal' >&2; exit 2
    fi
    action=Install; ((uninstall)) && action=Uninstall
    printf '%s nib in %s? [y/N] ' "$action" "$prefix" >&3
    read -r answer <&3 || answer=""
    exec 3<&-
    [[ $answer == y || $answer == Y ]] || exit 1
fi
if ((uninstall)); then
    # Kept identical to scripts/install-files.sh so uninstall needs no download.
    for file in bin/nib share/applications/io.github.codedsleep.Nib.desktop \
        share/icons/hicolor/scalable/apps/io.github.codedsleep.Nib.svg share/man/man1/nib.1 \
        share/gtksourceview-5/styles/nib-nord.xml share/gtksourceview-5/styles/nib-light.xml \
        share/gtksourceview-5/language-specs/nib-markdown.lang; do
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
    for dir in share/icons/hicolor/scalable/apps share/icons/hicolor/scalable share/icons/hicolor share/icons share/applications share/man/man1 share/man share/gtksourceview-5/styles share/gtksourceview-5/language-specs share/gtksourceview-5 share bin; do
        rmdir -- "$prefix/$dir" 2>/dev/null || true
    done
    rmdir -- "$prefix" 2>/dev/null || true
    echo "nib: uninstalled from $prefix (config and state kept)"
    exit 0
fi
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'install: releases are built for Linux x86_64; build from source on other architectures' >&2; exit 1; }
command -v curl >/dev/null || { echo 'install: curl is required' >&2; exit 1; }
if ! ldconfig -p 2>/dev/null | grep 'libgtksourceview-5.so.0' >/dev/null; then
    echo 'install: GtkSourceView 5 is missing; install it first (Fedora: sudo dnf install gtk4 gtksourceview5)' >&2
    exit 1
fi
if [[ -n $version && ! $version =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$ ]]; then
    echo 'install: --version must be a release tag such as v0.2.1' >&2; exit 2
fi
if [[ -z $base_url ]]; then
    if [[ -n $version ]]; then
        base_url=https://github.com/$repo/releases/download/$version
    else
        base_url=https://github.com/$repo/releases/latest/download
    fi
fi
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
curl -fsSL --proto '=https,file' -o "$tmp/SHA256SUMS" "$base_url/SHA256SUMS" \
    || { echo "install: could not download SHA256SUMS from $base_url" >&2; exit 1; }
# The checksum list names the archive, so the latest release needs no API call.
mapfile -t names < <(awk '{name = $2; sub(/^\*/, "", name); if (name ~ /^nib-[^\/]*-x86_64-linux\.tar\.gz$/) print name}' "$tmp/SHA256SUMS")
((${#names[@]} == 1)) || { echo 'install: expected exactly one x86_64 archive in SHA256SUMS' >&2; exit 1; }
archive=${names[0]}
checksum=$(awk -v file="$archive" '$2 == file || $2 == "*" file {print $1}' "$tmp/SHA256SUMS")
[[ $checksum =~ ^[a-fA-F0-9]{64}$ ]] || { echo 'install: missing or ambiguous archive checksum' >&2; exit 1; }
curl -fsSL --proto '=https,file' -o "$tmp/$archive" "$base_url/$archive" \
    || { echo "install: could not download $archive" >&2; exit 1; }
(cd "$tmp"; printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check --strict -)
mkdir "$tmp/payload"
tar -xzf "$tmp/$archive" -C "$tmp/payload" --no-same-owner --no-same-permissions
bash "$tmp/payload/install-files.sh" -y --prefix "$prefix" --payload "$tmp/payload"
"$prefix/bin/nib" --version
case ":$PATH:" in *":$prefix/bin:"*) ;; *) echo "nib: add $prefix/bin to PATH" ;; esac
