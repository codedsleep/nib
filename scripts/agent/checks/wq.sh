# wq — vim mode in the status line, `:w` saved by nib (vim's own write signal
# never fires), `:wq` writes the bytes and closes the tab, unknown commands
# get a notice and vim's own (:N) still work (6.1, 6.2).
source "$ROOT/scripts/agent/checks/lib.sh"
open_note linker00.md
focus_editor
check "mode is NOR" test "$(q .status.mode)" = NOR
vin key i
settle 0.2
check "i shows -- INSERT --" test "$(q .status.mode)" = "-- INSERT --"
vin key End type " added" key Escape
settle 0.2
check "Esc returns to NOR" test "$(q .status.mode)" = NOR
vin type :w key Return
settle 0.4
check ":w saved" grep -q 'added' "$VAULT/linker00.md"
check ":w cleared the modified mark" test "$(q '.tabs[0].modified')" = false
check "vim's write signal never fired" bash -c "! grep -q 'vim write signal' '$OUT/nib.log'"
vin type :2 key Return
settle 0.3
check "vim's own :N still runs" test "$(q .cursor.line)" = 2
vin key i key End type " changed" key Escape
vin type :frob key Return
settle 0.3
check "unknown command notice" has "$(status_right)" 'not an editor command: frob'
vin type :wq key Return
settle 0.5
check ":wq wrote the bytes" grep -q 'changed' "$VAULT/linker00.md"
check ":wq closed the tab" test "$(q '.tabs[0].path')" = untitled
