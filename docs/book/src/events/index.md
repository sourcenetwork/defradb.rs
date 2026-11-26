# Events

Clients consume change through subscriptions, and the ordering guarantee is the whole
design: **events are published in commit-receipt order** (`CommitReceipt.seq`), the
same order the engine made commits durable and visible. There is no separate sequencer
to drift.

## The bus

The event bus (`crates/foundation/events`) publishes named events: `Update` (a document create,
update or delete, with the block payload), `Merge` / `MergeComplete` (replicated
merges), peer and connection events, replicator completions, ACP height/cache events,
quarantine events, and unique-index conflicts. Subscribers subscribe by name;
GraphQL subscriptions are the client surface.

Events fire from **ordered post-commit callbacks**: the transaction registers an
ordered `on_commit` hook, the engine's receipt stream dispatches ordered callbacks in
`seq` order across threads ([`crates/engine/storage/src/backends/regolith/event_order.rs`](https://github.com/sourcenetwork/defradb.rs/blob/main/crates/engine/storage/src/backends/regolith/event_order.rs)), and
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

## Delivery contract

What each surface promises a consumer, answering the same seven questions for each.
Every answer cites the test or model invariant that holds it; TBD marks a question
with no evidence yet. Order is stated per collection only: each journal is per
collection, and events carry no sequence number, so no surface offers a consumer a
total order it can check across collections. The models are
[`ChangeDelivery.tla`](https://github.com/sourcenetwork/defradb.rs/blob/main/proofs/tla/ChangeDelivery.tla)
(publisher, journal and subscriber) and
[`EventOrder.tla`](https://github.com/sourcenetwork/defradb.rs/blob/main/proofs/tla/EventOrder.tla)
(the receipt-ordered dispatch).

### The supported pattern

Subscribe first, to document change notifications or to a GraphQL subscription of
the collection with no `docID`, `docIDs` or `filter` argument (one with any of them wakes
only for the documents it names or matches, so an update to another can wait unannounced). Then,
once the subscription request has returned its stream (never concurrently with it),
read `_documentUpdates` from the saved cursor up to `head`, and save the cursor with
what the read produced. Read again on every wake-up and every loss signal (a set
`resync_required`, an ended stream), subscribing again first when the stream ended.
A subscriber that does this observes every committed update of the collection, local
or merged, that it is allowed to see, exactly once and in commit order, with no rescan:
`ChangeDelivery.tla` `INV_ExactlyOnce`, `INV_CommitOrder` and `EventuallyObserved`
(GREEN `MC_ChangeDelivery_Green_Closing.cfg` and `MC_ChangeDelivery_Green_Coalescing.cfg`).
Exactly once covers the journal reads and the saved cursor, not what a consumer does with
a row: one that applies an update and stops before it saves the cursor reads that update
again, so its effect and the cursor save must commit together, or the effect must be
idempotent.
In a collection with a `@policy`, a row is not shown when its document is deleted or
purged, or when the reader may not read the document; the cursor moves past it
anyway, so a row hidden when the reader reads past it is skipped for good, even if
access is granted later (`defra.events.delivery.visibility`; `INV_HiddenNotShown`,
`Red_GateOff`, `Red_HiddenStalls`). An update observed before its document was
deleted stays observed. In a collection without a policy (other than a `@chunk` collection, whose rows follow their parent document's access) every row is shown, a
deleted document's included.
Reading before subscribing can miss an update for good (`Red_ReadBeforeSubscribe`).
The raw event subscription is not a wake-up source for this pattern: its loss counter
wakes nobody (`Red_CounterNoWake`).

### Event bus through the embedded API (`Node::subscribe`)

| Question | Answer | Evidence |
|---|---|---|
| Delivery | At most once. A full buffer drops the event and adds one to the subscription's dropped counter; the subscription stays open. | `crates/foundation/events/tests/unit/channel_bus.rs` `ordinary_raw_subscription_remains_open_after_overflow` |
| Duplicates | TBD: whether one update's `Update` event can go out twice (`is_retry`, a block merged again by a fresh handler). | none yet |
| Order within a collection | Commit-receipt order, for local writes and P2P merges alike: both register their events as ordered post-commit callbacks (`register_ordered_events`), which go out in receipt order. | `EventOrder.tla` `Ordered`, `INV_CommitOrder`; `crates/engine/storage/src/backends/regolith/event_order.rs` `a_later_receipt_waits_for_an_earlier_commit_still_in_flight`; `event_order.rs` `reversed_concurrent_receipts_dispatch_in_seq_order` |
| Order across collections | Not stated: events carry no sequence number. | none |
| Restart | Nothing survives: the bus is in memory and keeps no cursor. | `proofs/survey/events.md` (live-only) |
| Resubscription | A new subscription sees only events published after it. Subscribe, then read state; reading first can miss an update. | `ChangeDelivery.tla` `Red_ReadBeforeSubscribe` |
| Gap detection | The dropped counter, read with `check_and_reset_dropped()`. It is raised for every drop, but it wakes no reader: a reader that drained the buffer and read zero just before the count landed waits with nothing to wake it. | `crates/foundation/events/tests/unit/channel_bus.rs` `raw_overflow_recorded_after_a_drain_is_counted_but_wakes_no_reader`; `ChangeDelivery.tla` `Red_CounterNoWake` |
| Combination | Not supported as the wake-up source of the pattern above. | `ChangeDelivery.tla` `Red_CounterNoWake` |

### GraphQL subscription over SSE

| Question | Answer | Evidence |
|---|---|---|
| Delivery | At most once, with the loss signalled: an overflow ends the stream before any result queued behind it. | `crates/surfaces/http/tests/graphql_sse_recovery.rs` `overflow_ends_the_graphql_stream_before_a_queued_result`; `SseDelivery` `INV_DetectedLossEnds` |
| Duplicates | TBD, as for the event bus it is built on. | none yet |
| Order within a collection | Commit-receipt order, as the event bus. | `EventOrder.tla` `Ordered`, `INV_CommitOrder` |
| Order across collections | Not stated. | none |
| Restart | The stream ends with the process; nothing resumes it. | `proofs/survey/events.md` (live-only) |
| Resubscription | A new stream sees only what is published after it. Subscribe, then read the journal. | `ChangeDelivery.tla` `Red_ReadBeforeSubscribe` |
| Gap detection | The stream ends, and the end wakes a reader that already drained what it had. | `crates/foundation/events/tests/unit/channel_bus.rs` `overflow_wakes_reader_that_drained_before_loss_was_recorded`; `SseDelivery` `LossEndsStream` |
| Combination | Supported: on every result and on the end of the stream (subscribing again first), read the update journal from the cursor. | `ChangeDelivery.tla` `MC_ChangeDelivery_Green_Closing.cfg` |

### Document change notifications (`subscribe_document_changes`, `subscribe_collection_changes`)

| Question | Answer | Evidence |
|---|---|---|
| Delivery | A lossless invalidation, not a revision stream: the changed documents' state is written first, then the reader is woken. Revisions of one document coalesce into one pending entry. | `crates/foundation/events/tests/document_changes.rs` `history_revisions_coalesce_without_changing_raw_events`, `publishing_races_with_draining_without_losing_a_wake` |
| Duplicates | Each published update is counted in exactly one batch; a document is reported again only for a later update. | `crates/foundation/events/tests/document_changes.rs` `publishing_races_with_draining_without_losing_a_wake` (10 000 published, 10 000 counted), `distinct_document_overflow_is_one_bounded_resync_then_recovers` |
| Order within a collection | Not a sequence: a wake-up names documents, not an order. | none |
| Order across collections | Not stated. | none |
| Restart | Nothing survives. | `proofs/survey/events.md` (live-only) |
| Resubscription | Subscribe, then read; a subscription filtered to collections is woken only by them. | `crates/foundation/events/tests/document_changes.rs` `collection_scope_ignores_other_collections`; `ChangeDelivery.tla` `Red_ReadBeforeSubscribe` |
| Gap detection | Too many distinct pending documents clear the set and set `resync_required`, once, with a wake-up. | `crates/foundation/events/tests/document_changes.rs` `distinct_document_overflow_is_one_bounded_resync_then_recovers` |
| Combination | Supported: the recommended wake-up source for the pattern above. P2P merges feed it through the same `Update` event as local writes (`Message::as_update`); a test through a merge is TBD. | `ChangeDelivery.tla` `MC_ChangeDelivery_Green_Coalescing.cfg`, `Red_WakeBeforeState` |

### Arrival journal (`_documentArrivals`)

| Question | Answer | Evidence |
|---|---|---|
| Delivery | Durable, read by cursor. One row per document, however it arrived. In a collection with a `@policy`, a reader is handed only the rows of documents it may read that are neither deleted nor purged, through the update journal's gate (`Gate::admits`); in a `@chunk` collection, only the rows of chunks whose parent it may read and whose parent's current text uses them. | `reqs/defra/change-delivery.md` `defra.events.arrivals.visibility`, `defra.events.arrivals.chunk`; `tools/integration-test/tests/testo/subscription/delivery/arrivals.rs`, `tools/integration-test/tests/testo/chunk/journal_visibility.rs`; `crates/database/db/tests/read/arrival_transactions.rs` `rollback_is_atomic_and_duplicate_arrivals_do_not_advance`; `CommitOrderedAppend.tla` (regolith) |
| Duplicates | None: the document is the once key, so a create racing a merge of it keeps one row. | `crates/database/db/tests/read/arrival_transactions.rs` `rollback_is_atomic_and_duplicate_arrivals_do_not_advance` |
| Order within a collection | Commit order, dense cursors from 1. | `crates/database/db/tests/read/arrival_transactions.rs` `reversed_concurrent_commits_receive_cursors_in_commit_order` |
| Order across collections | None: each collection has its own cursors. | none |
| Restart | The journal and a saved cursor survive a reopen. Retention across truncate, purge, backup restore and `drop_all`: TBD. | `crates/database/db/tests/read/arrival_transactions.rs` `cursor_survives_database_reopen` |
| Resubscription | A read is a snapshot: the stored rows it covers are a dense prefix up to `head`. The rows a reader is handed skip those of documents it may not read (see Gap detection). | `CommitOrderedAppend.tla` (regolith) snapshot prefix |
| Gap detection | The stored rows are dense; in a read without `docID`, a row missing from storage in the read range is an error, never skipped. A `docID` read takes each cursor from the document's once key and does not read the row, so it does not detect a missing row. The cursors a reader is handed are not: a row of a document it may not read, or of a deleted or purged document in a collection with a `@policy`, is passed silently and `next` moves past it (`arrivals::read`, `execute_arrivals_query`). A cursor past `head` fails with `arrival cursor is beyond this node's collection head` (a test through a client is TBD). | `crates/database/db/src/event/arrivals.rs` |
| Combination | Read on wake-up, as the update journal. | `ChangeDelivery.tla` (as for the update journal) |

### Update journal (`_documentUpdates`)

| Question | Answer | Evidence |
|---|---|---|
| Delivery | Durable, read by cursor: one row per update that changed its document, local or merged. A create adds no row. In a collection with a `@policy`, a reader is handed only the rows of documents it may read that are neither deleted nor purged; in a `@chunk` collection, only the rows whose parent document it may read; the others are hidden from it. | `reqs/defra/change-delivery.md` `defra.events.delivery.merge-once`, `defra.events.delivery.visibility`, `defra.events.delivery.chunk`; `tools/integration-test/tests/testo/subscription/delivery/merge_once.rs`, `visibility.rs`, `tools/integration-test/tests/testo/chunk/journal_visibility.rs`; `DocumentUpdateJournal.tla` `INV_Once` |
| Duplicates | None: (collection, document, block) is the once key, so a block merged a second time (by a handler that does not yet know it was applied) adds no row. A block the node already applied is skipped before the merge, from the durable head set (`DbMergeHandler::composite_was_applied`), so the once key is held by a database-level test only; no client-level test reaches a second merge. | `crates/database/db/tests/merge/update_journal.rs` `a_block_merged_again_by_another_handler_adds_no_second_row`; `ChangeDelivery.tla` `Red_ReMergeAppends` |
| Order within a collection | Commit order, dense cursors from 1, for local writes and merges. | `defra.events.delivery.order`; `crates/database/db/tests/read/update_transactions.rs` `commit_order_rollback_and_replayed_updates` |
| Order across collections | None: each collection has its own cursors. | `ChangeDelivery.tla` (one collection; out of scope) |
| Restart | The journal and a saved cursor survive a reopen with no gap and no duplicate. Truncate at e222d6b deletes no `/arrival/updates/` key (`collection/ops/truncate/all.rs`, `filtered.rs`), so the rows of truncated documents stay; what a reader is handed for them, and retention across purge, backup restore and `drop_all`: TBD. | `defra.events.delivery.resume`; `crates/database/db/tests/read/update_transactions.rs` `updates_and_replay_checkpoint_survive_reopen` |
| Resubscription | A read is a snapshot of rows up to `head`; a reader that saves its cursor with its effect resumes exactly. | `ChangeDelivery.tla` `Red_CursorNotDurable` |
| Gap detection | The stored rows are dense: a row missing from storage is an error, and a cursor past `head` fails with `update cursor is beyond this node's collection head` through every client. The cursors a reader is handed are not dense: a row hidden from it (see Delivery) is passed silently, `next` moves past it, and no error is raised. | `defra.events.delivery.beyond-head`, `defra.events.delivery.visibility`; `crates/database/db/tests/read/update_transactions.rs` `a_scan_is_closed_when_a_row_is_missing`; `crates/database/db/tests/read/update_visibility.rs` `a_deleted_private_documents_updates_are_shown_to_nobody` |
| Combination | The durable half of the pattern above, for the rows the reader is allowed to see. | `ChangeDelivery.tla` `INV_ExactlyOnce`, `INV_CommitOrder`, `INV_HiddenNotShown`, `EventuallyObserved` |

### What counts as breaking

Weakening any answer above is a breaking change: a surface's delivery class (at most
once with a signal, lossless invalidation, durable), its order, its loss signal, or how
long a journal cursor stays valid. Such a change says so in its PR and updates this
section, the `defra.events.delivery` requirements and the models in the same change.

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
