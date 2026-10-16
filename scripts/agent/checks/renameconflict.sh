# Rename conflicts must never apply a skipped plan or target the wrong note.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note hub.md
before=$(bytes linker13.md)
run rename linker13.md
settle 0.3
vin type y
settle 0.4
check 'rename conflict asks what to do' has "$(q .dialog)" 'already exists'
vin type s
settle 0.5
check 'skip keeps the source' test -f "$VAULT/hub.md"
check 'skip keeps the existing destination' same_bytes linker13.md "$before"
check 'skip leaves all fourteen links unchanged' test "$(rg -l '\[\[hub' "$VAULT"/linker*.md | wc -l)" = 14
check 'skip keeps the open note identity' test "$(q '.tabs[] | select(.active) | .path')" = hub.md
run rename linker13.md
settle 0.3
vin type y
settle 0.4
vin type n
settle 1.2
check 'keep both creates the suffixed destination' test -f "$VAULT/linker13 (1).md"
check 'keep both removes the original name' test ! -e "$VAULT/hub.md"
check 'keep both preserves the existing destination contents except approved links' rg -q '# Linker 13' "$VAULT/linker13.md"
check 'all fourteen links name the actual destination' test "$(rg -l -F '[[linker13 (1)' "$VAULT"/linker*.md | wc -l)" = 14
check 'keep both follows the actual destination in the tab' test "$(q '.tabs[] | select(.active) | .path')" = 'linker13 (1).md'
