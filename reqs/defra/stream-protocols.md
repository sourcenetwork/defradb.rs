# `defra.stream_protocols`: An embedder's own stream protocols over iroh

- **Parents:** ["defra"]

An application embedding a node registers stream protocols on `NodeBuilder::with_stream_protocol` and opens them with `Node::open_p2p_stream`. Their streams ride the iroh connection the node already shares with each peer, under the same inbound allowlist, revocations and per-priority budgets as Defra's own protocols. It is an embedder API with no HTTP, CLI or Go surface, so its scenarios run under the in-process client.

# `defra.stream_protocols.registration`: Only a protocol the node can serve is registered or opened

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::a_node_refuses_a_protocol_it_cannot_serve", "net::stream_protocols::opening_a_protocol_the_opener_cannot_send_fails_before_the_network"]

A node refuses to start on a protocol whose tag is empty or longer than 64 bytes (`a tag is 1 to 64 bytes`), starts with `/defra-iroh/` (`is in Defra's reserved /defra-iroh/ namespace`), or is given twice (`is registered twice`); whose request limit is above one peer's share of its priority's inbound budget (`one peer may hold at most <share> bytes`); or whose deadline is zero or longer than a day (`a deadline is more than zero and at most 86400s`). Each error starts `invalid configuration: stream protocol "<tag>"`, wrapped as `iroh peer failed to start: …`. A 64-byte tag, a limit of exactly the share and a deadline of exactly a day are accepted. A node with no iroh transport fails to build with `stream protocols require the iroh transport`. Opening a reserved tag fails before anything is sent, and so does a request larger than the protocol's limit, with its own error rather than a configuration one (`a request of <n> bytes is larger than stream protocol "<tag>" accepts (<max> bytes)`), and opening from a node without P2P fails with `P2P is not enabled for this node`.

# `defra.stream_protocols.round_trip`: A request and its reply cross between two nodes

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::a_request_and_its_reply_round_trip_without_connecting_first", "net::stream_protocols::a_handler_answers_from_its_nodes_database"]

A node that never connected to the serving node opens a registered protocol at the server's address. The server's handler gets the opener's authenticated peer ID and the request, and its reply reaches the opener. A handler that runs the request as a GraphQL query through its node's request path (`Node::execute`) answers with that query's data.

# `defra.stream_protocols.two_way`: The connection a stream dials serves both nodes

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::the_connection_a_stream_dials_serves_both_nodes"]

When `open_p2p_stream` has to dial, it dials as `connect_peer` does. Afterwards the serving node can open a protocol back to the opener by its peer ID alone, over that same connection.

# `defra.stream_protocols.admission`: A stream is admitted as Defra's own are

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::a_tag_nobody_registered_is_stopped_and_the_connection_keeps_serving", "net::stream_protocols::a_peer_the_allowlist_refuses_is_served_once_allowed"]

A stream whose tag nobody registered is stopped before its request is read (`stream reset by the peer with code 2`), and the connection keeps serving registered tags. A peer the node's explicit inbound allowlist doesn't name can't open a protocol; once `allow_p2p_peer` admits it, the same request is served.

# `defra.stream_protocols.budget`: A request's room in the budget lasts as long as the request

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::a_hung_handler_that_dropped_its_request_holds_no_room", "net::stream_protocols::a_handler_that_keeps_its_request_holds_its_room"]

A request's room in the peer's share of its priority's inbound budget is held by the request handed to the handler and comes back when the handler drops it. Two requests, each the peer's whole share, both reach handlers that dropped their request and hang. While a handler keeps such a request, a second one is refused with `stream reset by the peer with code 4`. A handler can therefore hold no more than the peer's share however many streams it serves, and never longer than its deadline.

# `defra.stream_protocols.deadline`: A handler past its deadline is cut

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::a_handler_past_its_deadline_is_cut_with_code_7"]

A handler still running at its protocol's deadline is dropped, and the opener's read fails with `stream reset by the peer with code 7`. A send half dropped unfinished, by the node or by a handler, resets the stream the same way rather than finishing it.

# `defra.stream_protocols.revocation`: Revoking a peer cuts its open protocol streams

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::denying_a_peer_cuts_its_open_streams"]

After `deny_p2p_peer`, a stream the revoked peer holds open to a hung handler ends with an error on the peer's side.

# `defra.stream_protocols.shutdown`: Closing the serving node cuts its streams

- **Parents:** ["defra.stream_protocols"]
- **Scenarios:** ["net::stream_protocols::closing_the_serving_node_cuts_its_open_streams"]

When the serving node shuts down, a stream an opener holds open to a hung handler ends with an error on the opener's side.
