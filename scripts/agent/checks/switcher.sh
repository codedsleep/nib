# switcher — the finder (7.6, 7.7): Alt+O ranks notes and creates one from
# the query; Alt+P runs a command; z finds below a folder; :tag lists tags
# then notes; Alt+F searches the vault, grouped, and opens at the line.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
focus_editor
vin key alt+o
settle 0.3
check "Alt+O opens the switcher" test "$(q .finder.source)" = switcher
check "the entry has the keyboard" test "$(q .focus)" = other
vin type note00040
settle 0.4
check "first row is Note 40" test "$(q '.finder.rows[0]')" = "Note 40"
vin key Return
settle 0.4
check "Enter opened it" test "$(q '.tabs[] | select(.active) | .path')" = folder00/note00040.md
vin key alt+o type "Brand new idea"
settle 0.4
check "a Create row leads" test "$(q '.finder.rows[0]')" = 'Create "Brand new idea"'
vin key Return
settle 0.5
check "created and opened" test -f "$VAULT/Brand new idea.md"
check "the tab shows it" test "$(q '.tabs[] | select(.active) | .path')" = "Brand new idea.md"
vin key alt+p type "parent column"
settle 0.4
vin key Return
settle 0.3
check "Alt+P ran 'Show or hide the parent column'" test "$(q .panels.parent)" = true
vin key F7
focus_files
vin type gh type z
settle 0.6
vin type 'image05'
settle 0.4
check "z lists paths below the folder" has "$(q '.finder.rows[0]')" 'attachments/image05.png'
vin key Escape
settle 0.2
check "Esc closes the finder and gives the keyboard back" test "$(q .focus)" = files
focus_editor
vin type :tag key Return
settle 0.4
check ":tag lists tags with counts" has "$(q '.finder.rows | join("|")')" '#fixture \([0-9]+\)'
vin type fixture key Return
settle 0.4
check "choosing a tag lists its notes" test "$(q .finder.source)" = tag-notes
vin key Escape
vin key alt+f type "Hub paragraph"
settle 0.8
check "Alt+F found the line, grouped under its note" has "$(q '.finder.rows | join("|")')" '(^|\|)hub\|    Hub paragraph'
vin type " -linked"
settle 0.8
check "-word excludes paths" test "$(q '.finder.rows | length')" = 2
vin key Down key Return
settle 0.4
check "Enter opened hub.md at the line" test "$(q '.tabs[] | select(.active) | .path'):$(q .cursor.line)" = "hub.md:3"
