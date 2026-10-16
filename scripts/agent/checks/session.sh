# session — closing and reopening restores the tabs, their cursor lines, the
# active tab, panel visibility, the files column's folder, cursor and sort,
# and pane sizes (9.3). run-all.sh runs it twice with --keep.
source "$ROOT/scripts/agent/checks/lib.sh"
if [[ -z ${SECOND:-} ]]; then
    wait_q .index_ready true 15
    open_note hub.md
    vin key alt+t
    nib "$VAULT/linker04.md" >/dev/null
    settle 0.3
    focus_editor
    vin type :2 key Return key l key l key l
    nib "$VAULT/folder00/note00000.md" >/dev/null
    settle 0.3
    vin type :20 key Return
    vin key alt+e
    settle 0.3
    vin key F8
    focus_files
    vin type gd type ,m type /2026-09-05 key Return
    settle 0.3
    vin key 'alt+['
    settle 0.3
    dump >"$AGENT_HOME/session-before.json"
    vin key ctrl+q
    sleep 1.5
    check "nib quit on :qa" bash -c "! kill -0 $app_pid 2>/dev/null"
else
    settle 1.0
    before="$AGENT_HOME/session-before.json"
    now=$(dump)
    for f in '.tabs | map(.path) | join(",")' '.tabs[] | select(.active) | .path' '.panels.right' \
        '.files.folder' '.files.cursor'; do
        check "restored $f" test "$(jq -r "$f" <<<"$now")" = "$(jq -r "$f" "$before")"
    done
    check "the active tab's cursor line" test "$(jq -r .cursor.line <<<"$now")" = "$(jq -r .cursor.line "$before")"
    check "the active tab's cursor column" test "$(jq -r .cursor.column <<<"$now")" = "$(jq -r .cursor.column "$before")"
    vin key 'alt+]'
    settle 0.3
    check "the reading tab came back in reading mode" test "$(q .reading.mode)" = reading
fi
