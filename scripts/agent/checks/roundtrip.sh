# roundtrip — open and save every edge file of the fixture: the bytes on disk
# stay identical (CRLF, BOM, mixed endings, no final newline, a symlinked
# note), and a non-UTF-8 note opens read-only and is never written (S3, 5.8).
source "$ROOT/scripts/agent/checks/lib.sh"
for f in crlf.md bom.md mixed-eol.md no-trailing-newline.md linked-symlink.md; do
    before=$(bytes "$f")
    open_note "$f"
    focus_editor
    vin key ctrl+s
    settle 0.3
    check "$f: saved with no edit, bytes identical" same_bytes "$f" "$before"
    # An edit that is undone by hand: the remembered format is used again.
    vin key End type x key BackSpace key ctrl+s
    settle 0.3
    check "$f: typed and erased, bytes identical" same_bytes "$f" "$before"
done
check "linked-symlink.md is still a symlink" test -L "$VAULT/linked-symlink.md"
before=$(bytes latin1.md)
open_note latin1.md
check "latin1.md opens read-only" test "$(q '.tabs[] | select(.active) | .read_only')" = true
focus_editor
vin key ctrl+s
settle 0.3
check "latin1.md: save refused with a notice" has "$(status_right)" 'read-only'
check "latin1.md: bytes untouched" same_bytes latin1.md "$before"
check "no temporary files left" test -z "$(find "$VAULT" -name '*.nib-tmp')"
