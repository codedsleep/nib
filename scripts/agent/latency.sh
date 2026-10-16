#!/usr/bin/env bash
# Frame timing measured from outside, trek's method: nib runs with
# WAYLAND_DEBUG=1 in the headless cage session and scripts/agent/latency.awk
# reads, from nib's own protocol log, how long after each key press it
# committed a frame. Release build.
#
#   scripts/agent/latency.sh             files column over a 20 000-entry folder (5.5)
#
# Prints one row per scenario: key presses; key-to-commit median and p95
# (this includes waiting for the 60 Hz frame callback, so it can pass 16 ms
# without a dropped frame); and the frame work GTK did per frame
# (before-paint to after-paint, NIB_FRAMES=1): median, worst, and frames over
# 16 ms. Rows land in target/agent/latency.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/target/agent/latency"
big="${XDG_CACHE_HOME:-$HOME/.cache}/nib-agent/latency-vault"
if [[ ! -f $big/big/note19999.md ]]; then
    mkdir -p "$big/big"
    (cd "$big/big" && seq -f 'note%05g.md' 0 19999 | xargs touch)
fi
rm -rf "$out"
"$root/scripts/agent/agent-run.sh" --release --vault "$big/big" --out "$out" --timeout 300 \
    --env WAYLAND_DEBUG=1 --env NIB_FRAMES=1 -- "$big/big" <<'DRIVER'
log="$OUT/nib.log"
mark() { printf 'MARK %s\n' "$*" >>"$log"; }
# keys LABEL COUNT SPACING KEY...: the keys COUNT times over, one every SPACING ms.
keys() {
    local label=$1 count=$2 gap=$(($3 - 40)) args=(sleep 300) key
    shift 3
    for ((n = 0; n < count; n++)); do
        for key in "$@"; do args+=(key "$key" sleep "$gap"); done
    done
    mark "$label"
    vin "${args[@]}"
    sleep 0.5
    mark
}
settle 1.5
vin key alt+1
settle 0.5
keys files.down 40 150 j
keys files.held 100 40 j
keys files.page 15 200 ctrl+f
keys files.bottom-top 6 400 G
keys files.top 6 400 g g
keys files.half 20 150 ctrl+d
DRIVER
awk -f "$root/scripts/agent/latency.awk" "$out/nib.log" | grep -v -P '^-\t' >"$out/rows.tsv"
# Frame work per scenario, from the MARK lines the driver wrote into the log.
awk '
/^MARK / { label = $2; next }
/^frame ms=/ && label != "" { sub(/^frame ms=/, ""); print label "\t" $0 }
' "$out/nib.log" >"$out/frames.tsv"
awk -F'\t' '
function at(list, n, share,    i) { i = int((n - 1) * share + 1.5); return list[i > n ? n : i] }
FNR == 1 { file++ }
file == 1 {
    if (!($1 in seen)) { seen[$1] = 1; order[++scenarios] = $1 }
    n[$1]++
    if ($4 == 0) next
    d[$1]++; first[$1][d[$1]] = $2 + 0
    next
}
{ f[$1]++; work[$1][f[$1]] = $2 + 0; if ($2 + 0 > 16) over[$1]++ }
END {
    printf "%-16s %5s %10s %8s %10s %8s %8s\n", "scenario", "keys", "commit med", "p95", "frame med", "worst", ">16 ms"
    for (s = 1; s <= scenarios; s++) {
        k = order[s]; m = d[k] + 0; w = f[k] + 0
        if (m) asort(first[k])
        if (w) asort(work[k])
        printf "%-16s %5d %10.1f %8.1f %10.1f %8.1f %8d\n", k, n[k], m ? at(first[k], m, 0.5) : 0,
            m ? at(first[k], m, 0.95) : 0, w ? at(work[k], w, 0.5) : 0, w ? work[k][w] : 0, over[k] + 0
    }
}' "$out/rows.tsv" "$out/frames.tsv"
