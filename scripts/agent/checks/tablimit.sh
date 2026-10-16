# The 33rd tab retains its note and evicts the least recently used clean tab.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
for n in $(seq 0 32); do
    printf -v name 'folder%02d/note%05d.md' "$((n % 40))" "$n"
    nib "$VAULT/$name" >/dev/null
done
settle 0.3
check '32 tabs remain after opening 33 notes' test "$(q '.tabs | length')" = 32
check 'newest note remains active' test "$(q '.tabs[] | select(.active) | .path')" = folder32/note00032.md
check 'oldest clean note was closed' test "$(q '[.tabs[] | select(.path == "folder00/note00000.md")] | length')" = 0
focus_editor
vin key G key o type 'newest tab is editable' key Escape
run save
settle 0.3
check 'newest tab saves its own buffer' rg -q 'newest tab is editable' "$VAULT/folder32/note00032.md"
for n in $(seq 1 32); do
    run tab "$n"
    vin key i type x key Escape
done
vin key alt+t
settle 0.3
check 'all dirty tabs: creating a tab retains all 32 buffers' test "$(q '[.tabs[] | select(.modified)] | length')" = 32
check 'all dirty tabs: a notice explains the limit' has "$(status_right)" 'all 32 tabs have unsaved changes'
run config
settle 0.3
check 'all dirty tabs: config cannot replace an existing buffer' test "$(q '[.tabs[] | select(.modified)] | length')" = 32
