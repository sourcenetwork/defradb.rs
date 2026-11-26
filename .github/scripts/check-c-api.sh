#!/usr/bin/env bash
# Checks the C API this working tree generates against the one BASE generates:
# the new header compiles with no struct padding (`just check-ffi-header`), and
# check-c-api.py finds nothing removed or changed that callers of the old one
# depend on. With C_API_ACCEPT_BREAK=true (the pull request carries the
# c-api:break label) every break is still reported, as a warning, and passes.
#
# Usage: check-c-api.sh BASE
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 BASE" >&2
    exit 2
fi
readonly BASE="$1"

root=$(git rev-parse --show-toplevel)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

generate() {
    local tree="$1" out="$2"
    cbindgen --quiet --config "$tree/crates/entry-points/ffi/cbindgen.toml" --crate ffi --output "$out" "$tree"
}

mkdir "$work/base"
git -C "$root" archive "$BASE" | tar -x -C "$work/base"
generate "$work/base" "$work/old.h"
generate "$root" "$work/new.h"

just --justfile "$root/justfile" check-ffi-header "$work/new.h"
accept=()
if [ "${C_API_ACCEPT_BREAK:-false}" = true ]; then
    accept=(--accept-break)
fi
python3 "$root/.github/scripts/check-c-api.py" "$work/old.h" "$work/new.h" "${accept[@]}"
