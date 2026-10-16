#!/usr/bin/env bash
# Runs nib for an agent: headless cage compositor, private D-Bus session,
# isolated XDG directories under ~/.cache/nib-agent, --profile agent, and a
# fresh copy of the fixture vault. Nothing reaches the owner's monitors, bus,
# config, trash or notes (D11). Every write is confined by NIB_ALLOW_ROOTS.
#
#   scripts/agent/agent-run.sh [options] [-- nib arguments] < driver.sh
#
# The driver is a bash script read from stdin, run inside the session once nib
# has painted its first frame. Helpers:
#   VAULT                the vault copy nib was opened on
#   shot NAME            screenshot to $OUT/NAME.png
#   vin ...              keyboard and pointer input (scripts/agent/vinput.c)
#   run NAME [ARG]       run a registry entry over D-Bus (debug builds)
#   dump                 print nib's --test-dump JSON
#   q FILTER             jq FILTER over a fresh dump, raw output
#   wait_log REGEX [S]   wait until nib's log matches (default 10 s)
#   wait_q FILTER VALUE [S]  wait until `q FILTER` prints VALUE
#   settle [S]           sleep (default 0.4)
#   nib ARGS...          a further invocation with the agent profile
#   check DESC CMD...    print PASS or FAIL; any FAIL fails the run
#   has TEXT REGEX       true when TEXT matches
#   bytes FILE           sha256 of a file in the vault copy
# With no driver nib runs until --timeout.
#
# Options:
#   --size WxH     output size (default 1280x800)
#   --release      use the release build
#   --env K=V      set a variable for nib only (repeatable)
#   --keep         keep the agent profile's config and state from the last run
#   --config TEXT  write TEXT as the agent profile's nib.toml first
#   --vault DIR    open DIR instead of a fixture copy (must be under the agent home)
#   --no-copy      reuse the existing vault copy
#   --out DIR      artefact directory (default target/agent/out)
#   --timeout S    hard limit for the whole session (default 90)
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
agent_home="${XDG_CACHE_HOME:-$HOME/.cache}/nib-agent"
fixture="${XDG_CACHE_HOME:-$HOME/.cache}/nib-fixture"

if [[ ${1:-} != --inside ]]; then
    size=1280x800 build=debug keep=0 out="$root/target/agent/out" limit=90 config="" vault="" copy=1
    app_env=()
    while [[ $# -gt 0 ]]; do
        case $1 in
            --size) size=$2; shift 2 ;;
            --release) build=release; shift ;;
            --env) app_env+=("$2"); shift 2 ;;
            --keep) keep=1; shift ;;
            --config) config=$2; shift 2 ;;
            --vault) vault=$2; shift 2 ;;
            --no-copy) copy=0; shift ;;
            --out) out=$2; shift 2 ;;
            --timeout) limit=$2; shift 2 ;;
            --) shift; break ;;
            *) echo "agent-run.sh: unknown option $1" >&2; exit 2 ;;
        esac
    done
    for tool in cage grim dbus-run-session gdbus wlr-randr jq; do
        command -v "$tool" >/dev/null || { echo "agent-run.sh: $tool is not installed" >&2; exit 2; }
    done
    "$root/scripts/agent/build-vinput.sh"
    if [[ $build == release ]]; then cargo build --release -q -p nib; else cargo build -q -p nib; fi
    [[ -f $fixture/.fixture-marker ]] || "$root/scripts/mkfixture.sh" --seed 7 >/dev/null
    mkdir -p "$out" "$agent_home"/{config,cache,data,state}
    if [[ $keep == 0 ]]; then
        rm -rf "$agent_home/config/nib/agent" "$agent_home/state/nib/agent"
    fi
    if [[ -n $config ]]; then
        mkdir -p "$agent_home/config/nib/agent"
        printf '%b' "$config" >"$agent_home/config/nib/agent/nib.toml"
    fi
    if [[ -z $vault ]]; then
        vault="$agent_home/vault"
        if [[ $copy == 1 ]]; then
            mkdir -p "$vault"
            rsync -a --delete "$fixture/" "$vault/"
        fi
    fi
    driver="$out/driver.sh"
    if [[ -t 0 ]]; then : >"$driver"; else cat >"$driver"; fi
    unset WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE NIB_GTK_THEME GTK_THEME
    export XDG_CONFIG_HOME="$agent_home/config" XDG_CACHE_HOME="$agent_home/cache"
    export XDG_DATA_HOME="$agent_home/data" XDG_STATE_HOME="$agent_home/state"
    export NIB_ALLOW_ROOTS="$agent_home${NIB_ALLOW_ROOTS:+:$NIB_ALLOW_ROOTS}"
    export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 GTK_A11Y=none NO_AT_BRIDGE=1
    export AGENT_SIZE=$size AGENT_BUILD=$build OUT=$out VAULT=$vault ROOT=$root
    export AGENT_DRIVER=$driver AGENT_APP_ENV="${app_env[*]}"
    export NIB_VINPUT="$root/target/agent/vinput"
    exec timeout --kill-after=5 "$limit" dbus-run-session -- \
        cage -- "$root/scripts/agent/agent-run.sh" --inside "$@" 2>"$out/session.log"
fi

# ---- inside the headless session ----
shift
wlr-randr --output HEADLESS-1 --custom-mode "$AGENT_SIZE" >/dev/null
export VINPUT_EXTENT=$AGENT_SIZE
app_id=io.github.codedsleep.Nib.p_agent
app_path=/io/github/codedsleep/Nib/p_agent
log="$OUT/nib.log"
bin="$ROOT/target/$AGENT_BUILD/nib"

shot() { grim "$OUT/$1.png"; }
vin() { "$NIB_VINPUT" "$@"; }
run() {
    local param="$1${2:+ $2}"
    gdbus call --session --dest "$app_id" --object-path "$app_path/window/1" \
        --method org.gtk.Actions.Activate run "[<'$param'>]" "{}" >/dev/null
}
nib() { "$bin" --profile agent "$@"; }
dump() { "$bin" --profile agent --test-dump; }
q() { dump | jq -r "$1"; }
FAILED=0
check() {
    local description=$1
    shift
    if "$@"; then echo "PASS $description"; else echo "FAIL $description"; FAILED=1; fi
}
has() { grep -Eq -- "$2" <<<"$1"; }
settle() { sleep "${1:-0.4}"; }
bytes() { sha256sum "$VAULT/$1" | cut -d' ' -f1; }
wait_log() {
    local deadline=$((SECONDS + ${2:-10}))
    until grep -Eq "$1" "$log" 2>/dev/null; do
        if ((SECONDS >= deadline)); then echo "wait_log: timed out on $1" >&2; return 1; fi
        sleep 0.05
    done
}
wait_q() {
    local deadline=$((SECONDS + ${3:-10})) got
    while true; do
        got=$(q "$1" 2>/dev/null || true)
        [[ $got == "$2" ]] && return 0
        if ((SECONDS >= deadline)); then echo "wait_q: $1 is '$got', wanted '$2'" >&2; return 1; fi
        sleep 0.1
    done
}

: >"$log"
# shellcheck disable=SC2086
env $AGENT_APP_ENV NIB_TRACE=1 NIB_T0="$(date +%s%N)" "$bin" --profile agent "$@" >>"$log" 2>&1 &
app_pid=$!
status=0
if [[ -s $AGENT_DRIVER ]]; then
    wait_log '^nib ready' 20 || true
    set +e
    # shellcheck disable=SC1090
    (set -e; source "$AGENT_DRIVER"; exit "$FAILED")
    status=$?
    set -e
else
    wait "$app_pid" || status=$?
fi
if kill -0 "$app_pid" 2>/dev/null; then
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
else
    app_status=0
    wait "$app_pid" || app_status=$?
    [[ $status != 0 ]] || status=$app_status
fi
exit "$status"
