#!/usr/bin/env bash
# When a `defra client` command has run for 45 seconds (a request to a node
# that does not answer), writes the thread stacks of every defra process to
# target/e2e/stack-dumps/logs/, which the job uploads with the node logs.
set -u
out="${GITHUB_WORKSPACE:-$PWD}/target/e2e/stack-dumps/logs"
mkdir -p "$out"
command -v gdb >/dev/null || sudo apt-get install -y -qq gdb >/dev/null 2>&1
declare -A dumped
while true; do
  for client in $(pgrep -f '^[^ ]*target/ci/defra[^ ]* client'); do
    age=$(ps -o etimes= -p "$client" 2>/dev/null | tr -d ' ')
    if [ -z "$age" ] || [ "$age" -lt 45 ] || [ -n "${dumped[$client]:-}" ]; then
      continue
    fi
    dumped[$client]=1
    file="$out/stacks-$(date +%s)-$client.log"
    {
      echo "client $client: $(tr '\0' ' ' < "/proc/$client/cmdline")"
      for pid in $(pgrep -f '^[^ ]*target/ci/defra'); do
        echo "=== $pid: $(tr '\0' ' ' < "/proc/$pid/cmdline")"
        sudo gdb -p "$pid" -batch -ex 'thread apply all bt' 2>&1 | grep -v '^\[New LWP'
      done
    } > "$file" 2>&1
    echo "stack-watchdog: wrote $file"
  done
  sleep 5
done
