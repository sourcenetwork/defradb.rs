#!/usr/bin/env bash
# Generates the Rust conformance tests from the scenario corpus, one corpus
# area at a time, then checks the export round trip.
#
#   generate_scenarios.sh <scenarios-dir> [area...]
#
# Environment:
#   DEFRADB_GO_REPO   Go checkout holding commit 137dc624 and the parity pin (required)
#   REQS              requirements root holding go/ and defra/ (default <repo>/reqs)
#   CARGO_PROFILE     cargo profile (default super-dev)
#   RUN_TESTS=1       also run each generated area with nextest and summarize it
set -euo pipefail

scenarios=${1:?usage: generate_scenarios.sh <scenarios-dir> [area...]}
shift
root=$(git rev-parse --show-toplevel)
profile=${CARGO_PROFILE:-super-dev}
reqs=${REQS:-$root/reqs}
go_repo=${DEFRADB_GO_REPO:?set DEFRADB_GO_REPO}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

pin_short=$(sed -nE 's/^pub const GO_COMPAT_COMMIT: &str = "([0-9a-f]+)";/\1/p' "$root/crates/defra-version/src/lib.rs")
pin=$(git -C "$go_repo" rev-parse "$pin_short^{commit}")
git -C "$go_repo" diff --name-only 137dc624 "$pin" -- tests/integration >"$work/drift.txt"

cd "$root"
cargo build --profile "$profile" -p integration-test --features generator --bin gen_scenarios
target=${CARGO_TARGET_DIR:-$root/target}
gen="$target/$profile/gen_scenarios"

if [ "$#" -eq 0 ]; then
  areas=$(find "$scenarios" -mindepth 1 -maxdepth 1 -type d -exec basename {} \; | sort)
else
  areas="$*"
fi

for area in $areas; do
  echo "== $area"
  "$gen" generate --scenarios "$scenarios" --tests tools/integration-test/tests \
    --mantra "$reqs/go" --mantra "$reqs/defra" --drift "$work/drift.txt" \
    --go-repo "$go_repo" --pin "$pin" --area "$area" | grep -v '^  op '
  if [ "${RUN_TESTS:-0}" = 1 ]; then
    bin=$area
    case $area in acp | backup | encryption | query) bin=${area}_scenarios ;; esac
    cargo nextest run --cargo-profile "$profile" -p integration-test --test "$bin" \
      --no-fail-fast 2>&1 | tee "$work/$area.run" | grep -E '^ +Summary' || true
    cp "$work/$area.run" "$root/../scenario-run-$area.txt"
    DEFRA_EXPORT_SCENARIOS="$work/export-$area" cargo nextest run --cargo-profile "$profile" \
      -p integration-test --test "$bin" >/dev/null 2>&1 || true
    "$gen" check --scenarios "$scenarios" --exported "$work/export-$area" \
      --drift "$work/drift.txt" --pin "$pin" --area "$area" | head -20 || echo "ROUNDTRIP-FAILED $area"
  fi
done
