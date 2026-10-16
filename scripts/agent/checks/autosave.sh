# autosave — 1 s after the last edit the bytes are on disk (9.4), and on a
# tab switch; with editor.autosave = false, :q asks and :qa asks once,
# listing the notes.
source "$ROOT/scripts/agent/checks/lib.sh"
if [[ -z ${AUTOSAVE_OFF:-} ]]; then
    open_note linker06.md
    focus_editor
    vin key G key o type "autosaved line" key Escape
    start=$(date +%s%N)
    until grep -q 'autosaved line' "$VAULT/linker06.md" 2>/dev/null; do
        (( $(date +%s%N) - start < 3000000000 )) || break
        sleep 0.05
    done
    elapsed=$(( ($(date +%s%N) - start) / 1000000 ))
    echo "autosave landed after ${elapsed} ms"
    check "on disk within 1.5 s" test "$elapsed" -le 1500
    vin key o type "switch saves" key Escape key alt+t
    settle 0.3
    check "a tab switch saved at once" grep -q 'switch saves' "$VAULT/linker06.md"
else
    open_note linker07.md
    focus_editor
    vin key G key o type "not saved" key Escape
    settle 1.6
    check "autosave off: nothing written" bash -c "! grep -q 'not saved' '$VAULT/linker07.md'"
    vin type :q key Return
    settle 0.3
    check ":q asks" has "$(q .dialog)" 'unsaved changes'
    vin type n
    nib "$VAULT/linker08.md" >/dev/null
    settle 0.3
    focus_editor
    vin key G key o type "also not saved" key Escape type :qa key Return
    settle 0.3
    check ":qa asks once, listing both" has "$(q .dialog)" 'linker07.md, linker08.md|linker08.md, linker07.md'
    vin type n
    settle 0.2
    check "and nib is still there" kill -0 "$app_pid"
fi
