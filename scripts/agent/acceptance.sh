#!/usr/bin/env bash
# Run only on an isolated, committed acceptance copy, never on a live vault.
set -euo pipefail
cd "$(dirname "$0")/../.."
vault=$(realpath "${1:?usage: acceptance.sh ACCEPTANCE_COPY}")
case "$vault" in
    "${HOME}/.cache/nib-acceptance/"*) ;;
    *) echo 'acceptance: use a copy below ~/.cache/nib-acceptance/' >&2; exit 2 ;;
esac
[[ -d $vault/.git && ! -e $vault/nib-phase10-check ]] || {
    echo 'acceptance: needs a committed copy with no nib-phase10-check folder' >&2; exit 2;
}
out="$PWD/target/agent/acceptance"
mkdir -p "$out"
git -C "$vault" status --porcelain --untracked-files=all >"$out/status-before.txt"
[[ ! -s $out/status-before.txt ]] || { echo 'acceptance: copy has uncommitted changes' >&2; exit 2; }
manifest() {
    python3 - "$vault" <<'PY'
import hashlib, json, os, pathlib, sys
root = pathlib.Path(sys.argv[1])
result = {}
for folder, dirs, files in os.walk(root, followlinks=False):
    dirs[:] = [d for d in dirs if d != '.git']
    for name in dirs + files:
        p = pathlib.Path(folder) / name
        if p.is_symlink():
            value = ['link', os.readlink(p)]
        elif p.is_file():
            value = ['file', p.stat().st_mode, hashlib.sha256(p.read_bytes()).hexdigest()]
        else:
            value = ['directory', p.stat().st_mode]
        result[str(p.relative_to(root))] = value
print(json.dumps(result, sort_keys=True))
PY
}
manifest >"$out/manifest-before.json"
mkdir "$vault/nib-phase10-check"
printf '# Acceptance hub\n## Heading\n' >"$vault/nib-phase10-check/hub.md"
for n in $(seq 0 13); do
    printf '# Linker %s\n[[nib-phase10-check/hub#Heading|alias %s]]\n' "$n" "$n" >"$vault/nib-phase10-check/linker$n.md"
done
NIB_ALLOW_ROOTS="$vault" scripts/agent/agent-run.sh --vault "$vault" --out "$out" \
    --config '[vault]\nattachments = "nib-phase10-check/images"\n' -- "$vault" <<'DRIVER'
source "$ROOT/scripts/agent/checks/lib.sh"
wait_q .index_ready true 15
open_note nib-phase10-check/linker3.md
focus_editor
vin key G key o type 'acceptance edit' key Escape
run save
settle 0.3
check 'edited note saved in the acceptance copy' rg -q 'acceptance edit' "$VAULT/nib-phase10-check/linker3.md"
open_note nib-phase10-check/hub.md
run rename renamed.md
settle 0.4
check 'rename asks to rewrite fourteen links' test "$(q .dialog)" = 'Update 14 links in 14 notes?'
vin type y
settle 1.0
check 'renamed note exists' test -f "$VAULT/nib-phase10-check/renamed.md"
check 'fourteen links preserve their aliases and headings' test "$(rg -l -F '[[nib-phase10-check/renamed#Heading|alias ' "$VAULT"/nib-phase10-check/linker*.md | wc -l)" = 14
open_note nib-phase10-check/linker3.md
focus_editor
vin key u
settle 0.2
check 'rewritten open buffer retains undo' test "$(q '.tabs[] | select(.active) | .modified')" = true
run reload-note
settle 0.2
"$NIB_VINPUT" sleep 60000 >/dev/null 2>&1 &
keyboard=$!
wl-copy --foreground -t image/png <"$HOME/.cache/nib-fixture/attachments/image01.png" >/dev/null 2>&1 &
copier=$!
settle 0.4
vin key G key o key ctrl+v
settle 0.6
vin key Escape
run save
settle 0.2
check 'pasted image inserted an embed' rg -q '!\[\[Pasted image [0-9]{14}\.png\]\]' "$VAULT/nib-phase10-check/linker3.md"
vin key alt+e
settle 0.8
check 'pasted image renders' test "$(q .reading.images)" -ge 1
kill "$keyboard" "$copier" 2>/dev/null || true
run quit
settle 0.5
DRIVER
git -C "$vault" status --porcelain --untracked-files=all >"$out/status-after.txt"
manifest >"$out/manifest-after.json"
python3 - "$vault" "$out" <<'PY'
import json, pathlib, subprocess, sys
vault = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
before = json.loads((out / 'manifest-before.json').read_text())
after = json.loads((out / 'manifest-after.json').read_text())
assert all(after.get(p) == value for p, value in before.items()), 'an existing vault entry changed'
assert all(p.startswith('nib-phase10-check') for p in after.keys() - before.keys()), 'unexpected vault entry'
changed = subprocess.check_output(['git', '-C', str(vault), 'status', '--porcelain', '-z', '--untracked-files=all']).split(b'\0')
paths = [row[3:].decode() for row in changed if row]
assert len(paths) == 16, f'expected 14 linkers, renamed hub and pasted image; got {len(paths)}'
assert all(p.startswith('nib-phase10-check/') for p in paths), 'unexpected change outside the test notes'
assert not subprocess.check_output(['git', '-C', str(vault), 'diff', '--name-only', '--', '.obsidian']), '.obsidian changed'
print('PASS acceptance manifest: only the 16 requested notes and attachment changed; existing vault files untouched')
PY
