#!/usr/bin/env bash
# Run every TLA+ model and check its verdict against the expected red/green oracle.
# Each invocation gets private TLC state and output directories. Exits non-zero
# if any verdict does not match, including a worker that fails to run TLC.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

# The runs are discovered, not listed: every top-level *.cfg here is a run of
# <module> expecting <verdict>, one of GREEN (no error), RED (an invariant is
# violated), or, for a recorded trace replay, TRACE:<depth> or DEADLOCK:<l>.
# A trace replay walks one path, so its verdict also says where it ends:
# TRACE:<depth> finds no error from exactly one initial state and walks every
# event to the settled rest (depth = events + 1), and DEADLOCK:<l> stops with
# event l next (past the end when l = events + 1).
#
# The module and verdict come from one place, the cfg's first line:
#     \* run-all: <Module>.tla <GREEN|RED|TRACE:<n>|DEADLOCK:<n>>
# The cfg's name says nothing to this script. A cfg whose first line is not
# such a header is reported and fails the whole run; so is a header naming a
# module that does not exist or a verdict that is not one of these.
# --list prints the discovered `cfg module verdict` rows and nothing else, and
# exits 2 when any cfg did not resolve to a module and verdict.

list_only=
[ "${1:-}" = "--list" ] && list_only=1

unresolved=0
RUNS=()
for cfg in *.cfg; do
  [ -f "$cfg" ] || continue
  mod='' ; want=''
  # The first line, '\* run-all: <Module>.tla <verdict>', names both.
  header=$(sed -n '1s/^\\\* run-all: \([^ ][^ ]*\) \([^ ][^ ]*\)[[:space:]]*$/\1 \2/p' "$cfg")
  if [ -n "$header" ]; then
    want=${header##* }
    mod=${header% *}
  fi
  case "$want" in
    GREEN|RED) ;;
    TRACE:[0-9]*|DEADLOCK:[0-9]*) ;;
    *) want='' ;;
  esac
  if [ -z "$mod" ] || [ -z "$want" ] || [ ! -f "$mod" ]; then
    echo "run-all: $cfg: its first line must be '\* run-all: <Module>.tla <GREEN|RED|TRACE:<n>|DEADLOCK:<n>>', naming a module that exists" >&2
    unresolved=$((unresolved+1))
    continue
  fi
  RUNS+=("$cfg $mod $want")
done

if [ "$unresolved" -gt 0 ]; then exit 2; fi
if [ "${#RUNS[@]}" -eq 0 ]; then
  echo "run-all: no *.cfg found" >&2
  exit 2
fi
if [ -n "$list_only" ]; then
  printf '%s\n' "${RUNS[@]}"
  exit 0
fi

# Each TLC run checks with one worker, so one lane per core keeps every core
# busy; TLA_JOBS overrides it.
parallelism="${TLA_JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"
case "$parallelism" in
  ''|*[!0-9]*) echo "TLA_JOBS must be a positive integer" >&2; exit 2 ;;
esac
parallelism=$((10#$parallelism))
if [ "$parallelism" -lt 1 ]; then
  echo "TLA_JOBS must be a positive integer" >&2
  exit 2
fi
# Half of the machine's memory, shared out between the lanes. When the
# default lane count would leave a lane under 512 MB, fewer lanes run.
if [ -z "${TLA_JAVA_HEAP:-}" ]; then
  if [ -r /proc/meminfo ]; then
    memory_mb=$(awk '/^MemTotal:/ {print int($2 / 1024)}' /proc/meminfo)
  else
    memory_mb=$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1048576 ))
  fi
  if [ "${memory_mb:-0}" -gt 0 ]; then
    budget_mb=$(( memory_mb / 2 ))
    if [ -z "${TLA_JOBS:-}" ] && [ $(( budget_mb / parallelism )) -lt 512 ]; then
      parallelism=$(( budget_mb / 512 ))
      [ "$parallelism" -lt 1 ] && parallelism=1
    fi
    export TLA_JAVA_HEAP="$(( budget_mb / parallelism ))m"
  fi
fi
if ! workdir=$(mktemp -d "${TMPDIR:-/tmp}/defra-tla.XXXXXX"); then
  echo "Could not create TLA scratch directory" >&2
  exit 1
fi
trap 'rm -rf "$workdir"' EXIT

# The verdict of a trace replay: TRACE:<depth> when TLC found no error from
# exactly one initial state, DEADLOCK:<l> with the index l of the event the
# last state of the deadlocked behaviour waits on, ERROR otherwise.
trace_verdict() {
  local out="$1" depth last
  depth=$(sed -n 's/^The depth of the complete state graph search is \([0-9]*\)\.$/\1/p' <<<"$out")
  if grep -q "No error has been found" <<<"$out" &&
     grep -q "^Finished computing initial states: 1 distinct state generated" <<<"$out"; then
    echo "TRACE:$depth"
  elif grep -q "Deadlock reached" <<<"$out"; then
    last=$(grep -E '^/\\ l = [0-9]+$' <<<"$out" | tail -n 1)
    echo "DEADLOCK:${last##* }"
  else
    echo ERROR
  fi
}

run_model() {
  local index="$1" row="$2" cfg mod want out got start
  read -r cfg mod want <<<"$row"
  mkdir -p "$workdir/java-$index"
  start=$SECONDS
  out=$(TLA_JAVA_TMPDIR="$workdir/java-$index" ./tools/tlc -metadir "$workdir/state-$index" -config "$cfg" "$mod" 2>&1)
  echo "$((SECONDS-start)) $cfg" >"$workdir/$index.time"
  # Here-strings, not pipes: `grep -q` stops at the first match, and under
  # pipefail the writer's broken pipe would turn a long trace into ERROR.
  case "$want" in
    TRACE:*|DEADLOCK:*) got=$(trace_verdict "$out") ;;
    *)
      if grep -q "No error has been found" <<<"$out"; then got=GREEN
      elif grep -qE "is violated|was violated|properties were violated|Deadlock reached|Deadlock" <<<"$out"; then got=RED
      else got=ERROR; fi ;;
  esac
  if [ "$got" = "$want" ]; then
    printf "  ok   %-6s %-34s %s\n" "$got" "$cfg" "$mod"
  else
    printf "  FAIL want=%s got=%s  %-30s %s\n%s\n" "$want" "$got" "$cfg" "$mod" "$out"
    return 1
  fi
}

run_indexed() {
  local index="${1%% *}"
  run_model "$index" "${1#* }" >"$workdir/$index.result" 2>&1 || touch "$workdir/$index.fail"
}
export workdir
export -f run_model trace_verdict run_indexed

# A rolling pool: a lane takes the next run as soon as its last one ends, so
# one slow model never holds the other lanes idle the way a fixed batch did.
n=${#RUNS[@]}
for index in "${!RUNS[@]}"; do printf '%s %s\n' "$((index+1))" "${RUNS[$index]}"; done |
  xargs -P "$parallelism" -I{} bash -c 'run_indexed "$1"' _ {}

fails=0
for ((index=1; index<=n; index++)); do
  if [ ! -e "$workdir/$index.result" ] || [ -e "$workdir/$index.fail" ]; then fails=$((fails+1)); fi
  cat "$workdir/$index.result" 2>/dev/null || echo "  FAIL run $index never ran: ${RUNS[$((index-1))]}"
done

echo "----"
echo "slowest runs (seconds):"
sort -rn "$workdir"/*.time 2>/dev/null | sed -n '1,10s/^/  /p'
if [ "$fails" -eq 0 ]; then echo "all $n TLA+ runs matched their expected verdict"; else echo "$fails/$n MISMATCHED"; fi
exit "$((fails > 0))"
