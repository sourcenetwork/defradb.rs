#!/usr/bin/env bash
# Feature-graph contracts for defra-node (#1398–#1400). Not a size check.
#
# Inspect `cargo tree -p <crate> -e normal --locked [features] -i <pkg>` stdout.
# Present iff a line matches ^<pkg> v (e.g. ^vera v). Absent iff exit 101
# or exit 0 with empty stdout (cargo prints `warning: nothing to print.` on
# stderr for unused optional deps). Never treat the -i exit code as the
# predicate. Never pass --workspace: workspace feature unification would pull
# the CLI's pinc into every tree.
set -euo pipefail

# Always `-p defra-node`, `-p cli`, `-p db`, `-p ffi` or `-p vera-delegation`.
# Workspace trees are forbidden.
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

assert_present() {
  local crate=$1
  local pkg=$2
  shift 2
  local stdout rc
  set +e
  stdout="$(cargo tree -p "$crate" -e normal --locked "$@" -i "$pkg")"
  rc=$?
  set -e
  if grep -q "^${pkg} v" <<<"$stdout"; then
    echo "ok: ${crate}${*:+ $*} contains ^${pkg} v"
    return 0
  fi
  echo "error: expected ^${pkg} v in ${crate}${*:+ $*} tree (cargo tree -i exit ${rc})" >&2
  if [ -n "$stdout" ]; then
    printf '%s\n' "$stdout" >&2
  fi
  exit 1
}

# Present iff stdout matches ^<pkg> v. Absent iff no such line: exit 101 or
# exit 0 + empty stdout both count as absent. Never treat the -i exit code
# as present.
assert_absent() {
  local crate=$1
  local pkg=$2
  shift 2
  local stdout rc
  set +e
  stdout="$(cargo tree -p "$crate" -e normal --locked "$@" -i "$pkg")"
  rc=$?
  set -e
  if grep -q "^${pkg} v" <<<"$stdout"; then
    echo "error: unexpected ^${pkg} v in ${crate}${*:+ $*} tree (cargo tree -i exit ${rc})" >&2
    printf '%s\n' "$stdout" >&2
    exit 1
  fi
  echo "ok: ${crate}${*:+ $*} has no ^${pkg} v"
}

unique_crate_names() {
  cargo tree -p defra-node -e normal --locked --prefix none "$@" \
    | awk '{print $1}' | grep -v '^(*)' | sort -u | wc -l
}

assert_present defra-node vera
assert_present defra-node wasmtime
# The CLI always runs P2P, over pinc.
assert_present cli pinc

# Default defra-node (no p2p feature) and the isolated db native graph carry no
# transport.
assert_absent defra-node pinc
assert_absent db pinc --no-default-features --features native

# Lean local-ACP + native host. No Vera, no Wasmtime, no transport.
assert_absent defra-node vera --no-default-features --features native
assert_absent defra-node acp-light-client --no-default-features --features native
assert_absent defra-node commonware-cryptography --no-default-features --features native
assert_absent defra-node aws-lc-rs --no-default-features --features native
assert_absent defra-node cosmrs --no-default-features --features native
assert_absent defra-node wasmtime --no-default-features --features native
assert_absent defra-node cranelift-codegen --no-default-features --features native
assert_absent defra-node pinc --no-default-features --features native

# Lean P2P: pinc present. `p2p` implies native, so listing native is redundant
# but matches the advertised combo.
assert_present defra-node pinc --no-default-features --features native,p2p

# Lean-embedded ffi (#1345): pinc in, wasmtime/vera out.
LEAN_FFI=(--no-default-features --features native,p2p)
assert_present ffi pinc "${LEAN_FFI[@]}"
assert_absent  ffi wasmtime "${LEAN_FFI[@]}"
assert_absent  ffi vera "${LEAN_FFI[@]}"
assert_absent  ffi cosmrs "${LEAN_FFI[@]}"

# No-transport ffi (#1653): no pinc anywhere in the graph.
assert_absent ffi pinc --no-default-features --features native

# Default ffi carries the full capability set.
assert_present ffi pinc
assert_present ffi vera
assert_present ffi wasmtime

# Delegation minting (#3038) builds wherever a client runs: no vera.rs client
# or Commonware runtime (which does not build for Windows), no keyring (the
# upstream crate links the OS keyring), no network runtime. `--target all`
# also takes in target-specific dependencies no real target selects.
assert_graph_free_of() {
  local crate=$1 target=$2 graph pkg
  shift 2
  graph="$(cargo tree -p "$crate" -e normal --locked --target "$target" --prefix none)"
  for pkg in "$@"; do
    if grep -q "^${pkg} v" <<<"$graph"; then
      echo "error: unexpected ^${pkg} v in ${crate} --target ${target} tree" >&2
      cargo tree -p "$crate" -e normal --locked --target "$target" -i "$pkg" >&2 || true
      exit 1
    fi
  done
  echo "ok: ${crate} --target ${target} has none of: $*"
}
for target in all x86_64-pc-windows-msvc x86_64-unknown-linux-gnu; do
  assert_graph_free_of vera-delegation "$target" \
    commonware-runtime commonware-glue commonware-consensus commonware-p2p commonware-storage \
    vera-client vera-domain vera-permission acp-light-client keyring edge cosmrs reqwest
done

# Unique crate names. Log only — not a gate and not binary size. Main may move.
echo "defra-node default unique crate names: $(unique_crate_names)"
echo "defra-node --no-default-features --features native unique crate names: $(unique_crate_names --no-default-features --features native)"
echo "defra-node --no-default-features --features native,p2p unique crate names: $(unique_crate_names --no-default-features --features native,p2p)"

assert_sha2_backend() {
  local target=$1 expected=$2 actual=portable graph
  graph="$(cargo tree -p defra-node -e normal --locked --target "$target" --prefix none)"
  if [[ "$graph" == *$'\nsha2-asm v'* ]]; then
    actual=assembly
  fi
  if [[ "$actual" != "$expected" ]]; then
    printf 'error: %s selects %s SHA-2, expected %s\n' "$target" "$actual" "$expected" >&2
    exit 1
  fi
  printf 'ok: %s selects %s SHA-2\n' "$target" "$actual"
}

assert_sha2_backend x86_64-pc-windows-msvc portable
assert_sha2_backend aarch64-pc-windows-msvc portable
assert_sha2_backend x86_64-pc-windows-gnu assembly
assert_sha2_backend x86_64-unknown-linux-gnu assembly
assert_sha2_backend aarch64-apple-darwin assembly
