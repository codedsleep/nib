#!/usr/bin/env bash
# Runs saved UI checks in the headless agent session, one line per result.
#
#   scripts/agent/checks/run-all.sh NAME...    # e.g. roundtrip wq
#   scripts/agent/checks/run-all.sh --all      # every check (slow; Goal C)
#
# Exit status 1 if any check failed or a session reported a toolkit warning.
set -uo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
checks="$root/scripts/agent/checks"
run="$root/scripts/agent/agent-run.sh"
vault="${XDG_CACHE_HOME:-$HOME/.cache}/nib-agent/vault"
artifacts="$root/target/agent/checks"

if [[ $# -eq 0 ]]; then
    echo "run-all.sh: name the checks to run, or --all" >&2
    exit 2
fi
if [[ $1 == --all ]]; then
    mapfile -t names < <(find "$checks" -name '*.sh' ! -name run-all.sh ! -name lib.sh -printf '%f\n' | sed 's/\.sh$//' | sort)
else
    names=("$@")
fi

session() {
    local name=$1 out=$2 status=0
    shift 2
    "$run" --out "$out" --env G_DEBUG=fatal-criticals "$@" <"$checks/$name.sh" || status=$?
    if [[ $status != 0 ]]; then echo "FAIL session exited with status $status (logs: $out)"; fi
    # One GTK warning is ignored: replacing a buffer makes GtkTextView revalidate
    # lines inside its own snapshot, which moves the adjustment and leaves the
    # scrollbar slider unallocated for that frame (GTK 4.22; harmless).
    if grep -En '(Gtk|Gdk|GLib|GObject|GtkSourceView)(-[[:alnum:]]+)*-(WARNING|CRITICAL)|assertion .+ failed' \
        "$out/nib.log" | grep -v 'Trying to snapshot GtkGizmo .* without a current allocation' \
        >"$out/toolkit-diagnostics.log"; then
        echo "FAIL toolkit warning or critical (logs: $out/toolkit-diagnostics.log)"
        cat "$out/toolkit-diagnostics.log"
        status=1
    fi
    return "$status"
}

one() {
    local name=$1 out="$artifacts/$1"
    case $name in
        listcont)
            session "$name" "$out-en" --env LANG=en_GB.UTF-8 -- "$vault" || return 1
            # de_DE is not installed here; LANGUAGE still selects German strings.
            session "$name" "$out-de" \
                --env LANG=en_GB.UTF-8 --env LANGUAGE=de -- "$vault"
            ;;
        complete)
            session "$name" "$out" -- "$vault" || return 1
            NOT_READY=1 session "$name" "$out-not-ready" --env NIB_TEST_INDEX_DELAY_MS=20000 -- "$vault"
            ;;
        *) session "$name" "$out" -- "$vault" ;;
    esac
}

failed=0
for name in "${names[@]}"; do
    [[ -f $checks/$name.sh ]] || { echo "run-all.sh: no check $name" >&2; failed=1; continue; }
    echo "== $name"
    result=$(one "$name" 2>&1)
    printf '%s\n' "$result" | grep -E '^(PASS|FAIL)' || true
    if ! grep -q '^PASS' <<<"$result" || grep -q '^FAIL' <<<"$result"; then
        failed=1
        printf '%s\n' "$result" | grep -Ev '^(PASS|FAIL)' | tail -20
    fi
done
exit "$failed"
