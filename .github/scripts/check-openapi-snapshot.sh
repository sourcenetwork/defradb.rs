#!/usr/bin/env bash
# Checks the Go OpenAPI snapshot the HTTP crate merges into its served document
# is Go's document at GO_COMPAT_COMMIT: the file `handlers/openapi.rs` includes
# is named after the pin, no other snapshot lies next to it, and its content is
# Go's checked-in document at the pin, compared as JSON (the snapshot is
# minified, Go's file is indented).
#
# Usage: check-openapi-snapshot.sh [GO_REPO]
# With GO_REPO (or DEFRADB_GO_REPO), Go's document is read with `git show`, so
# the pin must be fetched there; otherwise it is fetched from GitHub at the pin,
# authenticated with GITHUB_TOKEN when it is set.
set -euo pipefail

readonly GO_DOCUMENT="docs/website/references/http/openapi.json"
readonly GO_REPOSITORY="sourcenetwork/defradb"

if [ "$#" -gt 1 ]; then
    echo "Usage: $0 [GO_REPO]" >&2
    exit 2
fi
go_repo="${1:-${DEFRADB_GO_REPO:-}}"

root=$(git rev-parse --show-toplevel)
handlers="$root/crates/surfaces/http/src/handlers"

pin=$(sed -n 's/^pub const GO_COMPAT_COMMIT: &str = "\(.*\)";$/\1/p' \
    "$root/crates/foundation/defra-version/src/lib.rs")
if [ -z "$pin" ]; then
    echo "GO_COMPAT_COMMIT not found in crates/foundation/defra-version/src/lib.rs" >&2
    exit 1
fi

snapshot=$(sed -n 's/.*include_str!("\(openapi-go-[^"]*\.json\)").*/\1/p' "$handlers/openapi.rs")
if [ "$snapshot" != "openapi-go-$pin.json" ]; then
    echo "handlers/openapi.rs includes '${snapshot:-nothing}', but GO_COMPAT_COMMIT is $pin:" >&2
    echo "fetch Go's $GO_DOCUMENT at $pin into handlers/openapi-go-$pin.json and include that" >&2
    exit 1
fi

stale=$(find "$handlers" -maxdepth 1 -name 'openapi-go-*.json' ! -name "$snapshot" -exec basename {} \;)
if [ -n "$stale" ]; then
    echo "stale snapshots next to $snapshot, remove them: $stale" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ -n "$go_repo" ]; then
    git -C "$go_repo" show "$pin:$GO_DOCUMENT" > "$work/go.json"
else
    auth=()
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth=(-H "Authorization: Bearer $GITHUB_TOKEN")
    fi
    curl -fsSL ${auth[@]+"${auth[@]}"} -H "Accept: application/vnd.github.raw" \
        "https://api.github.com/repos/$GO_REPOSITORY/contents/$GO_DOCUMENT?ref=$pin" \
        > "$work/go.json"
fi

canonical() {
    python3 -I -c 'import json, sys; print(json.dumps(json.load(open(sys.argv[1])), indent=2, sort_keys=True))' "$1"
}
canonical "$work/go.json" > "$work/go.canonical.json"
canonical "$handlers/$snapshot" > "$work/snapshot.canonical.json"

if ! diff -u --label "go@$pin:$GO_DOCUMENT" --label "handlers/$snapshot" \
    "$work/go.canonical.json" "$work/snapshot.canonical.json" > "$work/diff"; then
    head -n 200 "$work/diff" >&2
    echo "handlers/$snapshot is not Go's $GO_DOCUMENT at $pin: refresh it from that commit" >&2
    exit 1
fi
echo "handlers/$snapshot is Go's $GO_DOCUMENT at $pin"
