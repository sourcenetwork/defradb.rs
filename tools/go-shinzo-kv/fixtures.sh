#!/usr/bin/env bash
# Writes the KV snapshots defradb.rs's import tests read
# (tools/integration-test/tests/shinzo/fixtures/), exported by Go Shinzo
# shinzo-develop 2a899b860, the last Go Shinzo nodes defradb.rs imports from.
# Needs Go 1.26 and network access; run from the repository root:
#
#   tools/go-shinzo-kv/fixtures.sh
set -euo pipefail
COMMIT=2a899b860
OUT=$PWD/tools/integration-test/tests/shinzo/fixtures
WORK=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$WORK"' EXIT

git clone -q --filter=blob:none --branch shinzo-develop https://github.com/sourcenetwork/defradb "$WORK/src"
git -C "$WORK/src" checkout -q "$COMMIT"
make -C "$WORK/src" build path="$WORK/defradb" >/dev/null
mkdir -p "$WORK/src/cmd/kvsnap"
cp tools/go-shinzo-kv/main.go "$WORK/src/cmd/kvsnap/main.go"
(cd "$WORK/src" && go build -o "$WORK/kvsnap" ./cmd/kvsnap)

URL=127.0.0.1:9181
client() { "$WORK/defradb" client --url "$URL" "$@"; }
query() { client query "$1" | sed -n 's/.*"_docID": *"\([^"]*\)".*/\1/p' | head -1; }

# A node at <root>, the collections in $2; runs the rest of the arguments as
# commands against it, then stops it.
node() {
  local root=$WORK/$1
  # Unsigned and unencrypted, as testo starts Go nodes: Go's export leaves out
  # signature blocks, so a signed document's snapshot does not import (#2608).
  "$WORK/defradb" start --rootdir "$root" --url "$URL" --no-p2p --no-keyring --development \
    --no-signing --no-encryption --no-searchable-encryption --no-telemetry >"$root.log" 2>&1 &
  local pid=$!
  until client collection describe >/dev/null 2>&1; do sleep 0.2; done
  client collection add "$2" >/dev/null
  shift 2
  "$@"
  kill "$pid"
  wait "$pid" 2>/dev/null || true
}

# Each adds documents and appends their DocIDs to $WORK/<node>.<collection>:
# Go exports only the documents it is named.
add() { query "mutation { add_$2(input: $3) { _docID } }" >>"$WORK/$1.$2"; }
users() {
  add users User '{name: "Alice", age: 40}'
  add users User '{name: "Bob", age: 7}'
  add users Log '{address: "a5", blockNumber: 5}'
}
updated() {
  add updated User '{name: "Alice", age: 40}'
  add updated User '{name: "Bob", age: 7}'
  query "mutation { update_User(docID: \"$(head -1 "$WORK/updated.User")\", input: {age: 41}) { _docID } }" >/dev/null
}
email() {
  add email User '{name: "Alice", age: 40, email: "alice@example.com"}'
}

USERS='type User { name: String @index age: Int }'
LOGS='type Log { address: String blockNumber: Int }'
node users "$USERS $LOGS" users
node updated "$USERS" updated
node email 'type User { name: String @index age: Int email: String }' email

mkdir -p "$OUT"
kv() {
  # shellcheck disable=SC2046 # one argument per DocID
  "$WORK/kvsnap" "$1" "$WORK/$2" "$3" "$OUT/$4" $(cat "$WORK/$2.$3") >/dev/null
}
kv export users User users.kv
kv export-datastore users User users-datastore.kv
kv export users Log logs.kv
kv export updated User users-updated.kv
kv export email User users-email.kv
ls -l "$OUT"
