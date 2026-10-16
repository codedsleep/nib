# reading — the reading view (8.1-8.4, 8.6): frontmatter line that expands
# on click, task glyphs, callouts, headings in the scheme's colour, 20
# vault images decoded and a remote one shown as its link text, embeds at
# depth one with a title line, image files in a reading tab, the reading
# keys, Tab/Enter following a link, i back to source, and the split view
# re-rendering after edits.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
{
    for i in $(seq -w 0 19); do printf '![[image%s.png]]\n' "$i"; done
    printf '\n![remote](https://example.com/remote.png)\n'
} >"$VAULT/gallery.md"
open_note folder00/note00000.md
vin key alt+e
settle 0.6
check "Alt+E shows the reading view" test "$(q .reading.mode)" = reading
check "the reading view has the keyboard" test "$(q .focus)" = reading
text=$(q .reading.text)
check "frontmatter is one dim line" has "$text" '^— 4 properties —'
check "tasks are glyphs" has "$text" '☐ task' && has "$text" '☑ done'
check "a callout keeps its title" has "$text" 'note Remember'
check "markup is gone" bash -c "! grep -q '\*\*bold\*\*' <<<'$text'"
shot reading-note
vin move 280 65 click
settle 0.4
check "clicking the frontmatter line shows the raw block" has "$(q .reading.text)" 'opaque: keep me unchanged'
vin key G
settle 0.2
bottom=$(q .reading.scroll)
check "G scrolls to the end" test "$bottom" -gt 0
vin key g key g
settle 0.2
check "gg back to the top" test "$(q .reading.scroll)" = 0
vin key j key j
settle 0.2
check "j scrolls down" test "$(q .reading.scroll)" -gt 0
vin key Tab key Return
settle 0.5
check "Tab then Enter followed the first link" test "$(q '.tabs[] | select(.active) | .path')" = folder01/note00001.md
check "the opened note shows in reading mode too" test "$(q .reading.mode)" = reading
vin key i
settle 0.3
check "i returns to the source" test "$(q .reading.mode):$(q .focus)" = source:editor
open_note gallery.md
vin key alt+e
settle 1.5
check "20 vault images decoded" test "$(q .reading.images):$(q .reading.waiting)" = 20:0
check "the remote image is its link text" has "$(q .reading.text)" 'https://example.com/remote.png'
vin key alt+e
open_note embeds.md
vin key alt+e
settle 0.6
text=$(q .reading.text)
check "three embeds, each with a title line" test "$(grep -c '^↳' <<<"$text")" = 3
check "the heading embed shows that section" has "$text" '↳ Note 0'$'\n''Detail'
vin key alt+e
focus_files
vin type ga type /image03 key Return type l
settle 0.8
check "an image file opens in a reading tab" test "$(q .reading.mode):$(q .reading.images)" = reading:1
open_note linker05.md
focus_editor
vin key alt+E
settle 0.6
check "Alt+Shift+E splits" test "$(q .reading.mode):$(q .focus)" = split:editor
vin key G key o type "Split edit marker" key Escape
settle 1.0
check "the reading side re-rendered after 400 ms" has "$(q .reading.text)" 'Split edit marker'
