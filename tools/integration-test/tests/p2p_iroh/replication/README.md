# Replication Tests

51 passing, 0 ignored.

## Files

- `adversarial/` — An in-process iroh peer pushes hand-built blocks to a real node: a forged signature and bytes not matching their CID are refused, and a genesis pushed under another document's ID merges only under the ID derived from its own CID; an author's signature survives a relay hop. Under local ACP, updates and deletes to a document registered on the node are judged by each composite's own verified signer: a non-writer's, an unsigned, and a non-deleter's commit are refused, as is an owner-signed child over an attacker-signed ancestor; a granted writer's update merges, and a replicated (unregistered) document stays public. `blocks.rs` builds every block, `peer.rs` pushes them, `protected.rs` sets up the local-ACP node.
- `collection_sub.rs` — Collection subscription: add/remove/get P2P collections, error cases (all pass)
- `document.rs` — Document subscription: single/multi-doc sync via iroh (all pass)
- `document_sub.rs` — Document-level subscriptions: add/remove/sync, error handling (all pass)
- `replication.rs` — Core replication: batch, update, delete, GraphQL filter queries over replicated data (all pass). Note: the filter test reads replicated data with a `filter:` argument; it does NOT cover replication-side filtering (a replicator predicate gating which documents are pushed). Filtered-replication coverage over iroh lives in `tools/integration-test/tests/p2p/filtered_replication.rs` (the `*_iroh` tests).
- `replicator.rs` — Replicator lifecycle: CRUD, CRDT counters, restart persistence (all pass)
