# complete — the [[ / ![[ / # completion popover (6.5, 6.7): ambiguous stems
# insert their shortest unique link, aliases insert link|alias, headings
# after #, tags, attachments first for embeds; Esc dismisses; Ctrl+Space
# reopens. run-all.sh runs a second session with the index held back, where
# the popover says "index not ready" and the editor still takes text.
source "$ROOT/scripts/agent/checks/lib.sh"
if [[ -n ${NOT_READY:-} ]]; then
    open_note hub.md
    focus_editor
    vin key G key o type "[[amb"
    settle 0.3
    check "popover open while indexing" test "$(q .completion.visible)" = true
    check "it says the index is not ready" has "$(dump)" 'index not ready'
    vin key Escape type " typed" key Escape
    settle 0.2
    check "the editor kept the text" has "$(q '.tabs[0].modified')" true
    return 0 2>/dev/null || exit 0
fi
wait_q .index_ready true 15
open_note hub.md
focus_editor
accepted() { tail -n 1 "$VAULT/hub.md"; }
vin key G key o type "see [[amb"
settle 0.4
check "popover open on [[amb" test "$(q .completion.visible)" = true
check "first row is an ambiguous note" has "$(q '.completion.rows[0]')" 'ambiguous|Ambiguous'
vin key Tab type " end" key Escape type :w key Return
settle 0.4
check "accepting wrote the shortest unique link, cursor after ]]" \
    has "$(accepted)" '^see \[\[(ambiguous|folder[0-9]+/ambiguous)\]\] end$'
vin key o type "[[alias12"
settle 0.4
check "alias row shown as alias → title" has "$(q '.completion.rows[0]')" '^alias12 → '
vin key Return key Escape type :w key Return
settle 0.4
check "alias accepted as link|alias" has "$(accepted)" '^\[\[note00012\|alias12\]\]$'
vin key o type "[[hub#Hea"
settle 0.4
vin key Return key Escape type :w key Return
settle 0.4
check "heading completion" test "$(accepted)" = "[[hub#Heading]]"
vin key o type "![[image0"
settle 0.4
check "embeds list attachments first" has "$(q '.completion.rows[0]')" 'attachments/image0'
vin key ctrl+n key ctrl+p key Return key Escape type :w key Return
settle 0.4
check "embed accepted with the unique basename" has "$(accepted)" '^!\[\[image0[0-9]\.png\]\]$'
vin key o type "#fixt"
settle 0.4
check "tag completion offers #fixture" has "$(q '.completion.rows[0]')" '^#fixture'
vin key Tab key Escape type :w key Return
settle 0.4
check "tag accepted" test "$(accepted)" = "#fixture"
vin key o type "[[zzzz"
settle 0.4
vin key Escape
settle 0.2
check "Esc dismissed the popover" test "$(q .completion.visible)" = false
check "and stayed in insert mode" has "$(q .status.mode)" 'INSERT'
vin key ctrl+space
settle 0.3
check "Ctrl+Space reopened it" test "$(q .completion.visible)" = true
vin key Escape key Escape
key_us=$(grep -o 'name=completion_key us=[0-9.]*' "$OUT/nib.log" | sed 's/.*us=//' | sort -n | awk '{a[NR]=$1} END {print a[int(NR/2)+1] " median, " a[NR] " worst over " NR}')
echo "completion key path (debug build): $key_us µs"
