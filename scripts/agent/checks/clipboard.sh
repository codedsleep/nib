# clipboard — files yanked in nib paste in the installed trek and the
# reverse (7.3, P§8), inside one cage session. trek's offer is read with
# wl-paste while trek runs and put back by wl-copy after trek exits (cage
# cannot hand the keyboard back to nib while trek's window is up).
# wl-copy/wl-paste need a keyboard on the seat; vinput's comes and goes with
# each command, so one idle vinput keeps a keyboard there throughout.
source "$ROOT/scripts/agent/checks/lib.sh"
trek_bin=${TREK_BIN:-$HOME/.local/bin/trek}
[[ -x $trek_bin ]] || { echo "FAIL trek is not installed at $trek_bin"; exit 1; }
"$NIB_VINPUT" sleep 120000 >/dev/null 2>&1 &
keyboard=$!
settle 0.3
focus_files
vin type /hub key Return type y
settle 0.3
check "nib offers the GNOME and uri-list formats" has "$(wl-paste --list-types)" 'x-special/gnome-copied-files'
check "the GNOME form is 'copy' and a file URI" has "$(wl-paste -t x-special/gnome-copied-files)" '^copy'
TREK_ALLOW_ROOTS="$VAULT" "$trek_bin" --profile nibcheck "$VAULT/folder01" >"$OUT/trek1.log" 2>&1 &
trek=$!
sleep 2
vin key p
sleep 1.5
check "trek pasted nib's yank" test -f "$VAULT/folder01/hub.md"
kill "$trek"; wait "$trek" 2>/dev/null || true
settle 0.5
TREK_ALLOW_ROOTS="$VAULT" "$trek_bin" --profile nibcheck "$VAULT/folder02" >"$OUT/trek2.log" 2>&1 &
trek=$!
sleep 2
vin key y
sleep 0.5
wl-paste -t x-special/gnome-copied-files >"$OUT/trek-offer" 2>/dev/null || true
yanked=$(sed -n 2p "$OUT/trek-offer" | sed 's|^file://||')
# trek owning the clipboard does not leave on SIGTERM promptly.
kill -9 "$trek"; wait "$trek" 2>/dev/null || true
check "trek offered a copy of a file" has "$(head -1 "$OUT/trek-offer")" '^copy$'
wl-copy --foreground -t x-special/gnome-copied-files <"$OUT/trek-offer" >/dev/null 2>&1 &
copier=$!
settle 0.5
focus_files
vin type gh type /folder03 key Return type l
settle 0.4
vin type p
settle 1.0
name=$(basename "$yanked")
check "nib pasted trek's yank ($name)" test -f "$VAULT/folder03/$name"
# By PID only: the owner's own trek and wl-copy share this PID namespace.
kill "$keyboard" "$copier" 2>/dev/null || true
