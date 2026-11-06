# `defra.stream_protocols`: An embedder's own stream protocols over iroh

- **Parents:** ["defra"]

An application embedding a node registers stream protocols on `NodeBuilder::with_stream_protocol` and opens them with `Node::open_p2p_stream`. Their streams ride the iroh connection the node already shares with each peer, under the same inbound allowlist, revocations and per-priority budgets as Defra's own protocols.

# `defra.stream_protocols.registration`: Only a protocol the node can serve is registered

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocols::tests::defra_tags_and_the_reserved_prefix_are_refused", "iroh::stream_protocols::tests::a_tag_outside_one_to_64_bytes_is_refused", "iroh::stream_protocols::tests::a_limit_above_a_peers_share_is_refused", "iroh::stream_protocols::tests::a_zero_or_unbounded_deadline_is_refused", "iroh::stream_protocols::tests::a_tag_registered_twice_is_refused_and_each_tag_finds_its_own", "iroh::stream_protocol_tests::a_node_refuses_to_start_on_an_invalid_or_repeated_protocol", "iroh::stream_protocol_tests::opening_a_reserved_tag_or_an_oversized_request_fails_locally", "a_node_without_iroh_refuses_stream_protocols"]

A node refuses to start on a protocol whose tag is empty or longer than 64 bytes, starts with `/defra-iroh/`, or is given twice; whose request limit is above one peer's share of its priority's inbound budget; or whose deadline is zero or longer than a day. Each fails with `invalid configuration: stream protocol "<tag>" …` naming the problem (an embedded node wraps it as `iroh peer failed to start: …`). Opening such a protocol, or a request larger than its limit, fails the same way without touching the network. A node whose transport is not iroh fails to build with `stream protocols require the iroh transport`, and `open_p2p_stream` on a node without P2P fails with `P2P is not enabled for this node`.

# `defra.stream_protocols.round_trip`: A request and its reply cross between two nodes

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::a_request_and_its_reply_round_trip_without_dialling_first", "an_apps_stream_protocol_round_trips_between_embedded_nodes"]

A node that never connected to the serving node opens a registered protocol with the server's address; the server's handler gets the opener's authenticated peer ID and the request, and its reply reaches the opener. A handler that queries its node through `Node::execute` answers with that query's data.

# `defra.stream_protocols.two_way`: The connection a stream dials serves both nodes

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::the_connection_a_stream_dials_accepts_the_peers_streams"]

When `open_p2p_stream` has to dial, it dials as `connect_peer` does: the serving node's own requests over that connection (an identity request) are answered.

# `defra.stream_protocols.admission`: A stream is admitted as Defra's own are

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::an_unregistered_tag_is_still_refused", "iroh::stream_protocol_tests::a_peer_the_allowlist_refuses_cannot_open_the_protocol"]

A stream whose tag nobody registered is stopped with code 2 before its request is read, and the connection keeps serving registered tags. A peer the inbound allowlist refuses never reaches a handler; once allowed, the same request is served.

# `defra.stream_protocols.budget`: A running handler holds none of its peer's budget

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::a_stuck_handler_holds_none_of_the_peers_budget"]

A request's room in the inbound budget is given back once the request is read, before its handler runs. Two requests each as large as the peer's whole share are both handed to handlers that never return, with no busy reset and nothing held.

# `defra.stream_protocols.deadline`: A handler past its deadline is cancelled

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::a_handler_past_its_deadline_is_dropped_and_its_stream_reset"]

A handler still running at its protocol's deadline is dropped, and the opener's read fails with `stream reset by the peer with code 7`. A send half dropped unfinished, by the node or by a handler, resets the stream the same way rather than finishing it.

# `defra.stream_protocols.revocation`: Revoking a peer cuts its protocol streams

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::denying_a_peer_cuts_its_open_protocol_streams"]

`deny_peer` closes the peer's connections, which drops the handlers serving its open streams; the peer's reads on them fail.

# `defra.stream_protocols.shutdown`: Shutdown cancels running handlers

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["iroh::stream_protocol_tests::shutdown_cancels_a_running_handler"]

Shutting the endpoint down drops every handler still running.
