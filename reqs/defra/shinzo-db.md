# `go.shinzo.purge.removes_document`: A purged document leaves nothing to find

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["shinzo::purge::shinzo_purge_removes_the_document", "shinzo::purge::shinzo_purge_removes_the_unique_index_entry"]
- **Go-Ref:** "shinzo-develop 2a899b860"

After `PurgeByDocIDs([alice], false)` on a `User { name: String, age: Int }` collection, a full scan of `User` returns no row for alice and no trace of a document that cannot be resolved. With a unique index on `name`, the purged document's index entries are gone too: adding a new document with `name: "alice"` afterwards succeeds instead of failing with `can not index a doc's field(s) that violates unique index`.

# `go.shinzo.purge.mappings`: Purge removes every DocID alias of the document

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["shinzo::purge::shinzo_purged_doc_id_no_longer_resolves", "db::collection::purge::purge_without_pruning_keeps_blocks_and_drops_the_owner_edges"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Once a document is purged its DocID no longer resolves on this node, and neither does any other DocID that pointed at the same document (an older alias). Looking any of them up returns not-found. No block's ownership record names a purged DocID any more, with or without `pruneHistory`: without it the blocks stay, owned only by the documents that still exist. Go keeps those owner edges when it does not prune, so its blocks stay owned by a document that no longer exists and the orphan sweep (`go.shinzo.orphan_gc`) can never reclaim them; defradb.rs drops them.

# `go.shinzo.purge.prune_history`: `pruneHistory` deletes every block only the purged documents own

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["db::collection::purge::prune_deletes_history_and_keeps_a_block_another_document_owns", "db::collection::purge::purging_every_owner_together_deletes_the_shared_block", "db::collection::purge::a_block_a_merge_adopts_during_the_purge_is_kept", "shinzo::purge::shinzo_a_pruned_document_can_be_added_again", "db::collection::purge::prune_keeps_the_composites_a_branchable_collection_links"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With `pruneHistory: true`, every block reachable from the document's heads is deleted: for a document created as `{"name":"Alice","age":40}` and updated once, both composite blocks, both first-version field blocks and the updated field block (five blocks) are gone. A field block is content-addressed, so two documents with the same `name` share one `name` block. Purging one of them keeps the shared block; purging the other later removes it. Purging both in one call removes it as well, whether the shared block is a current head or is reached only through an older version. A block another document comes to own while the purge runs (a merge that finds the block already stored) is kept. In a `@branchable` collection the collection DAG links every version's composite (one collection block per document commit), so it owns them: pruning keeps each composite and every block it links, in the caller's transaction as in its own, and the collection's `_commits` and its sync stay whole; only the purged owner edges go.

- **Go-Bug-Suspect:** "Go's prune deletes a `@branchable` collection's document composites while its collection DAG still links them, so the collection's `_commits` and a peer syncing it reach blocks that are gone; defradb.rs keeps them"

# `go.shinzo.purge.no_conflict`: Pruning history does not conflict with a concurrent block store

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["db::collection::purge::a_sync_storing_a_walked_block_again_does_not_fail_the_purge"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A `PurgeByDocIDs(..., true)` commits successfully even when another transaction stores one of the blocks the purge is walking (as a P2P sync recreating a block would) and commits first.

# `go.shinzo.purge.count`: Purged documents are counted once they commit

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["db::collection::purge::the_count_names_each_stored_document_once", "shinzo::purge::shinzo_purge_counts_each_stored_document_once", "shinzo::purge::shinzo_a_discarded_purge_removes_nothing", "shinzo::purge::shinzo_a_purge_in_a_transaction_lands_on_commit", "shinzo::purge::a_purge_in_a_transaction_is_invisible_until_commit"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A DocID that names no document is not an error and is not counted: `PurgeByDocIDs([alice, bob, alice])` succeeds and counts 2. A purge run in a caller's transaction removes nothing until that transaction commits, and nothing at all if it is discarded. defradb.rs returns the count to the caller; the periodic `purge stats` log line is `go.shinzo.telemetry.merge.report`.

# `go.shinzo.retention.rule`: The merge refuses what the rule refuses

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_merge_refuses_what_the_rule_refuses", "db::merge::retention::a_floor_of_zero_refuses_nothing_by_height", "db::merge::retention::an_uncovered_collection_or_a_node_without_a_rule_stores_everything"]
- **Go-Ref:** "shinzo-develop 2a899b860"

For `type Log { blockNumber: Int address: String }` with field `blockNumber` and floor 100, merging a replicated document:

- `blockNumber: 100` (at the floor) is refused with `replicated document is at or below the retention floor`;
- `blockNumber: 101` is stored;
- no `blockNumber`, or `blockNumber: -5`, is refused with `replicated document has no valid retention height` (a null, non-integer or negative value has no valid height);
- with a floor of 0 or less, nothing is refused by height (`blockNumber: 0` is stored), but a document with no valid height is still refused;
- a collection the rule does not cover, or a node with no rule, stores everything.

Both refusals read `<reason>: refused by the retention rule`, as Go's wrapped errors do. A refused document is not stored, its head is not marked merged, and its outcome is `Rejected`, never a dropped event. The `rejectedBelowFloor` and `rejectedNoHeight` counters are `go.shinzo.telemetry.merge.report`.

# `go.shinzo.retention.scope`: Branchable and unknown collections are not judged

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_floor_resolves_for_a_covered_collection_only", "db::merge::retention::a_branchable_collection_is_not_judged"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A covered collection resolves to its field and floor (`blockNumber`, 100). A `@branchable` collection is never judged, because one of its heads commits many documents at once. A collection ID this node does not hold, or a node with no rule, is not judged either.

# `go.shinzo.retention.delete`: A delete is applied below the floor

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_delete_is_applied_below_the_floor"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With floor 100, a replicated delete of a document whose `blockNumber` is 5 merges without error and its head is marked merged: deleted documents are not judged.

# `go.shinzo.retention.batch`: A refusal in a batch refuses only that document

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_refusal_in_a_batch_refuses_only_that_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Merging a batch of three Logs at heights 101, 100 and 150 against floor 100 returns the outcomes `[Merged, Rejected, Merged]`; the 101 and 150 documents are stored, the 100 one is not, and no drop is recorded.
