# trash — d moves to the XDG trash (D9): a note, a selection, a still-linked
# attachment (trashed with a notice giving the count); nothing is deleted.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
trash="$XDG_DATA_HOME/Trash/files"
focus_files
vin type /crlf key Return type d
settle 0.6
check "crlf.md left the vault" test ! -e "$VAULT/crlf.md"
check "crlf.md is in the trash" test -f "$trash/crlf.md"
vin type ga
settle 0.3
vin type /image00 key Return type d
settle 0.6
check "image00.png trashed" test -f "$trash/image00.png"
check "with a notice that it is still linked" has "$(status_right)" 'image00.png is still linked from [0-9]+ note'
vin key g key g key space key space type d
settle 0.6
check "a selection of two trashed together" has "$(status_right)" 'trashed 2 items'
check "nothing under .trash/ in the vault" test ! -e "$VAULT/.trash"
