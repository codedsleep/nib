# vimoff — with editor.vim = false every feature stays reachable (9.10):
# typing goes straight in, chords and the palette do the rest, list
# continuation works, and every P§17 command is in the registry.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note linker12.md
focus_editor
check "no vim mode in the status line" test "$(q .status.mode)" = ""
vin key ctrl+End key Return type "- typed directly" key Return type item key ctrl+s
settle 0.4
check "typing and list continuation without vim" grep -q '^- item$' "$VAULT/linker12.md"
vin key ctrl+Return key ctrl+s
settle 0.3
check "Ctrl+Enter toggles a task" grep -q '^- \[ \] item$' "$VAULT/linker12.md"
vin key alt+p type "outline"
settle 0.4
vin key Return
settle 0.3
check "the palette runs a :-only command" test "$(q .panel.page)" = outline
vin key alt+p type "go to a line"
settle 0.3
vin key Return
settle 0.3
vin type 1 key Return
settle 0.3
check "go to line through the palette" test "$(q .cursor.line)" = 1
for c in w w! wq x q q! qa e e! o new daily find tag follow check backlinks outline files read edit split \
    rename trash template attach tab config reindex help version paste-image; do
    check ":$c is registered" bash -c "grep -q '\"$c\"' '$ROOT/crates/nib/src/registry.rs'"
done
