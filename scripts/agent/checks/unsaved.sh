# Unsaved buffers survive note replacement, failed saves, close and conflicts.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
active() { q '.tabs[] | select(.active) | .path'; }
modified() { q '.tabs[] | select(.active) | .modified'; }
open_note linker06.md
focus_editor
vin key G key o type 'keep this edit' key Escape
if [[ -n ${AUTOSAVE_OFF:-} ]]; then
    run edit-note linker07
    settle 0.3
    check 'autosave off: navigation keeps the current buffer' test "$(active):$(modified)" = linker06.md:true
    check 'autosave off: navigation does not write' bash -c '! rg -q "keep this edit" "$1"' _ "$VAULT/linker06.md"
else
    # A fresh temp blocks atomic installation without relying on root permissions.
    touch "$VAULT/.linker06.nib-tmp"
    run edit-note linker07
    settle 0.3
    check 'failed save: navigation keeps the current buffer' test "$(active):$(modified)" = linker06.md:true
fi
run quit
settle 0.3
check 'quit asks before losing an unsaved buffer' has "$(q .dialog)" 'Unsaved changes in linker06.md'
vin type n
settle 0.3
check 'cancelled quit preserves the edit' test "$(modified)" = true
run close-tab
settle 0.3
check 'tab close asks before losing the edit' has "$(q .dialog)" 'unsaved changes'
vin type n
settle 0.2
if [[ -z ${AUTOSAVE_OFF:-} ]]; then rm "$VAULT/.linker06.nib-tmp"; fi
run save
settle 0.3
check 'explicit save keeps the edit on disk' rg -q 'keep this edit' "$VAULT/linker06.md"
run config
settle 0.3
config_before=$(sha256sum "$AGENT_HOME/config/nib/agent/nib.toml" | cut -d' ' -f1)
focus_editor
vin key G key o type '# deliberately unsaved config' key Escape key alt+t
settle 1.2
check 'switching tabs never autosaves config' test "$(sha256sum "$AGENT_HOME/config/nib/agent/nib.toml" | cut -d' ' -f1)" = "$config_before"
