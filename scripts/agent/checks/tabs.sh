# tabs — the tab bar and per-tab history (9.1, 9.2): Alt+T, Alt+W,
# Alt+[ Alt+], :tab N|new|close|next|prev, O and Shift+Enter open in a new
# tab, an open note switches to its tab, Alt+Left/Right with the cursor
# line remembered, ● on a modified tab, and an untitled tab named on :w.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
ntabs() { q '.tabs | length'; }
active() { q '.tabs[] | select(.active) | .path'; }
open_note hub.md
vin key alt+t
settle 0.3
check "Alt+T: a second, untitled tab" test "$(ntabs):$(active)" = 2:untitled
vin key alt+[
settle 0.2
check "Alt+[ back to hub.md" test "$(active)" = hub.md
vin key alt+]
settle 0.2
check "Alt+] to the untitled tab" test "$(active)" = untitled
focus_editor
vin key i type "fresh words" key Escape type :w key Return
settle 0.4
check ":w on an untitled tab asks for a name" has "$(q .dialog)" 'Name for the new note'
vin type "named on write" key Return
settle 0.5
check "it became a note" test -f "$VAULT/named on write.md"
check "the tab shows it" test "$(active)" = "named on write.md"
focus_files
vin type /linker02 key Return type O
settle 0.4
check "O opens in a new tab" test "$(ntabs):$(active)" = 3:linker02.md
vin key alt+o type note00040
settle 0.4
vin key shift+Return
settle 0.4
check "Shift+Enter in the switcher opens a new tab" test "$(ntabs):$(active)" = 4:folder00/note00040.md
nib "$VAULT/hub.md" >/dev/null
settle 0.4
check "an open note switches to its tab" test "$(ntabs):$(active)" = 4:hub.md
vin type ":tab 3" key Return
settle 0.3
check ":tab 3" test "$(active)" = linker02.md
vin type ":tab next" key Return
settle 0.3
check ":tab next" test "$(active)" = folder00/note00040.md
vin type ":tab prev" key Return
settle 0.3
check ":tab prev" test "$(active)" = linker02.md
focus_editor
vin type :2 key Return type ":e linker03" key Return
settle 0.4
check "the tab now shows linker03" test "$(active)" = linker03.md
vin key alt+Left
settle 0.4
check "Alt+Left: back to linker02.md" test "$(active)" = linker02.md
check "the cursor line was remembered" test "$(q .cursor.line)" = 2
vin key alt+Right
settle 0.4
check "Alt+Right: forward to linker03.md" test "$(active)" = linker03.md
vin key alt+Left
settle 0.4
vin key i type x
settle 0.2
check "a modified tab shows ●" has "$(q '.tabs[] | select(.active) | .title')" '●$|^linker02'
vin key BackSpace key Escape type :w key Return key alt+w
settle 0.4
check "Alt+W closed it" test "$(ntabs)" = 3
vin type ":tab close" key Return
settle 0.3
check ":tab close" test "$(ntabs)" = 2
