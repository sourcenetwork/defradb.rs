#!/bin/bash
# Runs one vector_disk_cost query process in a cgroup capped at <cap> with swap
# off, from a cold page cache, and prints its results plus the cgroup's peak
# memory, major faults and device I/O. Needs systemd and sudo; Linux only.
#
# usage: BIN=<vector test binary> capped.sh <cap|infinity> <name> <dir> <engine> <profile> <effort> <threads> [warmup.f32]
# Any DB=... in the environment is passed through, for a SPLIT=1 build's db-resident.
set -u
cap=$1 name=$2 dir=$3 engine=$4 profile=$5 e=$6 th=$7 wu=${8:-}
sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null
sudo systemd-run --scope -q -p MemoryMax=$cap -p MemorySwapMax=0 -p IOAccounting=yes \
  --uid=$(id -u) --setenv=HOME=$HOME bash -c "cg=/sys/fs/cgroup\$(cut -d: -f3 /proc/self/cgroup); \
  ${wu:+WARMUP=$wu} ${DB:+DB=$DB} PASSES=1 THREADS=$th PROFILE=$profile DIR=$dir ENGINE=$engine EFFORT=$e PHASE=query \
  $BIN vector_disk_cost --ignored --nocapture 2>&1 | grep -E \"RESULT|RSS|QPS|panicked\"; \
  echo \"CG $name cap=$cap peak=\$((\$(cat \$cg/memory.peak) >> 20))MiB majfault=\$(grep -w pgmajfault \$cg/memory.stat | cut -d\" \" -f2) io=\$(grep -v \"^\$\" \$cg/io.stat | tr \"\\n\" \" \")\"" 2>&1
