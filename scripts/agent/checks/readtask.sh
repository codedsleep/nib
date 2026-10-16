# readtask — clicking a task glyph in the reading view writes [x] to the
# source line on disk and flips only that glyph (8.5).
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note folder00/note00000.md
vin key alt+e
settle 0.6
shot readtask-before
vin move 244 160 click
settle 0.6
check "the source line is [x] on disk" grep -q '^- \[x\] task$' "$VAULT/folder00/note00000.md"
check "the glyph flipped" has "$(q .reading.text)" '☑ task'
check "the buffer is saved" test "$(q '.tabs[] | select(.active) | .modified')" = false
vin move 244 160 click
settle 0.6
check "and back to [ ]" grep -q '^- \[ \] task$' "$VAULT/folder00/note00000.md"
