#!/usr/bin/env bash
# Shared payload installer; only these explicitly owned files are removed.
set -euo pipefail
prefix=${HOME}/.local
yes=0 uninstall=0 payload=""
while (($#)); do
    case $1 in
        --prefix) prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --payload) payload=${2:?--payload needs a directory}; shift 2 ;;
        --uninstall) uninstall=1; shift ;;
        -y) yes=1; shift ;;
        *) echo "install: unknown option $1" >&2; exit 2 ;;
    esac
done
[[ $prefix == /* && $prefix != / ]] || { echo 'install: --prefix must be an absolute directory other than /' >&2; exit 2; }
files=(bin/nib share/applications/io.github.codedsleep.Nib.desktop
    share/icons/hicolor/scalable/apps/io.github.codedsleep.Nib.svg
    share/man/man1/nib.1 share/gtksourceview-5/styles/nib-nord.xml
    share/gtksourceview-5/styles/nib-light.xml)
if (( !yes )); then
    [[ -t 0 ]] || { echo 'install: use -y when running without a terminal' >&2; exit 2; }
    read -r -p "Install/uninstall nib in $prefix? [y/N] " answer
    [[ $answer == y || $answer == Y ]] || exit 1
fi
if ((uninstall)); then
    for file in "${files[@]}"; do rm -f -- "$prefix/$file"; done
    if [[ -d $prefix/share/applications ]]; then
        if command -v update-desktop-database >/dev/null; then update-desktop-database "$prefix/share/applications"; fi
        if [[ -z $(find "$prefix/share/applications" -name '*.desktop' -print -quit) ]]; then
            rm -f -- "$prefix/share/applications/mimeinfo.cache"
        fi
    fi
    if [[ -d $prefix/share/icons/hicolor && -z $(find "$prefix/share/icons/hicolor" ! -type d ! -name icon-theme.cache -print -quit) ]]; then
        rm -f -- "$prefix/share/icons/hicolor/icon-theme.cache"
    fi
    # Remove empty parents only; preserve unrelated applications and user data.
    for dir in share/icons/hicolor/scalable/apps share/icons/hicolor/scalable share/icons/hicolor share/icons share/applications share/man/man1 share/man share/gtksourceview-5/styles share/gtksourceview-5 share bin; do
        rmdir -- "$prefix/$dir" 2>/dev/null || true
    done
    rmdir -- "$prefix" 2>/dev/null || true
else
    [[ -n $payload ]] || { echo 'install: missing payload' >&2; exit 2; }
    for file in "${files[@]}"; do [[ -f $payload/$file ]] || { echo "install: missing $file" >&2; exit 1; }; done
    for file in "${files[@]}"; do
        mode=644; [[ $file != bin/nib ]] || mode=755
        install -D -m "$mode" -- "$payload/$file" "$prefix/$file"
    done
fi
if (( !uninstall )) && [[ -d $prefix/share/applications ]] && command -v update-desktop-database >/dev/null; then
    update-desktop-database "$prefix/share/applications"
fi
if (( !uninstall )) && [[ -d $prefix/share/icons/hicolor ]] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -t -f "$prefix/share/icons/hicolor" || true
fi
printf 'nib: %s in %s\n' "$([[ $uninstall == 1 ]] && echo uninstalled || echo installed)" "$prefix"
