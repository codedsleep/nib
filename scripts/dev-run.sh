#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
fixture="${XDG_CACHE_HOME:-$HOME/.cache}/nib-fixture"
isolation="${XDG_CACHE_HOME:-$HOME/.cache}/nib-dev"
for name in XDG_CONFIG_HOME XDG_STATE_HOME XDG_DATA_HOME GTK_THEME; do
    if [[ -v $name ]]; then export "NIB_HOST_${name}=${!name}"; else export "NIB_HOST_${name}__UNSET=1"; fi
done
export NIB_ALLOW_ROOTS="$fixture:$isolation" NIB_TRACE=1
export XDG_CONFIG_HOME="$isolation/config" XDG_STATE_HOME="$isolation/state" XDG_DATA_HOME="$isolation/data"
if [[ -z ${NIB_GTK_THEME:-} ]]; then
    NIB_GTK_THEME=$(gsettings get org.gnome.desktop.interface gtk-theme | tr -d "'")
    export NIB_GTK_THEME
fi
exec cargo run -p nib -- "$fixture" "$@"
