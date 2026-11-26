# `defra.node.start.no_nameservers`: A libp2p node starts when the host lists no DNS nameservers

- **Parents:** ["go.node"]
- **Scenarios:** ["node::resolver::a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers", "node::resolver::a_libp2p_node_starts_when_resolv_conf_does_not_parse"]

A node started with libp2p networking starts, listens and answers requests when the host's resolver configuration (`/etc/resolv.conf` on unix) lists no nameservers or has lines the resolver cannot parse: `PeerInfo` returns the addresses it listens on and an introspection query succeeds. Startup never fails with `no nameservers found in config`.

Which nameservers the node then uses (the host's accepted lines, or Go's `127.0.0.1:53` and `[::1]:53` fallback, never a public DNS service) is internal to the libp2p transport and no client can observe it, so it is a unit-level contract: the module docs of `crates/database/p2p/src/host/transport/resolver.rs`, its unit tests, and `proofs/lean/DnsResolverFallback.lean`.

# `defra.write_validator`: An embedder's validator of local writes

- **Parents:** ["defra"]

An embedder may give a node a `query::access_hooks::WriteValidator` (`AccessHooks::with_write_validator`), asked `validate_write` for every GraphQL mutation a local client sends, before ACP checks and before any block is built. `Err(message)` refuses the mutation with that message; `Ok(())` lets it run.

# `defra.write_validator.once`: The validator is asked once per mutation

- **Parents:** ["defra.write_validator"]
- **Scenarios:** ["write_validation_once::a_write_validator_is_asked_once_per_mutation"]

An autocommit `add`, `update` (by DocID or by filter) or `delete` asks the write validator once, while the request is prepared, and runs with that approval: its first attempt finds the answer and does not end asking for it again. A refusal is asked once too and fails the mutation with the validator's message, such as `a note needs a body`. The validator is asked again only when the attempt's question differs from the one preparation asked (other target documents, another collection version).
