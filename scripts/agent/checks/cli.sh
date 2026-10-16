# cli — every invocation form against the running window (9.9): a second
# `nib note.md` opens a tab in the first window, --new and --daily act
# there, a folder shows in the column, another vault is refused with the
# --profile hint, and --version/--help exit 0 without a window.
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note hub.md
n=$(q '.tabs | length')
nib "$VAULT/linker11.md" >/dev/null
settle 0.4
check "a second nib note.md opened a tab in this window" test "$(q '.tabs | length'):$(q '.tabs[] | select(.active) | .path')" = "$((n + 1)):linker11.md"
nib --new "From the command line" >/dev/null
settle 0.4
check "--new created and opened the note" test -f "$VAULT/From the command line.md"
nib --daily >/dev/null
settle 0.4
check "--daily opened today's note" test "$(q '.tabs[] | select(.active) | .path')" = "daily/$(date +%Y-%m-%d).md"
nib "$VAULT/folder07" >/dev/null
settle 0.4
check "a folder shows in the files column" test "$(q .files.folder)" = folder07
mkdir -p "$AGENT_HOME/other-vault"
nib --vault "$AGENT_HOME/other-vault" >/dev/null
settle 0.3
check "another vault is refused with the profile hint" has "$(status_right)" 'nib --profile NAME'
check "--version" bash -c "'$ROOT/target/debug/nib' --version | grep -q '^nib '"
check "--help" bash -c "'$ROOT/target/debug/nib' --help | grep -q Usage"
check "the desktop file registers text/markdown" grep -q '^MimeType=text/markdown;' "$ROOT/data/io.github.codedsleep.Nib.desktop"
