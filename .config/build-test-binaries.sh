#!/usr/bin/env bash
# Builds the defra binaries that node-starting tests launch, once per run, and
# hands their paths to every test. A test that builds its own overwrites the one
# shared `target/<profile>/defra` while other tests are running it.
set -euo pipefail

target="${CARGO_TARGET_DIR:-target}"
out="$target/test-binaries"
mkdir -p "$out"

build() {
    local var=$1 name=$2
    shift 2
    [[ -n "${!var:-}" ]] && return
    cargo build -q -p cli --profile super-dev "$@"
    cp "$target/super-dev/defra" "$out/$name"
    echo "$var=$(cd "$out" && pwd)/$name" >> "$NEXTEST_ENV"
}

build DEFRA_RUST_BINARY defra
build DEFRA_IROH_BINARY defra-iroh --features iroh
# Conformance would otherwise launch `target/debug/defra`, which nextest never rebuilds.
if [[ -z "${DEFRA_CONFORMANCE_BINARY:-}" ]]; then
    echo "DEFRA_CONFORMANCE_BINARY=${DEFRA_RUST_BINARY:-$(cd "$out" && pwd)/defra}" >> "$NEXTEST_ENV"
fi
