# external — another program's changes (9.5): an unmodified tab reloads in
# place keeping the cursor line; a modified one only gets a notice; a removed
# note stays and :w recreates it; a renamed note's tab follows it.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note linker09.md
focus_editor
vin type :2 key Return
printf '# Linker 9\n![[hub#Heading|alias 9]]\nchanged outside\n' >"$VAULT/linker09.md.tmp"
mv "$VAULT/linker09.md.tmp" "$VAULT/linker09.md"
settle 1.0
check "reloaded with a notice" has "$(status_right)" 'reloaded linker09.md'
check "cursor still on line 2" test "$(q .cursor.line)" = 2
vin key i type mine key Escape
echo "outside again" >>"$VAULT/linker09.md"
settle 1.0
check "a modified tab is not reloaded" has "$(status_right)" 'changed on disk'
check "and keeps its edit" test "$(q '.tabs[0].modified')" = true
run edit-note linker10
settle 0.3
check "navigation keeps the conflicted buffer" test "$(q '.tabs[] | select(.active) | .path')" = linker09.md
run quit
settle 0.3
check "quit asks about the conflicted buffer" has "$(q .dialog)" 'Unsaved changes in linker09.md'
vin type n
settle 0.3
focus_editor
vin type ':e!' key Return
settle 0.4
check ":e! reloads" bash -c "grep -q 'outside again' <<<\"\$(cat '$VAULT/linker09.md')\" && test \"\$(jq -r '.tabs[0].modified' <<<\"\$('$ROOT/target/debug/nib' --profile agent --test-dump)\")\" = false"
rm "$VAULT/linker09.md"
settle 1.0
check "removal is a notice; the tab stays" has "$(status_right)" 'removed on disk'
vin type :w key Return
settle 0.4
check ":w recreates it" test -f "$VAULT/linker09.md"
mv "$VAULT/linker09.md" "$VAULT/linker-nine.md"
settle 1.2
check "the tab followed the rename" test "$(q '.tabs[0].path')" = linker-nine.md
