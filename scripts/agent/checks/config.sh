# config — :config opens nib.toml in a tab; saving it applies changes live
# (9.8): style_scheme recolours the editor and the reading view, wrap and
# line numbers follow, warnings go to the status line.
source "$ROOT/scripts/agent/checks/lib.sh"
open_note folder00/note00000.md
vin key alt+e
settle 0.4
dark=$(q .reading.heading)
check "starts on nib-nord" test "$(q .reading.scheme)" = nib-nord
vin key alt+e
focus_editor
vin type :config key Return
settle 0.4
check ":config opened nib.toml" test "$(q '.tabs[] | select(.active) | .title')" = nib.toml
vin type /nib-nord key Return type 'ct"' type nib-light key Escape type :w key Return
settle 0.5
check "the file says nib-light" grep -q 'style_scheme = "nib-light"' "$AGENT_HOME/config/nib/agent/nib.toml"
check "reloaded" has "$(status_right)" 'nib.toml reloaded'
vin type ":tab 1" key Return
settle 0.3
check "the editor took the new scheme" test "$(q .reading.scheme)" = nib-light
check "the reading view's heading colour changed" test "$(q .reading.heading)" != "$dark"
vin type ":tab 2" key Return type gg type /wat key Return
vin key o type "bogus_key = 1" key Escape type :w key Return
settle 0.4
check "an unknown key is a warning in the status line" has "$(status_right)" '^nib.toml: '
