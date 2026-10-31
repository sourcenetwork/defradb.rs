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

# `go.shinzo.save_many.create_or_update`: Each document is added or updated

- **Parents:** ["go.shinzo.save_many"]
- **Scenarios:** ["shinzo::save_many::save_many_updates_a_stored_document_and_adds_a_new_one"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With `User { name: String, age: Int }` holding Alice (`age: 40`), saving `[{_docID: <alice>, age: 41}, {name: "Bob", age: 50}]` leaves Alice at `age: 41` with her name unchanged and adds Bob. defradb.rs returns the saved documents' DocIDs in the order given.

# `go.shinzo.save_many.atomic`: One document that fails saves none

- **Parents:** ["go.shinzo.save_many"]
- **Scenarios:** ["shinzo::save_many::one_document_that_fails_saves_none"]
- **Go-Ref:** "shinzo-develop 2a899b860"

When one document of the batch cannot be saved, the call fails with that document's error and no document of the batch is stored, including the ones before it: saving `[{name: "Bob"}, {name: "Carol", nickname: "C"}]` on a `User` with no `nickname` field fails, and neither Bob nor Carol exists afterwards.

# `go.shinzo.save_many.deleted`: A deleted document is not saved again

- **Parents:** ["go.shinzo.save_many"]
- **Scenarios:** ["shinzo::save_many::a_deleted_document_is_not_saved_again"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A document whose DocID names a document that was deleted fails the batch with `a document with the given ID has been deleted`, and nothing of the batch is stored.

# `go.shinzo.save_many.transaction`: In a caller's transaction, the batch lands on commit

- **Parents:** ["go.shinzo.save_many"]
- **Scenarios:** ["shinzo::save_many::a_batch_in_a_transaction_lands_on_commit", "shinzo::save_many::a_discarded_batch_stores_nothing", "shinzo::save_many::an_empty_batch_stores_nothing"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Saved in a caller's transaction, the documents are not visible outside it until it commits, and are never stored if it is discarded. An empty batch succeeds and stores nothing.

# `go.shinzo.kv_snapshot.format`: The snapshot stream is length-prefixed frames ending in a zero length

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::shinzo_kv_go_and_rust_write_the_same_snapshot", "db::merge::kv_snapshot::frames_read_back_and_a_cut_frame_is_an_error"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a stream written by either runtime is read by the other, frame for frame"

A snapshot is a sequence of frames `[key length: u32 big-endian][key][value length: u32 big-endian][value]`, ended by a key length of 0. A key's first byte names its store: `d` the datastore, `h` the headstore, `b` the blockstore; the rest is Go's key inside that store, its integers in Go's ascending uvarint (short ID 1 is the byte `0x89`). A `b` key is the block's CID bytes and its value is the block's bytes. A stream that ends cleanly before the end mark is complete; one that ends inside a frame fails with `the KV snapshot ends inside a frame`. Writing frames and reading them back gives the same frames in order, and reading stops at the end mark whatever follows it (`proofs/lean/KvSnapshot/Frames.lean`).

# `go.shinzo.kv_snapshot.export`: Export writes each readable document's values, heads and blocks

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::shinzo_kv_go_and_rust_write_the_same_snapshot", "shinzo::kv_snapshot::kv_export_without_doc_ids_exports_every_document", "db::merge::kv_snapshot::an_export_leaves_out_documents_its_caller_cannot_read"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "without history, a Rust export of documents a Go Shinzo node holds equally is Go's export byte for byte"

For each DocID, in the order given, the export writes the document's datastore entries (each field's value as CBOR under `/<collection short ID>/v/<doc short ID>/<field short ID>`, the collection version under field `v`, then each field's priority under `/p/`), then its heads (`/<doc short ID>/<field short ID or C>/<CID>`, the priority as the value), then its blocks: from each head, the head and every block it links, each once, depth first. With history (the default) the walk also follows each block's earlier versions and its signature and encryption blocks, so another node can replay the document; without it the blocks are Go's. With `datastoreOnly`, only the datastore entries are written. The export reads one snapshot, as the caller: a document the caller cannot read under document access control is left out, as one this node does not have, or has deleted, is. It returns the frames written, not counting the end mark, and the field mapping. A collection that does not exist fails with `collection not found`, and an empty name with `collection name can't be empty`.

- **Go-Bug-Suspect:** "`kv export` documents `--doc-ids` as 'exports all if omitted', but `ExportDocKVs` with no DocIDs writes only the end mark; defradb.rs exports every document of the collection the caller can read, a case the comparison with Go leaves out"
- **Go-Bug-Suspect:** "Go's export follows a block's field links only, never its earlier versions or its signature, so an updated document's first version, which names its DocID, is not in the snapshot; defradb.rs exports history unless asked not to"

# `go.shinzo.kv_snapshot.field_mapping`: The field mapping names the collection and every stored field's short ID

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::shinzo_kv_go_and_rust_write_the_same_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the JSON is the shape Go writes and reads"

The mapping is `{"collectionName": <name>, "collectionShortID": <the source's collection short ID>, "fields": [{"name": <field>, "fieldID": <global field ID>, "shortID": <the snapshot's field short ID>}, …]}`, one entry per field of the collection's current version that has a field ID, in the version's field order; virtual fields (the secondary side of a relation) have none and are left out. defradb.rs keeps no per-node field short IDs, so it numbers the fields from 1 in that order, as Go numbers a new collection's. It adds `"documents": [{"docID": …, "shortID": …}]`, each exported document's DocID; Go's decoder ignores members it does not know. `client.CollectionFieldMapping` (snake-case keys) is declared in Go but nothing reads or writes it, so it is not part of the format.

# `go.shinzo.kv_snapshot.import`: Import replays each document's blocks, and the documents read back the same

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::shinzo_kv_a_go_snapshot_imports_into_rust", "shinzo::kv_snapshot::shinzo_kv_a_go_snapshot_of_an_updated_document_has_no_history", "shinzo::kv_snapshot::shinzo_kv_a_rust_snapshot_imports_into_a_fresh_go_node", "shinzo::kv_snapshot::kv_snapshot_round_trips_between_rust_nodes", "shinzo::kv_snapshot::kv_import_refuses_a_field_this_node_lacks", "shinzo::kv_snapshot::kv_a_datastore_only_snapshot_is_refused"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a Go Shinzo snapshot imports into a Rust node; a Rust snapshot imports into a fresh Go Shinzo node"

defradb.rs imports a document by storing its blocks and merging its composite heads, as a local replay of a replicated document: the first version names its DocID, the merge gives it this node's short IDs and its values, and the document reads back with the same DocID and values as on the source. A document whose versions do not reach its first one (a Go snapshot of an updated document) cannot be named: the others are imported, and the import fails with `… so their DocIDs cannot be recovered and they were not imported (N other documents were); export them with history`. A snapshot with no heads for a document (`datastoreOnly`) fails with `a datastore-only snapshot carries no DocIDs or history, so it cannot be imported`. With the source's mapping, the collection it names must exist (`collection not found`) with every field it names (`the field mapping names field <name> (<field ID>), which collection <collection> does not have: add the field before importing`), checked before anything is written. It runs as the caller, under the update-document node permission, and returns the frames read. `proofs/tla/KvImport.tla` models it.

- **Go-Bug-Suspect:** "Go writes the frames into the rootstore verbatim and exports no DocID-to-short-ID mappings, so its imported documents read back with an empty `_docID`, and on a node that already holds documents of the collection they land on other documents' short IDs; defradb.rs gives each imported document its DocID and its own short ID"
- **Go-Bug-Suspect:** "a mapped field whose field ID the destination does not have keeps the source's field short ID, so its value is written under whatever field owns that short ID locally; defradb.rs refuses the import"

# `go.shinzo.kv_snapshot.restore`: An import is a restore, not judged as a peer's push

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["db::merge::kv_snapshot::an_import_restores_what_the_floor_and_the_filter_refuse_from_peers"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The replication filter (`go.shinzo.p2p.replication_filter`) and the retention floor (`go.shinzo.retention`) judge what peers send. A snapshot import is the node's operator restoring documents, so neither judges it: a document at height 5 under a floor of 100, in a collection a filter refuses everything of, is imported. Go's import writes raw keys and passes through neither.

# `go.shinzo.kv_snapshot.batches`: An interrupted import is completed by running it again, and live writes are kept

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["db::merge::kv_snapshot::live_writes_during_an_import_are_kept", "shinzo::kv_snapshot::kv_snapshot_round_trips_between_rust_nodes"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go commits an import in transactions of 1000 frames, so one that fails part-way keeps what it committed. defradb.rs commits each document's merge on its own, so the same holds per document; running the whole import again finishes it, and importing a snapshot again changes nothing. A write made to an imported document after the import, or while it runs, is kept when the import runs again: the import merges, it never writes over.

# `go.shinzo.kv_snapshot.indexes`: Secondary indexes cover the imported documents

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::shinzo_kv_a_go_snapshot_imports_into_rust", "shinzo::kv_snapshot::shinzo_kv_a_rust_snapshot_imports_into_a_fresh_go_node", "shinzo::kv_snapshot::kv_snapshot_round_trips_between_rust_nodes"]
- **Go-Ref:** "shinzo-develop 2a899b860"

After an import, a query that uses a secondary index of the collection finds the imported documents. defradb.rs's merge maintains the indexes as it writes each document, so nothing more is needed. Go keeps them only once `RebuildCollectionIndexes(collection)` (`kv import --rebuild-indexes`) has dropped and recreated every index of the collection, keeping each index's name and fields; it fails with `collection not found` for an unknown collection.
