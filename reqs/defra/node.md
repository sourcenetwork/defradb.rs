# `defra.node.start.no_nameservers`: A libp2p node starts when the host lists no DNS nameservers

- **Parents:** ["go.node"]
- **Scenarios:** ["node::resolver::a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers", "node::resolver::a_libp2p_node_starts_when_resolv_conf_does_not_parse"]

A node started with libp2p networking starts, listens and answers requests when the host's resolver configuration (`/etc/resolv.conf` on unix) lists no nameservers or has lines the resolver cannot parse: `PeerInfo` returns the addresses it listens on and an introspection query succeeds. Startup never fails with `no nameservers found in config`.

Which nameservers the node then uses (the host's accepted lines, or Go's `127.0.0.1:53` and `[::1]:53` fallback, never a public DNS service) is internal to the libp2p transport and no client can observe it, so it is a unit-level contract: the module docs of `crates/database/p2p/src/host/transport/resolver.rs`, its unit tests, and `proofs/lean/DnsResolverFallback.lean`.

# `defra.node.ffi_peer_keyring`: A C host keeps the node's peer key in its own keyring

- **Parents:** ["defra"]

A host that embeds the node through the C API (a Swift or Kotlin app) passes a keyring it owns, such as the Keychain or the Android Keystore, in the `peer_keyring_get`, `peer_keyring_set`, `peer_keyring_delete` and `peer_keyring_context` fields of `DefraNodeInitOptions`. The node keeps its peer key there, under the name `peer-key`, instead of in plaintext in its store. Choosing and securing the keystore is the host's job. It is a C API surface with no HTTP, CLI or Go equivalent, so no testo client reaches it and its scenarios are the `ffi` crate's tests.

# `defra.node.ffi_peer_keyring.identity`: The node's peer identity is the key in the host keyring

- **Parents:** ["defra.node.ffi_peer_keyring"]
- **Scenarios:** ["peer_keyring::tests::node::a_node_keeps_its_peer_key_in_the_host_keyring"]

On its first start with a host keyring the node generates an Ed25519 peer key and stores it once, 64 bytes (seed and public key) under `peer-key`. Every later start with the same keyring reads it back and has the same peer ID, writing nothing, and that peer ID is the one a node started with that key as `p2p_private_key` has.

# `defra.node.ffi_peer_keyring.fail_closed`: A keyring failure stops the node and never falls back to plaintext

- **Parents:** ["defra.node.ffi_peer_keyring"]
- **Scenarios:** ["peer_keyring::tests::node::a_keyring_failure_stops_the_node", "peer_keyring::tests::node::a_plaintext_peer_key_is_refused"]

A get callback that returns any status other than `0` or `DEFRA_KEYRING_NOT_FOUND` (`1`) fails `defra_node_new` with `failed to read peer key from keyring: system keyring error: host keyring get of 'peer-key' failed with status <status>`, and nothing is written to the store. A store that already holds a plaintext peer key is refused with `plaintext peerstore /p2p/local-peer-key requires explicit keyring migration`, and the keyring is left untouched. A store started with a keyring is refused without one: `this database requires its application-owned peer keyring`.

# `defra.node.ffi_peer_keyring.callbacks`: The host's callbacks and what their answers mean

- **Parents:** ["defra.node.ffi_peer_keyring"]
- **Scenarios:** ["peer_keyring::tests::keys_round_trip_through_the_host_callbacks", "peer_keyring::tests::a_host_failure_is_an_error_never_a_missing_key", "peer_keyring::tests::the_three_callbacks_are_set_together", "peer_keyring::tests::node::a_node_refuses_part_of_a_keyring"]

The three callbacks are set together or not at all; setting only some fails `defra_node_new` with `peer_keyring_get, peer_keyring_set and peer_keyring_delete must be set together`. Each returns `0` on success. Get writes the key into a buffer of `DEFRA_KEYRING_MAX_KEY_LEN` (4096) bytes and reports its length; a longer key is an error (`host keyring key '<name>' is <n> bytes, more than the 4096 a key may hold`). Get and delete return `DEFRA_KEYRING_NOT_FOUND` only for a missing key; any other non-zero status is a failure (`host keyring <get|set|delete> of '<name>' failed with status <status>`), never a missing key. The key the node reads back is zeroed when it is dropped.
