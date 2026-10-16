# paste — a screenshot on the clipboard (wl-copy < png) pasted with Ctrl+V in
# insert mode becomes "Pasted image YYYYMMDDHHMMSS.png" in attachments/ with
# ![[…]] at the cursor (9.7, G2); Ctrl+V in normal mode stays vim's; a
# dropped/attached vault note is linked without copying.
source "$ROOT/scripts/agent/checks/lib.sh"
"$NIB_VINPUT" sleep 60000 >/dev/null 2>&1 &
keyboard=$!
settle 0.3
wl-copy --foreground -t image/png <"$VAULT/attachments/image01.png" >/dev/null 2>&1 &
copier=$!
settle 0.5
open_note linker10.md
focus_editor
vin key ctrl+v
settle 0.4
check "Ctrl+V in normal mode pasted nothing (it is vim's visual block)" test -z "$(find "$VAULT/attachments" -name 'Pasted image *')"
check "vim got the key" has "$(q .status.mode)" 'VISUAL'
vin key Escape key G key o key ctrl+v
settle 1.0
file=$(find "$VAULT/attachments" -name 'Pasted image *.png' | head -1)
check "a pasted image file exists" test -n "$file"
check "named like Obsidian" has "$(basename "$file")" '^Pasted image [0-9]{14}\.png$'
vin key Escape type :w key Return
settle 0.4
check "![[…]] inserted at the cursor" grep -qF "![[$(basename "$file")]]" "$VAULT/linker10.md"
vin key alt+e
settle 0.8
check "the pasted image renders in the reading view" test "$(q .reading.images)" -ge 1
vin key alt+e
vin type :paste-image key Return
settle 1.0
check ":paste-image works in normal mode too" test "$(find "$VAULT/attachments" -name 'Pasted image *' | wc -l)" -ge 1
kill "$keyboard" "$copier" 2>/dev/null || true
