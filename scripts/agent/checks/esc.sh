# esc — Esc routing (6.9, G4): with a files-column selection, focusing the
# editor, i, Esc leaves the selection intact and vim at NOR. Also F1 lists
# every registry entry (6.8).
source "$ROOT/scripts/agent/checks/lib.sh"
focus_files
vin key j key space key j key space
settle 0.2
selected=$(q '.files.selection | length')
check "two rows selected" test "$selected" = 2
focus_editor
vin key i key Escape
settle 0.2
check "mode back at NOR" test "$(q .status.mode)" = NOR
check "files selection intact" test "$(q '.files.selection | length')" = 2
vin key F1
settle 0.4
check "F1 lists every registry entry" test "$(q .keys_shown)" = "$(q .registry_size)"
vin type save
settle 0.3
check "the key list filters" test "$(q .keys_shown)" -lt "$(q .registry_size)"
vin key Escape
settle 0.2
check "Esc closes the key list" test "$(q .keys_shown)" = null
