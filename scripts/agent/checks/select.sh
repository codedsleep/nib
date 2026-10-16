# select — the files column's selection model (7.2): the cursor is not the
# selection; Space, v (visual), Ctrl+A, Ctrl+R, Esc; commands act on the
# selection, else the cursor row.
source "$ROOT/scripts/agent/checks/lib.sh"
focus_files
vin key g key g
nsel() { q '.files.selection | length'; }
check "moving the cursor selects nothing" test "$(nsel)" = 0
vin key space key space
check "Space twice: two rows" test "$(nsel)" = 2
vin key v key j key j
check "visual mode shows in the status line" test "$(q .status.mode)" = "-- VISUAL --"
check "v j j adds three rows" test "$(nsel)" = 5
vin key v
check "v again keeps them" test "$(nsel)" = 5
vin key Escape
check "Esc clears" test "$(nsel)" = 0
rows=$(q .files.rows)
vin key ctrl+a
check "Ctrl+A selects all $rows" test "$(nsel)" = "$rows"
vin key ctrl+r
check "Ctrl+R inverts to none" test "$(nsel)" = 0
vin key space key ctrl+r
check "Space then Ctrl+R leaves all but one" test "$(nsel)" = "$((rows - 1))"
vin key Escape key g key g
vin type cn
settle 0.2
check "with nothing selected, cn copies the cursor row" has "$(status_right)" '^copied attachments$'
vin key space key space type cn
settle 0.2
check "with a selection, cn copies the selection" has "$(status_right)" '^copied attachments'$'\n''daily$|^copied attachments.daily$'
