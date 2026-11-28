# Apple Embedding

`tools/apple/build-ffi.sh` packages the `ffi` crate for Apple embedding with a modern iOS deployment target, per-target static libraries, a Clang module map, `.xcframework` assembly, and a local Swift Package wrapper for Xcode.

## Prerequisites

- `cargo`
- `rustup`
- `cbindgen`
- `xcrun`
- Apple Rust targets:
  - `aarch64-apple-ios`
  - `aarch64-apple-ios-sim`
  - `x86_64-apple-ios`
- `xcodebuild` for `.xcframework` creation
- `lipo` when combining multiple simulator architectures

## Usage

```bash
tools/apple/build-ffi.sh
```

The script defaults to an iroh-capable build by setting
`FEATURES=iroh,native` and `NO_DEFAULT_FEATURES=1`.
It also defaults to `APPLE_DEPLOYMENT_TARGET=15.0` so Rust and native dependencies
are linked against a modern iOS target instead of Rust's legacy iOS 10 default.
The default iOS feature set avoids desktop-only FFI defaults such as libp2p and
the Wasmtime lens runtime. The store is regolith on every target.

Override the defaults with environment variables when needed:

```bash
APPLE_DEPLOYMENT_TARGET=16.0 \
FEATURES="iroh,native" \
OUT_DIR="$PWD/dist/apple" \
FRAMEWORK_NAME="DefraMobile" \
tools/apple/build-ffi.sh
```

Set `NO_DEFAULT_FEATURES=0` only for a desktop-style experiment where the full
FFI default feature set is intentionally desired.

`CARGO_TARGET_DIR` can also be overridden when you want the Apple build cache
somewhere other than `target/apple-cargo/<deployment-target>`.

## Outputs

The default output root is `target/apple-ffi/`.

- `include/defra.h`
- `include/module.modulemap`
- `lib/aarch64-apple-ios/libffi.a`
- `lib/aarch64-apple-ios-sim/libffi.a`
- `lib/x86_64-apple-ios/libffi.a`
- `lib/simulator/libffi.a`
- `xcframework/DefraFFI.xcframework`
- `swift/Package.swift`

If `xcodebuild` is unavailable, the script still emits the header and static libraries and skips the `.xcframework` step with a clear warning.

## Swift / Xcode

There are two supported integration paths:

1. Add `target/apple-ffi/swift` as a local Swift package in Xcode.
   The generated `Package.swift` wraps the XCFramework directly.
2. Add `target/apple-ffi/xcframework/DefraFFI.xcframework` to Xcode manually.
   The build script injects `module.modulemap` into every slice so Swift can
   `import DefraFFI` without a bridging header.

The generated C module is named after `FRAMEWORK_NAME` and defaults to `DefraFFI`.

## C API Surface

The FFI covers the calls of Go DefraDB's cbindings (`libdefradb.h`), named
`defra_<noun>_<verb>`. It matches Go's API, not its ABI: names and types are
prefixed (`DefraResult` for Go's `Result`), and `DefraNodeInitOptions` uses
its own layout. Every call that works on a store takes a handle naming a
node or one of its transactions. Results are `DefraResult` structs (`status`, `value`, `error`);
free each one with `defra_result_free`, and the node, transaction and identity
results with `defra_new_node_result_free`, `defra_new_txn_result_free` and
`defra_new_identity_result_free`.

A call that can grow takes its options as a struct, passed by value, whose
zero value is every default; the caller's identity handle (`0` for none) is
its `identity` field. Fields are only ever appended, so an app built against
an older header keeps working with a newer library. A field whose name
starts with `_reserved` (`_reserved0`, `_reserved1`, ...) fills what would
otherwise be padding: leave it zero. It never becomes a field. In Swift, start
from the zero value and set what you need:

```swift
var opts = DefraQueryOptions()
opts.identity = identity
let result = defra_query_execute(node, query, opts)
```

The entry points a mobile app typically needs:

- `defra_node_new(DefraNodeInitOptions)` / `defra_node_close(node)`
- `defra_collection_add(node, sdl, DefraCollectionAddOptions)`
- `defra_query_execute(node_or_txn, query, DefraQueryOptions)` (`identity`,
  `operation_name`, `variables`) and
  `defra_query_execute_async(node_or_txn, query, callback, context, DefraQueryOptions)`
- `defra_transaction_create(node, DefraTransactionOptions)` (`read_only`),
  `defra_transaction_commit(txn)`, `defra_transaction_discard(txn)`
- `defra_p2p_info(node, DefraP2POptions)`
- `defra_p2p_connect(node, addresses, DefraP2POptions)` /
  `defra_p2p_disconnect(node, addresses, DefraP2POptions)`
- `defra_p2p_replicator_add(node, collections, addresses, DefraReplicatorAddOptions)`
  (set `filters_json` to push only matching documents), and
  `defra_p2p_replicator_retry(node, DefraP2POptions)` to re-push what a
  replicator missed, for example after a reconnect
- `defra_p2p_collection_sync_versions(node, version_ids, DefraP2PSyncOptions)`
  and `defra_p2p_document_sync(node, collection, doc_ids, DefraP2PSyncOptions)`
- `defra_identity_new_remote(did, DefraRemoteIdentityOptions)`
- `defra_identity_new_from_bearer_token(did, bearer_token, DefraIdentityBearerOptions)`
- `defra_node_set_identity(node, identity)`

List arguments (`addresses`, `collections`, `doc_ids`, `version_ids`) are
comma-separated strings. The sync calls take `timeout_ms`, in milliseconds
(`0` for none).

Mobile-only calls, which Go's cbindings have no counterpart for, each taking
`DefraP2POptions`:

- `defra_mobile_sync_status(node, opts)`
- `defra_mobile_peer_details(node, peer_id, opts)`
- `defra_mobile_shareable_address(node, opts)`
- `defra_mobile_notify_network_change(node, opts)`

Rebuild the XCFramework after updating the FFI crate so the generated header
includes newly added symbols.

A node is configured with `DefraNodeInitOptions` fields rather than JSON. For
an iroh node with a persistent store, set for example `db_path`,
`p2p_transport = "iroh"`, `iroh_relay_url`, and `identity_ptr` (an identity
handle the node signs as). Leave `disable_p2p` at `0` to run P2P; a libp2p
node listens on the comma-separated `listening_addresses`.

For device-bound or delegated identities:

- register the device signing DID with `defra_identity_new_remote`, which
  answers with an identity handle
- optionally bind a delegated bearer token to a logical DID with
  `defra_identity_new_from_bearer_token`, which answers with a handle too
- set the effective identity as the node default via `defra_node_set_identity`,
  or pass its handle in each call's options

## Device-Bound DID Flow

For Secure Enclave-backed P-256 (`secp256r1`) identities on iOS:

1. Generate or load the Secure Enclave private key in the host app.
2. Export or derive the public key bytes, then derive the matching `did:key`.
3. Call `defra_identity_new_remote(did, opts)` with a
   `DefraRemoteIdentityOptions` that sets `key_type = "secp256r1"`,
   `signer_handle`, one signing callback (`sign`, or `sign_async`; see below),
   and the public key: `public_key_hex`, or `public_key` and `public_key_len`
   for raw bytes.
4. If the app uses a logical DID with delegated ACP authorization, call
   `defra_identity_new_from_bearer_token(logical_did, bearer_token, DefraIdentityBearerOptions())`.
5. Call `defra_node_set_identity(node, identity_ptr)` with the handle of either
   the device identity or the logical one that should back GraphQL and
   block-signing requests.

Keep `signer_handle` valid and the callback callable until every node using
the DID is closed (`defra_node_close`) or given another identity
(`defra_node_set_identity`), not just until you call `defra_identity_free`. A
node keeps its hold on the identity after the handle is freed and goes on
signing through the callback, so freeing the signing context together with
the handle leaves that node calling into freed memory.

While a DID has live identity handles, its signer is fixed. Registering the DID
again with another `signer_handle` or callback fails until every handle for it
is freed; registering it with the same `signer_handle` and callback gives
another handle for the same signer.

Setting `public_key` and `public_key_len` is the most direct Swift path when
the app already has the public key as `Data`. The public key formats:

- `secp256r1` / `secp256k1`: uncompressed SEC1 / X9.63 bytes (`0x04 || X || Y`)
- `ed25519`: the raw 32-byte public key

`public_key_hex` is the same bytes, hex-encoded. An empty or null `key_type`
means `secp256r1`.

## Callback Contract

`DefraRemoteSignCallback`, set as `sign`, is the host signing hook for
non-exportable keys.

- The host app keeps the private key. Defra never receives or persists it.
- `signer_handle` is an opaque host context value passed back on every sign.
- `payload_ptr` / `payload_len` are the exact message bytes to sign.
- On success, write the signature into the provided output buffer, set
  `out_signature_len`, and return `0`.
- For ECDSA keys (`secp256r1`, `secp256k1`), Defra expects ASN.1 DER / X9.62
  signatures, not raw `r || s`.
- For Secure Enclave P-256 signing, use
  `SecKeyCreateSignature(..., .ecdsaSignatureMessageX962SHA256, ...)`.

`DefraRemoteSignAsyncCallback`, set as `sign_async` instead of `sign`, is the
form for a signer that answers later (its own queue, a network round trip). No
Defra thread waits while the host signs, so it also suits a node running a
single worker. Set exactly one of `sign` and `sign_async`:

- The callback receives the same values plus a `completion` value, and returns
  `0` once the signature is under way (any other value fails the request).
- `payload_ptr`, `did` and `key_type` are valid only during the callback: copy
  the payload before signing on another queue.
- Report the result exactly once with
  `defra_remote_sign_complete(completion, status, signature_ptr, signature_len)`
  from any thread: `status` `0` with the signature, `DEFRA_SIGN_RETRYABLE` when
  the request never reached the signer, any other value for a refusal. It
  returns `1` when nothing waits under `completion` any more.
- The callback runs on a Defra thread that serves other work: return promptly.

## Validation

`tools/apple/build-ffi.sh` now runs a Swift import smoke test after packaging.
It typechecks [swift-import-smoke.swift](swift-import-smoke.swift)
against the generated `target/apple-ffi/include` module and fails if the
header regresses to `Option_DefraRemoteSignCallback`.
