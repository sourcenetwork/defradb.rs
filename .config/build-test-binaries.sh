#!/usr/bin/env bash
# Builds the defra binaries that node-starting tests launch, once per run, and
# hands their paths to every test. A test that builds its own overwrites the one
# shared `target/<profile>/defra` while other tests are running it.
#
# Usage: build-test-binaries.sh integration|conformance
# A binary whose variable is already set (CI sets them) is not built.
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

case "${1:-}" in
    integration)
        build DEFRA_RUST_BINARY defra
        build DEFRA_IROH_BINARY defra-iroh --features iroh
        ;;
    # Conformance would otherwise launch `target/debug/defra`, which nextest never rebuilds.
    conformance)
        build DEFRA_CONFORMANCE_BINARY defra
        ;;
    *)
        echo "usage: $0 integration|conformance" >&2
        exit 2
        ;;
esac
