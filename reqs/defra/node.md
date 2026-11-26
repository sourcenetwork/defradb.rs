# `defra.node.start.no_nameservers`: A libp2p node starts when the host lists no DNS nameservers

- **Parents:** ["go.node"]
- **Scenarios:** ["node::resolver::a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers", "node::resolver::a_libp2p_node_starts_when_resolv_conf_does_not_parse"]

A node started with libp2p networking starts, listens and answers requests when the host's resolver configuration (`/etc/resolv.conf` on unix) lists no nameservers or has lines the resolver cannot parse: `PeerInfo` returns the addresses it listens on and an introspection query succeeds. Startup never fails with `no nameservers found in config`.

Which nameservers the node then uses (the host's accepted lines, or Go's `127.0.0.1:53` and `[::1]:53` fallback, never a public DNS service) is internal to the libp2p transport and no client can observe it, so it is a unit-level contract: the module docs of `crates/database/p2p/src/host/transport/resolver.rs`, its unit tests, and `proofs/lean/DnsResolverFallback.lean`.

# `defra.node.start.go_datastore`: A Go DefraDB datastore is refused at start with what to do

- **Parents:** ["go.node"]
- **Scenarios:** ["node::go_datastore::a_go_badger_datastore_is_refused_with_what_to_do", "crates/entry-points/cli/tests/go_datastore.rs::the_binary_refuses_a_go_datastore_before_start_and_storage_stats"]

A node whose data directory holds the Badger datastore Go DefraDB writes there (a `MANIFEST` that starts with Badger's magic text `Bdgr`, or a `KEYREGISTRY` file), as a rootdir Go DefraDB created does, does not start. It fails before the storage engine opens the directory, so nothing in it changes, with `the data directory '<path>' holds a Badger datastore written by Go DefraDB, which defradb.rs cannot read; start this node on an empty data directory (another --rootdir or datastore.path), add the same schema to it first, then move the documents with Go's `defradb client backup export` (run against Go DefraDB on the old rootdir) and this node's `defradb client backup import``. The schema step is named because Go's export holds documents and not the schema, and this node's import looks each collection up by name, so it would fail on the first one. A read-only open of such a directory fails with the same error, and `defradb storage stats`, which opens the data path read-only, fails with it too. It never fails with the storage engine's `MANIFEST does not begin with the REGOMAN stamp`.

`defradb start --rootdir <go rootdir>` opens `config.data_path()` in its own process, and `crates/entry-points/cli/tests/go_datastore.rs` runs the binary for both commands against a rootdir whose `data` folder holds Badger's `MANIFEST` header: it asserts the non-zero exit, the message above on stderr, and that the folder still holds only that `MANIFEST` with its bytes unchanged.
