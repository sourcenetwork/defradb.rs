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

defradb.rs imports a document by storing its blocks and merging its composite heads, as a local replay of a replicated document: the first version names its DocID, the merge gives it this node's short IDs and its values, and the document reads back with the DocID and values it has on the Go node, the DocID a Rust node gives the same document. Go exports a document's heads and the blocks they link, never earlier versions, so a document updated since it was created cannot be named and is not imported (`go.shinzo.kv_snapshot.history`). A snapshot with no heads for a document (`datastoreOnly`) fails with `a datastore-only snapshot carries no DocIDs or history, so it cannot be imported`.

The import reads the source's field mapping, the JSON Go's `ExportFieldMapping` writes: `{"collectionName": <name>, "collectionShortID": <the source's collection short ID>, "fields": [{"name": <field>, "fieldID": <global field ID>, "shortID": <the snapshot's field short ID>}, …]}`. The collection it names must exist (`collection not found`) with every field it names (`the field mapping names field <name> (<field ID>), which collection <collection> does not have: add the field before importing`), checked before anything is written. It runs as the caller, under the update-document node permission, and returns the frames read and the documents imported. `proofs/tla/KvImport.tla` models it.

- **Go-Bug-Suspect:** "Go writes the frames into the rootstore verbatim and exports no DocID-to-short-ID mappings, so its imported documents read back with an empty `_docID`, and on a node that already holds documents of the collection they land on other documents' short IDs; defradb.rs gives each imported document its DocID and its own short ID"
- **Go-Bug-Suspect:** "a mapped field whose field ID the destination does not have keeps the source's field short ID, so its value is written under whatever field owns that short ID locally; defradb.rs refuses the import"

# `go.shinzo.kv_snapshot.restore`: An import is a restore, not judged as a peer's push

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_an_import_restores_what_the_floor_and_the_filter_refuse_from_peers"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The replication filter (`go.shinzo.p2p.replication_filter`) and the retention floor (`go.shinzo.retention`) judge what peers send. A snapshot import is the node's operator restoring documents, so neither judges it: a Log at blockNumber 5, under a floor of 100 and a filter refusing blockNumber 100 or less, is imported. Go's import writes raw keys and passes through neither.

# `go.shinzo.kv_snapshot.history`: A document whose create block the snapshot lacks is reported, not imported

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_a_go_snapshot_of_an_updated_document_has_no_history"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go derives a DocID from the document's create block (its first composite), and none of its deltas encodes it (`internal/core/crdt/delta_doc_id_test.go` `TestDocumentDeltasDoNotEncodeDocID`). Its export (`exportBlockDAG`) follows a block's `Links`, never its `Heads`, so the snapshot of a document updated since it was created holds only its newest version and no frame names its DocID. Such a document is not imported, and its blocks are not merged; the other documents of the snapshot are, and the import then fails with `the KV snapshot holds no history back to the first version of the documents with short IDs [<short ID>, …]: it has no create block for them, and a DocID is derived from the create block, so their DocIDs cannot be recovered and they were not imported (N other documents were): Go Shinzo exports only a document's newest version, so a document updated since it was created has to be replicated from a peer instead`. The short ID is the one the snapshot gives the document, the only name it carries.

# `go.shinzo.kv_snapshot.signatures`: A signed block whose signature block the snapshot lacks is restored as unsigned

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_a_signed_go_snapshot_imports_without_its_signature_blocks", "db::merge::merge_handler_tests::missing_signature_block_passes_only_a_restore", "db::merge::merge_handler_tests::verify_tampered_block_returns_error"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a snapshot a signing Go Shinzo node exported imports into a Rust node"

Go Shinzo signs a block when the request's identity holds its private key, and its export (`exportBlockDAG`) follows only a block's `Links`, never its `Signature`, so a signing node's snapshot links signature blocks it does not carry. The import accepts each such block as unsigned, and the documents read back with the values and the DocIDs they have on the Go node. Only a restore does: a block a peer pushes whose signature block is missing still fails with `block signature verification failed for cid=…: signature block … not found in blockstore`, and a signature block that is there but does not verify fails a restore too. `proofs/tla/KvImport.tla` models the rule.

# `go.shinzo.kv_snapshot.encryption`: An encrypted document the snapshot cannot decrypt is reported, not stored

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Scenarios:** ["shinzo::kv_snapshot::kv_an_encrypted_go_document_is_reported_not_stored"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go's export leaves out a block's `Encryption` link as it does its signature, and the document's key stays in the Go node's key store, so an encrypted document's blocks cannot be read where the snapshot is imported. A document with a block whose encryption block neither the snapshot nor the node holds is not stored; the others are imported, and the import fails with `the KV snapshot's documents [<DocID>] are encrypted, and it carries neither their encryption blocks nor their keys, so their values cannot be read and they were not imported (…)`, naming the document by its DocID, or by its short ID when its first version is missing.

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

# `go.shinzo.retention`: Retention floor for replicated documents

- **Parents:** ["go.shinzo"]

A node configured with a `client.RetentionRule` refuses replicated documents below a per-collection height. `RetentionFloor(collectionID) (field string, floor int64, ok bool)` names the integer height field and the floor for a covered collection. The rule applies only to documents arriving by P2P merge, never to local writes.

# `go.shinzo.merge.telemetry.report`: Creates, updates, conflicts and exhausted chunks

- **Parents:** ["go.shinzo.merge.telemetry"]
- **Scenarios:** ["db::merge::merge_stats::a_replicated_create_then_update_count_once_each_on_the_merging_node"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The `merge stats` info line carries `creates`, `updates`, `txnConflicts`, `chunkExhausted`, `rejectedBelowFloor` and `rejectedNoHeight`, and is logged only when one of them is non-zero; a separate `purge stats` info line carries `deleted`. A merge of a document with no local heads counts as a create, a later merge of the same document as an update. A merge counts once, when it commits: an event merged in a batch attempt that then failed and was re-run alone counts one create. Every abandoned transaction counts one `txnConflicts`. A single-event `Merge` that exhausts its attempts counts `txnConflicts` equal to its attempt count, no `chunkExhausted`, one `retryExhausted` drop, and its error names `DocID: <docID>` and `CID: <head CID>`. A batch of 8 whose shared attempt exhausts counts `chunkExhausted: 1` whether its per-event re-runs then all succeed (no drops) or all fail (8 `retryExhausted` drops); a batch of one that exhausts counts no `chunkExhausted` and one `retryExhausted` drop.

# `go.shinzo.merge.telemetry.drops`: Dropped events are counted by cause

- **Parents:** ["go.shinzo.merge.telemetry"]
- **Scenarios:** ["db::merge::merge_stats::a_merge_that_stores_nothing_counts_a_drop_by_cause"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A dropped event (one not stored) is counted under one cause: `missingBlock` (a block of the DAG could not be found), `uniqueIndex` (`can not index a doc's field(s) that violates unique index`), `retryExhausted`, `collectionNotFound` (the event names a collection this node does not hold; `Merge` returns `collection not found`), `contextDone` (cancelled or past its deadline, including a collection lookup that fails because the node is shutting down, on both `Merge` and `MergeBatchWithTxn`, where the outcome is `MergeDropped`), and `other` for anything else. The causes that occurred in an interval are logged on their own `merge drops` error line, one count per cause (e.g. `missingBlock: 2, uniqueIndex: 1`), then reset; an interval with no drops logs no such line.

# `go.shinzo.merge.telemetry`: Merge and purge counters

- **Parents:** ["go.shinzo.merge"]

Merge and purge activity is counted and reported every 30 seconds; each report resets its counters, so a line carries that interval's numbers.

# `go.shinzo.kv_snapshot`: Export and import a collection's documents as a KV snapshot

- **Parents:** ["go.shinzo"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`DB.ExportDocKVs(collection, docIDs, w, datastoreOnly)` writes the named documents of one collection to a byte stream; `DB.ExportFieldMapping(collection)` writes the JSON that lets another node read it; `DB.ImportRawKVs(r)` and `DB.ImportRawKVsWithMapping(r, mapping)` load a stream into a node; `DB.RebuildCollectionIndexes(collection)` restores that collection's secondary indexes afterwards. Short IDs are local to a node, so a snapshot moves between nodes only through the field mapping. Go reaches these only in-process: its CLI `client kv export|import` always runs through the HTTP client, which refuses all five with `ErrNotSupportedViaHTTP`. Go has no tests for them, and these requirements are written from `internal/db/kv_export.go`, `internal/db/kv_import.go`, `cli/kv_export.go` and `cli/kv_import.go`. defradb.rs imports what old Go Shinzo nodes export, with `DB::import_kv_snapshot_as`, and exports nothing in this format (`reqs/not-applying/shinzo.md`); `tools/go-shinzo-kv` ran Go's export once to write the import tests' fixtures.

# `go.shinzo.merge.merged_state`: Whether a block is merged follows the store

- **Parents:** ["go.shinzo.merge"]
- **Scenarios:** ["blockstore::blockstore_tests::merged_state::a_discarded_mark_leaves_the_block_unmerged_and_a_committed_one_merges_it", "blockstore::blockstore_tests::merged_state::an_absent_or_deleted_block_reads_as_not_merged", "blockstore::blockstore_tests::merged_state::storing_a_merged_block_again_leaves_it_merged_through_put_and_put_many"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A block fetched from a peer but not yet merged reads as not merged. Marking it merged takes effect only when that transaction commits; a discarded transaction leaves it unmerged. A block that was never stored, or was merged and then deleted (by a history prune), reads as not merged, so it is fetched again. Storing a block the node already holds as merged, through a single put or a batch put (as a CAR import does), leaves it merged.

# `go.shinzo.docid.concurrent_creates`: Concurrent creates all succeed with distinct documents

- **Parents:** ["go.shinzo.docid"]
- **Scenarios:** ["shinzo::merge::shinzo_concurrent_creates_all_store_their_own_documents"]
- **Go-Ref:** "shinzo-develop 2a899b860"

24 `AddDocument` calls for 24 different `User` documents, started together, all return no error, and each document is stored as its own document: no two of them share an internal document number, and each reads back with its own values.

# `go.shinzo.merge.concurrent_creates`: Concurrent new documents all merge

- **Parents:** ["go.shinzo.merge"]
- **Scenarios:** ["shinzo::merge::concurrently_created_documents_all_merge_on_the_receiver"]
- **Go-Ref:** "shinzo-develop 2a899b860"

24 `Merge` calls for 24 different new documents, started together, all return no error, and every document is readable afterwards.

# `go.shinzo.merge.local_ids`: A replicated document gets its own local IDs and keeps its DocID

- **Parents:** ["go.shinzo.merge"]
- **Scenarios:** ["shinzo::merge::a_replicated_document_keeps_its_doc_id_on_the_receiver", "db::merge::batch_isolation::a_merged_document_owns_its_field_blocks_under_its_doc_id"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the DocID is derived from the genesis composite block's CID (DocID V0) and must be the same on a Go Shinzo node and a Rust node for the same blocks"

A document created on one node as `{"name":"John","age":30}` and merged on another from its blocks reads back on the receiver as `{_docID: <same DocID>, name: "John", age: 30}`, even though the two nodes allocate their internal document numbers independently. On the receiver, the document's field blocks are owned by that DocID.

# `go.shinzo.block_owner.set`: A block can have several owners

- **Parents:** ["go.shinzo.block_owner"]
- **Scenarios:** ["db::collection::purge::a_shared_field_block_has_both_documents_as_owners"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Two `User` documents created with the same `name: "Shared"` and different `age` have different DocIDs but one shared `name` block, owned by both. Removing one owner leaves the block owned by the other; removing both leaves it unowned. An undefined CID, a block with no recorded owners, and an empty DocID record no owner. Looking up a DocID, document number or block that was never recorded returns not-found, not an error.

# `go.shinzo.docid`: Document identity under concurrency

- **Parents:** ["go.shinzo"]

# `go.shinzo.merge.batch_isolation`: One event that cannot merge does not stop the others

- **Parents:** ["go.shinzo.merge"]
- **Scenarios:** ["db::merge::batch_isolation::an_event_whose_parent_never_arrived_stops_no_other_event"]
- **Go-Ref:** "shinzo-develop 2a899b860"

In a batch where one event's head has a parent this node never received, every other event is stored and its outcome is `MergeCommitted`; the bad event's outcome is `MergeDropped` and it is not readable. This holds wherever the bad event sits: first in a batch of two, or at position 9 of 18. The returned error contains `cannot resolve document identity for composite block <head CID>`. The outcomes agree with what a read returns for each document.

# `go.shinzo.merge.retry`: Retries are bounded, and exhaustion is reported as a failure

- **Parents:** ["go.shinzo.merge"]
- **Scenarios:** ["db::merge::retry_exhaustion::exhausted_retries_fail_the_merge_saying_so", "db::merge::retry_exhaustion::a_budget_of_zero_still_makes_one_attempt"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go's text is `reached maximum transaction reties`, a typo for retries. defradb.rs does not copy it."

When every attempt to commit a merge conflicts, `Merge` returns an error containing `transaction retry attempts exhausted: <conflict>` (Go: `merge event dropped: reached maximum transaction reties: <conflict>. DocID: <docID>, CID: <head CID>`), and the batch path reports `[MergeDropped]` with an error; in both cases the document is not readable. A retry budget (`MaxTxnRetries`) of 0 still makes one attempt: a merge that does not conflict commits. (Go also checks that a head whose block is absent fails with `failed to load block for merge`; a defradb.rs merge is handed the block itself, so there is no head to load.)
