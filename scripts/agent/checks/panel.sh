# panel — the right panel (6.6): hub.md's backlinks list its fourteen linking
# notes with the linking line (link in bold), Enter opens a source at its
# line, Tab switches to the outline, which follows the cursor's section, and
# Alt+B / F8 show, hide and focus it.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note hub.md
settle 0.5
check "panel shows the backlinks page" test "$(q .panel.page)" = backlinks
check "14 linking notes" has "$(q '.panel.rows[0]')" '^\(14 linking notes\)$'
check "fourteen source headers" test "$(q '[.panel.rows[] | select(startswith("# "))] | length')" = 14
check "linking lines bold the link" has "$(q '.panel.rows[2]')" '<b>!?\[\[hub'
vin key alt+b
settle 0.3
check "Alt+B focuses the panel" test "$(q .focus)" = panel
vin key Down key Down key Return
settle 0.5
check "Enter opened a linking note" has "$(q '.tabs[] | select(.active) | .path')" '^linker[0-9]+\.md$'
check "at the linking line" test "$(q .cursor.line)" = 2
open_note folder00/note00000.md
vin key alt+b key Tab
settle 0.6
check "Tab switched to the outline" test "$(q .panel.page)" = outline
check "outline lists the headings" has "$(q '.panel.rows | join("|")')" 'Note 0.*Detail'
focus_editor
vin type :18 key Return
settle 0.6
check "outline follows the cursor's section" test "$(q .panel.section)" = Detail
vin key F8
settle 0.2
check "F8 hides the panel" test "$(q .panels.right)" = false
vin key F8
settle 0.2
check "F8 shows it again" test "$(q .panels.right)" = true
