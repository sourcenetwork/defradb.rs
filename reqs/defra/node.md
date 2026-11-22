# `defra.node.start.no_nameservers`: A libp2p node starts when the host lists no DNS nameservers

- **Parents:** ["go.node"]
- **Scenarios:** ["node::resolver::a_libp2p_node_starts_when_resolv_conf_lists_no_nameservers", "node::resolver::a_libp2p_node_starts_when_resolv_conf_does_not_parse"]

A node started with libp2p networking starts, listens and answers requests when the host's resolver configuration (`/etc/resolv.conf` on unix) lists no nameservers or cannot be read, as Go's node does: `PeerInfo` returns the addresses it listens on and an introspection query succeeds. Its `/dns`, `/dns4` and `/dns6` addresses then resolve through `127.0.0.1:53` and `[::1]:53`, over UDP and TCP, the nameservers Go's resolver falls back to, never a public DNS service. The node logs one warning naming why the host's configuration was not used: `no usable DNS nameservers in the host resolver configuration, using 127.0.0.1:53 and [::1]:53`, with the cause (`no nameservers found in config` when the file lists none). Startup never fails with `no nameservers found in config`.
