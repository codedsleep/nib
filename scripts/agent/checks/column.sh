# column — every remaining files-column key once (7.5): / ? n N find,
# f filter, ,n ,m ,r sort (,m shows ages), . hidden, gh gd ga gt, H L
# history, F5, F7 parent column, and the watcher refreshing the listing.
source "$ROOT/scripts/agent/checks/lib.sh"
focus_files
cur() { q .files.cursor; }
vin type /linker0 key Return
settle 0.2
check "/ finds linker00.md" test "$(cur)" = linker00.md
vin type n
check "n: linker01.md" test "$(cur)" = linker01.md
vin type N
check "N: back to linker00.md" test "$(cur)" = linker00.md
vin type '?hub' key Return
check "? finds backwards" test "$(cur)" = hub.md
vin type f type linker1
settle 0.3
check "f filters to the linker1x notes" test "$(q .files.rows)" = 4
vin key Return key Escape
settle 0.3
check "Esc clears the filter" test "$(q .files.rows)" -gt 40
vin type ,m
settle 0.3
check ",m sorts newest first" test "$(cur)" != ""
vin type ,n
settle 0.3
vin key g key g
check ",n: name order, attachments/ first" test "$(cur)" = attachments
vin type ,r
settle 0.3
vin key g key g
check ",r reverses it" test "$(cur)" != attachments
vin type ,r
settle 0.3
vin key g key g
check ",r again: attachments/ first" test "$(cur)" = attachments
vin type .
settle 0.3
check ". shows hidden files" has "$(dump)" '"hidden files shown"'
vin type .
vin type gd
settle 0.3
check "gd goes to daily/" test "$(q .files.folder)" = daily
vin type ga
settle 0.3
check "ga goes to attachments/" test "$(q .files.folder)" = attachments
vin type gt
settle 0.3
check "gt goes to templates/" test "$(q .files.folder)" = templates
vin type H
settle 0.3
check "H: back to attachments/" test "$(q .files.folder)" = attachments
vin type L
settle 0.3
check "L: forward to templates/" test "$(q .files.folder)" = templates
vin type gh
settle 0.3
check "gh goes to the root" test "$(q .files.folder)" = ""
vin type /folder05 key Return type l
settle 0.3
settle 0.3
vin key F7
settle 0.3
check "F7 shows the parent column" test "$(q .panels.parent)" = true
vin key F7
check "F7 hides it" test "$(q .panels.parent)" = false
before=$(q .files.rows)
touch "$VAULT/folder05/from-outside.md"
settle 1.0
check "the watcher added an outside file" test "$(q .files.rows)" = "$((before + 1))"
echo x > "$VAULT/folder05/f5-only.md"
vin key F5
settle 0.5
check "F5 rereads" test "$(q .files.rows)" -ge "$((before + 2))"
