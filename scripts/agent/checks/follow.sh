# follow — Alt+Enter, :follow and Ctrl+click (6.4, D10): an executable or
# .desktop attachment is refused and nothing is started; file: and
# javascript: links are refused; a wikilink opens its note at the heading
# (a missing heading gives a notice); an unresolved link creates its note.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note hub.md
focus_editor
vin key G key o type "[[attachments/evil.sh]]" key Return type "[[attachments/launcher.desktop]]" \
    key Return type "[f](file:///etc/passwd)" key Return type "[j](javascript:alert)" \
    key Return type "[[linker00#Linker 0]]" key Return type "[[hub#Nope]]" \
    key Return type "[[Brand new note]]" key Escape
vin type :w key Return
settle 0.4
procs_before=$(pgrep -c -P "$app_pid" || true)
# Put the cursor on a link with vim's search.
find() { vin key Escape type "/$1" key Return; settle 0.2; }
find evil
vin key alt+Return
settle 0.3
check "evil.sh refused" has "$(status_right)" 'refused to run evil.sh'
find launcher
vin type :follow key Return
settle 0.3
check "launcher.desktop refused" has "$(status_right)" 'refused to run launcher.desktop'
check "nothing was started" test "$(pgrep -c -P "$app_pid" || true)" = "$procs_before"
check "no evil process" bash -c "! ps -eo args | grep -F attachments/evil.sh | grep -v -e grep -e vinput"
find etc.passwd
vin key alt+Return
settle 0.3
check "file: link refused" has "$(status_right)" 'refused to open file:'
find javascript
vin key alt+Return
settle 0.3
check "javascript: link refused" has "$(status_right)" 'refused to open javascript:'
find Nope
vin key alt+Return
settle 0.3
check "missing heading: notice, note still opens" has "$(status_right)" 'no heading #nope'
open_note hub.md
find "linker00#"
vin key alt+Return
settle 0.4
check "wikilink opened linker00" test "$(q '.tabs[] | select(.active) | .path')" = linker00.md
open_note hub.md
find Brand
vin key alt+Return
settle 0.5
check "unresolved link created its note" test -f "$VAULT/Brand new note.md"
check "and opened it" test "$(q '.tabs[] | select(.active) | .path')" = "Brand new note.md"
# Ctrl+click on the link on the second line of linker00.md.
open_note linker00.md
vin mods ctrl move 300 102 click mods none
settle 0.5
check "Ctrl+click followed the link" test "$(q '.tabs[] | select(.active) | .path')" = hub.md
