#!/usr/bin/env bash
# Writes thread stacks to target/e2e/stack-dumps/logs/, which the job uploads
# with the node logs: every defra process's once a `defra client` command has
# run for 20 seconds (a request a node does not answer), and with
# WATCH_TESTS=1 a test process's once one test has run for 25 seconds.
set -u
out="${GITHUB_WORKSPACE:-$PWD}/target/e2e/stack-dumps/logs"
mkdir -p "$out"
uname -a > "$out/kernel.log"
command -v gdb >/dev/null || sudo apt-get install -y -qq gdb >/dev/null 2>&1
declare -A dumped

age() { ps -o etimes= -p "$1" 2>/dev/null | tr -d ' '; }

stacks() {
  echo "=== $1: $(tr '\0' ' ' < "/proc/$1/cmdline")"
  for t in /proc/"$1"/task/*; do
    echo "$(cat "$t/comm") state=$(awk '{print $3}' "$t/stat") wchan=$(cat "$t/wchan") syscall=$(cut -d' ' -f1 "$t/syscall" 2>/dev/null)"
  done
  for fd in /proc/"$1"/fd/*; do
    if [ "$(readlink "$fd")" = "anon_inode:[io_uring]" ]; then
      echo "--- io_uring fd ${fd##*/}"
      cat "/proc/$1/fdinfo/${fd##*/}"
    fi
  done
  sudo gdb -p "$1" -batch -ex 'thread apply all bt' 2>&1 | grep -v '^\[New LWP'
}

while true; do
  for client in $(pgrep -f '^[^ ]*target/ci/defra .* client '); do
    a=$(age "$client")
    if [ -z "$a" ] || [ "$a" -lt 20 ] || [ -n "${dumped[$client]:-}" ]; then
      continue
    fi
    dumped[$client]=1
    file="$out/stacks-$(date +%s)-$client.log"
    {
      for pid in $(pgrep -f '^[^ ]*target/ci/defra '); do stacks "$pid"; done
      echo "=== sockets"
      sudo ss -tanpi
    } > "$file" 2>&1
  done
  if [ "${WATCH_TESTS:-}" = 1 ]; then
    for test in $(pgrep -f '^[^ ]*/target/[^ ]*/deps/[^ ]* --exact '); do
      a=$(age "$test")
      if [ -z "$a" ] || [ "$a" -lt 25 ] || [ -n "${dumped[$test]:-}" ]; then
        continue
      fi
      dumped[$test]=1
      stacks "$test" > "$out/test-stacks-$(date +%s)-$test.log" 2>&1
    done
  fi
  sleep 5
done
