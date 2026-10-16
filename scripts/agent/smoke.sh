#!/usr/bin/env bash
# check.sh's window smoke: a release nib must map its window on the fixture in
# the headless session. Prints start_to_map for the first start (which also
# pays for the private session's portal activation) and for a warm second
# start (its own profile, so a separate instance) — the comparable number.
# Fails only if a window never maps (D7).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
if ! command -v cage >/dev/null; then
    echo "smoke: cage is not installed; window smoke skipped" >&2
    exit 0
fi
out="$root/target/agent/smoke"
"$root/scripts/agent/agent-run.sh" --release --out "$out" --timeout 60 -- \
    "${XDG_CACHE_HOME:-$HOME/.cache}/nib-agent/vault/hub.md" <<'DRIVER'
grep -q '^nib ready' "$OUT/nib.log" || { echo "smoke: the window never mapped" >&2; exit 1; }
for i in 1 2 3; do
    NIB_TRACE=1 NIB_T0="$(date +%s%N)" "$bin" --profile warm "$VAULT/hub.md" >"$OUT/warm.log" 2>&1 &
    pid=$!
    for _ in $(seq 100); do grep -q '^nib ready' "$OUT/warm.log" && break; sleep 0.05; done
    grep -h '^name=start_to_map' "$OUT/warm.log" | sed 's/^/warm /' >>"$OUT/warm-times"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
done
DRIVER
cold=$(grep -m1 '^name=start_to_map' "$out/nib.log" | sed 's/.*ms=//')
warm=$(sed 's/.*ms=//' "$out/warm-times" | sort -n | sed -n 2p)
rm -f "$out/warm-times"
[[ -n $cold && -n $warm ]] || { echo "smoke: the window never mapped" >&2; exit 1; }
echo "window smoke: mapped; start_to_map cold=${cold} ms, warm median=${warm} ms (ws target ≤ 90 ms)"
