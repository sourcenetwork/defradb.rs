# `defra.node.start.no_nameservers`: A libp2p node starts when the host lists no DNS nameservers

- **Parents:** ["go.node"]
- **Scenarios:** ["node::resolver::a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers", "node::resolver::a_libp2p_node_starts_when_resolv_conf_does_not_parse"]

A node started with libp2p networking starts, listens and answers requests when the host's resolver configuration (`/etc/resolv.conf` on unix) lists no nameservers or has lines the resolver cannot parse: `PeerInfo` returns the addresses it listens on and an introspection query succeeds. Startup never fails with `no nameservers found in config`.

Which nameservers the node then uses (the host's accepted lines, or Go's `127.0.0.1:53` and `[::1]:53` fallback, never a public DNS service) is internal to the libp2p transport and no client can observe it, so it is a unit-level contract: the module docs of `crates/database/p2p/src/host/transport/resolver.rs`, its unit tests, and `proofs/lean/DnsResolverFallback.lean`.

# `defra.node.vector_memory_budget`: A node-level budget caps what vector indexes keep in memory

- **Parents:** ["go.node"]
- **Scenarios:** ["node::vector_memory_budget::a_node_with_no_vector_memory_returns_the_nearest_documents"]

`defradb start --vector-memory-budget <bytes>`, the `datastore.vector_memory_budget` key of `config.yaml`, and `NodeBuilder::with_vector_memory_budget` set how many bytes the node's vector indexes may keep in memory between searches, all indexes on the node together: DiskANN's resident codes, the codes a search reads on a miss, and the copies of pending lists. Unset, the budget is `268435456` (256 MiB). What does not fit is read from storage when a search needs it, so a smaller budget makes searches slower, mostly at the tail, and never changes what they return: under any budget, `0` included, a similarity query returns the same documents in the same order with the same scores. Go has no such setting at the pinned v1.2.0 (sourcenetwork/defradb#5074 is still open), so the key and its default are the Rust node's own, and there is no per-index override.
