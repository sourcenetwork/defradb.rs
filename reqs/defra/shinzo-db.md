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
- **Scenarios:** ["db::collection::purge::prune_deletes_history_and_keeps_a_block_another_document_owns", "db::collection::purge::purging_every_owner_together_deletes_the_shared_block", "db::collection::purge::a_block_a_merge_adopts_during_the_purge_is_kept", "shinzo::purge::shinzo_a_pruned_document_can_be_added_again"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With `pruneHistory: true`, every block reachable from the document's heads is deleted: for a document created as `{"name":"Alice","age":40}` and updated once, both composite blocks, both first-version field blocks and the updated field block (five blocks) are gone. A field block is content-addressed, so two documents with the same `name` share one `name` block. Purging one of them keeps the shared block; purging the other later removes it. Purging both in one call removes it as well, whether the shared block is a current head or is reached only through an older version. A block another document comes to own while the purge runs (a merge that finds the block already stored) is kept.

# `go.shinzo.purge.no_conflict`: Pruning history does not conflict with a concurrent block store

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["db::collection::purge::a_sync_storing_a_walked_block_again_does_not_fail_the_purge"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A `PurgeByDocIDs(..., true)` commits successfully even when another transaction stores one of the blocks the purge is walking (as a P2P sync recreating a block would) and commits first.

# `go.shinzo.purge.count`: Purged documents are counted once they commit

- **Parents:** ["go.shinzo.purge"]
- **Scenarios:** ["db::collection::purge::the_count_names_each_stored_document_once", "shinzo::purge::shinzo_purge_counts_each_stored_document_once", "shinzo::purge::shinzo_a_discarded_purge_removes_nothing", "shinzo::purge::shinzo_a_purge_in_a_transaction_lands_on_commit"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A DocID that names no document is not an error and is not counted: `PurgeByDocIDs([alice, bob, alice])` succeeds and counts 2. A purge run in a caller's transaction removes nothing until that transaction commits, and nothing at all if it is discarded. defradb.rs returns the count to the caller; the periodic `purge stats` log line is `go.shinzo.telemetry.merge.report`.
