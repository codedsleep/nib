# readtask — clicking a task glyph in the reading view writes [x] to the
# source line on disk and flips only that glyph (8.5).
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note folder00/note00000.md
vin key alt+e
settle 0.6
shot readtask-before
vin move 244 185 click
settle 0.6
check "the source line is [x] on disk" grep -q '^- \[x\] task$' "$VAULT/folder00/note00000.md"
check "the glyph flipped" has "$(q .reading.text)" '☑ task'
check "the buffer is saved" test "$(q '.tabs[] | select(.active) | .modified')" = false
vin move 244 185 click
settle 0.6
check "and back to [ ]" grep -q '^- \[ \] task$' "$VAULT/folder00/note00000.md"
vin key alt+e
focus_editor
vin key G key o type 'unsaved local edit' key Escape
printf '\nexternal edit\n' >>"$VAULT/folder00/note00000.md"
settle 0.5
vin key alt+e
settle 0.4
vin move 244 185 click
settle 0.4
check 'task clicks preserve an external change' rg -q '^external edit$' "$VAULT/folder00/note00000.md"
check 'task clicks do not overwrite a conflicted note' bash -c '! rg -q "unsaved local edit" "$1"' _ "$VAULT/folder00/note00000.md"
check 'the conflicted task edit stays unsaved in the buffer' test "$(q '.tabs[] | select(.active) | .modified')" = true
