# Shared helpers for the UI checks, sourced by each driver after agent-run.sh's
# own helpers (shot, vin, run, dump, q, wait_q, check, has, bytes, settle).
# shellcheck shell=bash

# open_note REL: open a vault note in the current tab through a second
# invocation (the single-instance path) and wait until it is the current tab.
open_note() {
    nib "$VAULT/$1" >/dev/null
    wait_q '.tabs[] | select(.active) | .path' "$1" 5
}

# focus_editor / focus_files: the window chords.
focus_editor() { vin key alt+2; settle 0.2; }
focus_files() { vin key alt+1; settle 0.2; }

# status_right: the notice/index slot of the status line.
status_right() { q .status.right; }

# same_bytes REL SHA: the file still has the given sha256.
same_bytes() { [[ $(bytes "$1") == "$2" ]]; }
