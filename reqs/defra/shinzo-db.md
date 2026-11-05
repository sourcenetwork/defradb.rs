# `go.shinzo.retention.rule`: The merge refuses what the rule refuses

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_merge_refuses_what_the_rule_refuses", "db::merge::retention::a_floor_of_zero_refuses_nothing_by_height", "db::merge::retention::an_uncovered_collection_or_a_node_without_a_rule_stores_everything", "db::merge::retention::a_document_is_judged_as_its_head_leaves_it", "db::merge::retention::a_document_that_gains_a_height_above_the_floor_is_kept", "db::merge::retention::an_update_to_the_floor_is_refused_and_the_document_keeps_its_height", "db::merge::retention::a_null_height_is_no_height"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The merge judges the document a pushed head's walk ends with, once: the walk merges the head's unmerged older versions first, in the same attempt, and a refusal discards them all. So a Log created at `blockNumber: 50` and updated to 150 is stored at 150, whether the update arrives alone or after the create was refused on its own; a Log created with no `blockNumber` and updated to 150 is stored too, as Go keeps both. An update that moves a stored Log to 100 is refused and the Log stays at its earlier height.

For `type Log { blockNumber: Int address: String }` with field `blockNumber` and floor 100, merging a replicated document:

- `blockNumber: 100` (at the floor) is refused with `replicated document is at or below the retention floor`;
- `blockNumber: 101` is stored;
- no `blockNumber`, or `blockNumber: -5`, is refused with `replicated document has no valid retention height` (a null, non-integer or negative value has no valid height; a nillable Int read back as null is no height);
- with a floor of 0 or less, nothing is refused by height (`blockNumber: 0` is stored), but a document with no valid height is still refused;
- a collection the rule does not cover, or a node with no rule, stores everything.

Both refusals read `<reason>: refused by the retention rule`, as Go's wrapped errors do. A refused document is not stored, its head is not marked merged (merging it again judges it again), and its outcome is `Rejected`, never a dropped event. A node takes any rule (`NodeBuilder::with_retention`), so a floor can move with the chain. The `rejectedBelowFloor` and `rejectedNoHeight` counters are `go.shinzo.merge.telemetry.report`.

# `go.shinzo.retention.scope`: Branchable and unknown collections are not judged

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::the_floor_resolves_for_a_covered_collection_only", "db::merge::retention::a_branchable_collection_is_not_judged"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A covered collection resolves to its field and floor (`blockNumber`, 100). A `@branchable` collection is never judged, because one of its heads commits many documents at once. A collection ID this node does not hold, or a node with no rule, is not judged either.

# `go.shinzo.retention.delete`: A delete is applied below the floor

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_delete_is_applied_below_the_floor", "db::merge::retention::an_update_concurrent_with_a_delete_merges_into_the_deleted_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With floor 100, a replicated delete of a document whose `blockNumber` is 5 merges without error and its head is marked merged: deleted documents are not judged. An update made concurrently with the delete that moves the height to 50 merges too: the merged document is deleted, so it is not judged.

# `go.shinzo.retention.batch`: A refusal in a batch refuses only that document

- **Parents:** ["go.shinzo.retention"]
- **Scenarios:** ["db::merge::retention::a_refusal_in_a_batch_refuses_only_that_document"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Merging a batch of three Logs at heights 101, 100 and 150 against floor 100 returns the outcomes `[Merged, Rejected, Merged]`; the 101 and 150 documents are stored, the 100 one is not, and no drop is recorded.

# `go.shinzo.kv_snapshot.format`: The snapshot stream is length-prefixed frames ending in a zero length

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_a_go_snapshot_imports_with_its_docids_and_indexes", "db::merge::kv_snapshot::frames_read_back_and_a_cut_frame_is_an_error"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a stream Go Shinzo wrote is read frame for frame"

A snapshot is a sequence of frames `[key length: u32 big-endian][key][value length: u32 big-endian][value]`, ended by a key length of 0. A key's first byte names its store: `d` the datastore, `h` the headstore, `b` the blockstore; the rest is Go's key inside that store, its integers in Go's ascending uvarint (short ID 1 is the byte `0x89`). A `b` key is the block's CID bytes and its value is the block's bytes. A stream that ends cleanly before the end mark is complete; one that ends inside a frame fails with `the KV snapshot ends inside a frame`. Reading frames Go wrote gives them back in order, and reading stops at the end mark whatever follows it (`proofs/lean/KvSnapshot/Frames.lean`).

# `go.shinzo.kv_snapshot.import`: Import replays each document's blocks, and the documents read back the same

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_a_go_snapshot_imports_with_its_docids_and_indexes", "shinzo::kv_snapshot::kv_a_go_snapshot_of_an_updated_document_has_no_history", "shinzo::kv_snapshot::kv_import_refuses_a_field_this_node_lacks", "shinzo::kv_snapshot::kv_a_datastore_only_snapshot_is_refused"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a snapshot an old Go Shinzo node exported imports into a Rust node"

defradb.rs imports a document by storing its blocks and merging its composite heads, as a local replay of a replicated document: the first version names its DocID, the merge gives it this node's short IDs and its values, and the document reads back with the DocID and values it has on the Go node, the DocID a Rust node gives the same document. Go exports a document's heads and the blocks they link, never earlier versions, so a document updated since it was created cannot be named: the others are imported, and the import fails with `… so their DocIDs cannot be recovered and they were not imported (N other documents were): Go Shinzo exports only a document's newest version, so a document updated since it was created has to be replicated from a peer instead`. A snapshot with no heads for a document (`datastoreOnly`) fails with `a datastore-only snapshot carries no DocIDs or history, so it cannot be imported`.

The import reads the source's field mapping, the JSON Go's `ExportFieldMapping` writes: `{"collectionName": <name>, "collectionShortID": <the source's collection short ID>, "fields": [{"name": <field>, "fieldID": <global field ID>, "shortID": <the snapshot's field short ID>}, …]}`. The collection it names must exist (`collection not found`) with every field it names (`the field mapping names field <name> (<field ID>), which collection <collection> does not have: add the field before importing`), checked before anything is written. It runs as the caller, under the update-document node permission, and returns the frames read and the documents imported. `proofs/tla/KvImport.tla` models it.

- **Go-Bug-Suspect:** "Go writes the frames into the rootstore verbatim and exports no DocID-to-short-ID mappings, so its imported documents read back with an empty `_docID`, and on a node that already holds documents of the collection they land on other documents' short IDs; defradb.rs gives each imported document its DocID and its own short ID"
- **Go-Bug-Suspect:** "a mapped field whose field ID the destination does not have keeps the source's field short ID, so its value is written under whatever field owns that short ID locally; defradb.rs refuses the import"

# `go.shinzo.kv_snapshot.restore`: An import is a restore, not judged as a peer's push

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_an_import_restores_what_the_floor_and_the_filter_refuse_from_peers"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The replication filter (`go.shinzo.p2p.replication_filter`) and the retention floor (`go.shinzo.retention`) judge what peers send. A snapshot import is the node's operator restoring documents, so neither judges it: a Log at blockNumber 5, under a floor of 100 and a filter refusing blockNumber 100 or less, is imported. Go's import writes raw keys and passes through neither.

# `go.shinzo.kv_snapshot.batches`: An interrupted import is completed by running it again, and live writes are kept

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_live_writes_during_an_import_are_kept", "shinzo::kv_snapshot::kv_a_go_snapshot_imports_with_its_docids_and_indexes"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go commits an import in transactions of 1000 frames, so one that fails part-way keeps what it committed. defradb.rs commits each document's merge on its own, so the same holds per document; running the whole import again finishes it, and importing a snapshot again changes nothing. A write made to an imported document after the import, or while it runs, is kept when the import runs again: the import merges, it never writes over.

# `go.shinzo.kv_snapshot.indexes`: Secondary indexes cover the imported documents

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_a_go_snapshot_imports_with_its_docids_and_indexes"]
- **Go-Ref:** "shinzo-develop 2a899b860"

After an import, a query that uses a secondary index of the collection finds the imported documents. defradb.rs's merge maintains the indexes as it writes each document, so nothing more is needed. Go keeps them only once `RebuildCollectionIndexes(collection)` (`kv import --rebuild-indexes`) has dropped and recreated every index of the collection, keeping each index's name and fields.
