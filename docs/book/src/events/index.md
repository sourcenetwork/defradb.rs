# Events

Clients consume change through subscriptions, and the ordering guarantee is the whole
design: **events are published in commit-receipt order** (`CommitReceipt.seq`), the
same order the engine made commits durable and visible. There is no separate sequencer
to drift.

## The bus

The event bus (`crates/events`) publishes named events: `Update` (a document create,
update or delete, with the block payload), `Merge` / `MergeComplete` (replicated
merges), peer and connection events, replicator completions, ACP height/cache events,
quarantine events, and unique-index conflicts. Subscribers subscribe by name;
GraphQL subscriptions are the client surface.

Events fire from **ordered post-commit callbacks**: the transaction registers an
ordered `on_commit` hook, the engine's receipt stream dispatches ordered callbacks in
`seq` order across threads ([`crates/storage/src/backends/regolith/event_order.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/storage/src/backends/regolith/event_order.rs)), and
a discarded transaction publishes nothing. A commit's events follow its durability,
never precede it.

## The two subscription shapes

- **The event stream** (`Update` and friends): a revision stream for consumers that
  want every commit. Bounded channels; a shared dropped-counter; a non-zero
  `check_and_reset_dropped()` tells the client to resync rather than silently miss
  revisions.
- **Document change notifications**: coalescing, lossless *invalidation*: one pending
  flag per document (swapped in one RCU), not a queue of revisions. A slow consumer
  gets "documents X, Y changed" once, not a backlog it cannot drain; it then re-reads.
  Capacity overflow sets `resync_required`, again visibly.

This split is deliberate: a projection builder wants the revision stream; a cache
wants the invalidation signal. Forging a durable, gap-free stream of *arrivals* (as
opposed to revisions) is the [arrival journal](../documents/arrival-journal.md), which
clients consume by cursor rather than subscription.

## P2P forwarding

Committed writes are handed to the replication broadcaster (push-to-replicators plus
gossip): the broadcast marker is written durably, and only then does the committing
transaction report success, so "committed" always implies "at least queued for
replication" (see [Push and pull](../p2p/sync.md)).

> **Why receipt order and not wall-clock or per-source order.** Two workers commit at
> the same microsecond on different cores: a timestamp cannot order them, and each
> source ordering them "itself first" is not an order at all. The commit receipt is
> the one sequence the engine already totally ordered, and ordering events by it makes
> the event stream a faithful log of what became visible, when, without a second
> coordinator to keep honest.
