#!/usr/bin/env bash
# Builds the defra binaries that node-starting tests launch, once per run, and
# hands their paths to every test. A test that builds its own overwrites the one
# shared `target/<profile>/defra` while other tests are running it.
#
# Usage: build-test-binaries.sh integration|vera|conformance
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
    cp "$target/super-dev/defradb" "$out/$name"
    echo "$var=$(cd "$out" && pwd)/$name" >> "$NEXTEST_ENV"
}

# A test that spawns the Rust CLI without `--rootdir` would read the
# developer's ~/.defradb. Point the fallback at a config that does not parse,
# so such a spawn fails on every machine (#2193). Spawn the CLI through
# `integration_test::cli_command` instead. Go's defradb ignores this variable.
poison_rootdir() {
    local dir="$target/poisoned-rootdir"
    mkdir -p "$dir"
    printf 'spawned without --rootdir: use integration_test::cli_command: [\n' > "$dir/config.yaml"
    echo "DEFRA_ROOTDIR=$(cd "$dir" && pwd)" >> "$NEXTEST_ENV"
}

case "${1:-}" in
    integration)
        poison_rootdir
        build DEFRA_RUST_BINARY defra
        build DEFRA_IROH_BINARY defra-iroh --features iroh
        ;;
    vera)
        build DEFRA_VERA_BINARY defra-vera --features vera
        ;;
    # Conformance would otherwise launch `target/debug/defradb`, which nextest never rebuilds.
    conformance)
        build DEFRA_CONFORMANCE_BINARY defra
        build DEFRA_CONFORMANCE_IROH_BINARY defra-iroh --features iroh
        ;;
    *)
        echo "usage: $0 integration|vera|conformance" >&2
        exit 2
        ;;
esac
