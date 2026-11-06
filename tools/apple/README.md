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
The default iOS feature set avoids desktop-only FFI defaults such as libp2p,
Vera, and the Wasmtime lens runtime. The store is regolith on every target.

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
prefixed (`DefraResult` for Go's `Result`), and `NodeInitOptions` adds fields
after Go's, so code built against Go's header needs these names. Every call that works on a store takes a handle naming
a node, or one of its transactions, and an identity handle naming the caller
(`0` for none). Results are `DefraResult` structs (`status`, `value`,
`error`); free each one with `defra_result_free`, and the node, transaction
and identity results with `defra_new_node_result_free`,
`defra_new_txn_result_free` and `defra_new_identity_result_free`.

The entry points a mobile app typically needs:

- `defra_node_new(options)` / `defra_node_close(node)`
- `defra_collection_add(node, sdl, identity)`
- `defra_query_execute(node_or_txn, query, identity, operation_name, variables)`
  and `defra_query_execute_async(..., callback, context)`
- `defra_transaction_create(node, read_only)`, `defra_transaction_commit(txn)`,
  `defra_transaction_discard(txn)`
- `defra_p2p_info(node, identity)`
- `defra_p2p_connect(node, addresses, identity)` /
  `defra_p2p_disconnect(node, addresses, identity)`
- `defra_p2p_replicator_add(node, collections, addresses, identity)`
- `defra_p2p_collection_sync_versions(node, version_ids, timeout, identity)`
  and `defra_p2p_document_sync(node, collection, doc_ids, timeout, identity)`
- `defra_identity_new_remote(...)` / `defra_identity_new_remote_bytes(...)`
- `defra_identity_new_from_bearer_token(did, bearer_token)`
- `defra_node_set_identity(node, identity)`

List arguments (`addresses`, `collections`, `doc_ids`, `version_ids`) are
comma-separated strings, and timeouts are Go duration strings such as `"30s"`
(null or empty for none).

Mobile-only calls, which Go's cbindings have no counterpart for:

- `defra_mobile_sync_status(node, identity)`
- `defra_mobile_peer_details(node, peer_id, identity)`
- `defra_mobile_shareable_address(node, identity)`
- `defra_mobile_notify_network_change(node, identity)`

Rebuild the XCFramework after updating the FFI crate so the generated header
includes newly added symbols.

A node is configured with `NodeInitOptions` fields rather than JSON. For an
iroh node with a persistent store, set for example `db_path`,
`p2p_transport = "iroh"`, `iroh_relay_url`, and `identity_ptr` (an identity
handle the node signs as). Leave `disable_p2p` at `0` to run P2P; a libp2p
node listens on the comma-separated `listening_addresses`.

For device-bound or delegated identities:

- register the device signing DID with `defra_identity_new_remote` or
  `defra_identity_new_remote_bytes`, which answer with an identity handle
- optionally bind a delegated bearer token to a logical DID with
  `defra_identity_new_from_bearer_token`, which answers with a handle too
- set the effective identity as the node default via `defra_node_set_identity`,
  or pass its handle to each call

## Device-Bound DID Flow

For Secure Enclave-backed P-256 (`secp256r1`) identities on iOS:

1. Generate or load the Secure Enclave private key in the host app.
2. Export or derive the public key bytes, then derive the matching `did:key`.
3. Call one of:
   - `defra_identity_new_remote(did, public_key_hex, "secp256r1", signer_handle, callback)`
   - `defra_identity_new_remote_bytes(did, public_key_ptr, public_key_len, "secp256r1", signer_handle, callback)`
4. If the app uses a logical DID with delegated ACP authorization, call
   `defra_identity_new_from_bearer_token(logical_did, bearer_token)`.
5. Call `defra_node_set_identity(node, identity_ptr)` with the handle of either
   the device identity or the logical one that should back GraphQL and
   block-signing requests.

`defra_identity_new_remote_bytes` is the most direct Swift path when the app
already has the public key as `Data`. For `secp256r1`, pass the uncompressed
X9.63 / SEC1 public key bytes (`0x04 || X || Y`).

## Callback Contract

`DefraRemoteSignCallback` is the host signing hook for non-exportable keys.

- The host app keeps the private key. Defra never receives or persists it.
- `signer_handle` is an opaque host context value passed back on every sign.
- `payload_ptr` / `payload_len` are the exact message bytes to sign.
- On success, write the signature into the provided output buffer, set
  `out_signature_len`, and return `0`.
- For ECDSA keys (`secp256r1`, `secp256k1`), Defra expects ASN.1 DER / X9.62
  signatures, not raw `r || s`.
- For Secure Enclave P-256 signing, use
  `SecKeyCreateSignature(..., .ecdsaSignatureMessageX962SHA256, ...)`.

`DefraRemoteSignAsyncCallback`, registered with `defra_identity_new_remote_async`
or `defra_identity_new_remote_bytes_async`, is the form for a signer that
answers later (its own queue, a network round trip):

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
