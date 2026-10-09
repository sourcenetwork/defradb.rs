#!/usr/bin/env bash
set -euo pipefail

cargo update -p alloy-primitives -p alloy-rlp -p c-kzg
cargo metadata --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu >/dev/null
