#!/usr/bin/env bash
# A browser peer and a native node replicating through the relay the node hosts.
#
# Starts `defra` with an iroh relay server and local document ACP, then runs
# defra-wasm's p2p_relay tests in headless Firefox against it. The browser
# tests do the asserting;
# this script only stands the node up and tears it down.
#
# Needs: wasm32-unknown-unknown target, wasm-bindgen-test-runner, geckodriver,
# and Firefox on PATH.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
node_pid=""

cleanup() {
    if [ -n "$node_pid" ]; then
        kill "$node_pid" 2>/dev/null || true
        wait "$node_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT

free_port() {
    python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}

api_port="$(free_port)"
relay_port="$(free_port)"
api="http://127.0.0.1:${api_port}"
relay="http://127.0.0.1:${relay_port}"

cargo build -p cli --features iroh-relay-server --manifest-path "$root/Cargo.toml"
defra="$root/target/debug/defra"

# `defra start` writes a complete default config before anything else, which is
# the only reliable way to get every section the loader requires.
"$defra" --rootdir "$work" --no-keyring start --store memory --no-telemetry \
    --url "127.0.0.1:${api_port}" >"$work/seed.log" 2>&1 &
seed_pid=$!
for _ in $(seq 1 100); do
    [ -f "$work/config.yaml" ] && break
    sleep 0.1
done
kill "$seed_pid" 2>/dev/null || true
wait "$seed_pid" 2>/dev/null || true
[ -f "$work/config.yaml" ] || { cat "$work/seed.log"; echo "no config.yaml generated" >&2; exit 1; }

sed -i \
    -e 's/^  transport: .*/  transport: iroh/' \
    -e 's/^  iroh_discovery: .*/  iroh_discovery: false/' \
    -e "s/^  allowed_origins: .*/  allowed_origins: ['*']/" \
    -e '/^  iroh_relay_server: /d' \
    "$work/config.yaml"
sed -i "s|^net:$|net:\n  iroh_relay_server:\n    http_bind_addr: 127.0.0.1:${relay_port}\n    public_url: ${relay}|" \
    "$work/config.yaml"

# The rule module the governed test names: it rejects every composite. Its
# bytes are REJECT_MODULE_HEX in crates/wasm/tests/p2p_relay/governed.rs.
module_hex="$(grep -o 'REJECT_MODULE_HEX: &str = "[0-9a-f]*"' \
    "$root/crates/wasm/tests/p2p_relay/governed.rs" | grep -o '"[0-9a-f]*"' | tr -d '"')"
python3 -c 'import sys; open(sys.argv[1], "wb").write(bytes.fromhex(sys.argv[2]))' \
    "$work/reject.wasm" "$module_hex"

# Local document ACP, so the browser tests can check which signer the node lets
# update a protected document. Unprotected collections still replicate openly.
# Ledger is governed by the rejecting module; nothing else is.
"$defra" --rootdir "$work" --no-keyring --document-acp-type local start --store memory \
    --no-telemetry --url "127.0.0.1:${api_port}" \
    --governed Ledger --rule-module "$work/reject.wasm" >"$work/node.log" 2>&1 &
node_pid=$!
for _ in $(seq 1 300); do
    curl -sf "${api}/api/v0/p2p/info" >/dev/null 2>&1 && break
    kill -0 "$node_pid" 2>/dev/null || { cat "$work/node.log"; exit 1; }
    sleep 0.1
done

status=0
# The runner's 20 second default covers the whole suite, which spends most of
# its time waiting on replication across the relay.
DEFRA_E2E_API="$api" DEFRA_E2E_RELAY="$relay" WASM_BINDGEN_TEST_TIMEOUT=300 \
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
    cargo test -p defra-wasm --target wasm32-unknown-unknown --features relay-e2e --test p2p_relay \
    --manifest-path "$root/Cargo.toml" 2>&1 | tee "$work/browser.log" || status=$?

# The runner can fail to kill a sandboxed geckodriver after the results print,
# so the result line is the verdict, as it is for the other browser tests.
if grep -qE "^test result: ok\. 5 passed" "$work/browser.log"; then
    exit 0
fi
echo "--- node log ---" >&2
tail -n 200 "$work/node.log" >&2
# A zero status here means Cargo succeeded without reporting the relay test.
if [ "$status" -ne 0 ]; then
    exit "$status"
fi
exit 1
