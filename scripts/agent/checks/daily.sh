# daily — daily notes and templates (9.6): Alt+D creates today's note from
# the daily folder and format, :daily ±N and a date, :new, and a template
# inserted from the finder with its variables expanded.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
today=$(date +%Y-%m-%d)
vin key alt+d
settle 0.5
check "Alt+D created daily/$today.md" test -f "$VAULT/daily/$today.md"
check "and opened it" test "$(q '.tabs[] | select(.active) | .path')" = "daily/$today.md"
before=$(sha256sum "$VAULT/daily/2026-09-01.md")
vin type ":daily 2026-09-01" key Return
settle 0.4
check ":daily DATE opens an existing note unchanged" test "$(sha256sum "$VAULT/daily/2026-09-01.md")" = "$before"
vin type ":daily +1" key Return
settle 0.4
check ":daily +1" test -f "$VAULT/daily/$(date -d tomorrow +%Y-%m-%d).md"
vin type ":new Fresh thought" key Return
settle 0.4
check ":new created the note" test -f "$VAULT/Fresh thought.md"
focus_editor
vin key alt+T
settle 0.4
check "Alt+Shift+T lists templates" test "$(q .finder.source)" = templates
vin type note key Return
settle 0.4
vin type :w key Return
settle 0.4
check "the template filled the empty note" test -s "$VAULT/Fresh thought.md"
check "no template placeholder left unexpanded" bash -c "! grep -qE '\{\{(date|time|title)' '$VAULT/Fresh thought.md'"
cat "$VAULT/templates/note.md" | head -3
