# rename — link-aware rename (7.4): renaming hub.md asks to update 14 links
# in 14 notes, rewrites them keeping aliases and headings, patches an open
# linker's buffer in place, and u (vim undo) undoes that patch.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note linker03.md
before=$(cat "$VAULT/linker03.md")
focus_files
vin type /hub key Return
check "cursor on hub.md" test "$(q .files.cursor)" = hub.md
vin type r
settle 0.3
check "rename asks for a name" has "$(q .dialog)" 'Rename to'
vin key ctrl+a type "centre.md" key Return
settle 0.4
check "it asks to update 14 links in 14 notes" test "$(q .dialog)" = "Update 14 links in 14 notes?"
vin type y
settle 1.5
check "hub.md is gone" test ! -e "$VAULT/hub.md"
check "centre.md exists" test -f "$VAULT/centre.md"
check "no linker still points at hub" bash -c "! grep -l '\\[\\[hub' '$VAULT'/linker*.md"
check "linker00 keeps heading and alias" grep -q '!\[\[centre#Heading|alias 0\]\]' "$VAULT/linker00.md"
check "14 linkers mention centre" test "$(grep -l 'centre' "$VAULT"/linker*.md | wc -l)" = 14
check "status reports the rewrite" has "$(status_right)" 'updated 14 links in 14 notes'
check "the open linker03 buffer was patched (not modified)" test "$(q '.tabs[] | select(.path == "linker03.md") | .modified')" = false
focus_editor
vin key u
settle 0.3
check "u undoes the patch in the buffer" test "$(q '.tabs[] | select(.path == "linker03.md") | .modified')" = true
vin type :w key Return
settle 0.4
check "and the undone text is the old text" test "$(cat "$VAULT/linker03.md")" = "$before"
