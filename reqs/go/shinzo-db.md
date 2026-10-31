# `go.shinzo.purge`: Hard purge of documents by DocID

- **Parents:** ["go.shinzo"]

`Collection.PurgeByDocIDs(docIDs, pruneHistory)` permanently removes documents from this node. Unlike a delete, it writes no CRDT operation and replicates nothing: the documents are gone from this node only. It is checked against the same node permission as `Truncate`.

# `go.shinzo.retention`: Retention floor for replicated documents

- **Parents:** ["go.shinzo"]

A node configured with a `client.RetentionRule` refuses replicated documents below a per-collection height. `RetentionFloor(collectionID) (field string, floor int64, ok bool)` names the integer height field and the floor for a covered collection. The rule applies only to documents arriving by P2P merge, never to local writes.

# `go.shinzo.merge`: Merging replicated documents

- **Parents:** ["go.shinzo"]

`DB.Merge(event)` merges one replicated head; `DB.MergeBatchWithTxn(events)` merges many and returns one outcome per event, in the caller's order: `MergeCommitted` (stored), `MergeRejected` (refused by the retention rule) or `MergeDropped` (not stored). Callers relay only committed events onward.

# `go.shinzo.merge.concurrent_creates`: Concurrent new documents all merge

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_ConcurrentNewDocuments"]
- **Go-Ref:** "shinzo-develop 2a899b860"

24 `Merge` calls for 24 different new documents, started together, all return no error, and every document is readable afterwards.

# `go.shinzo.merge.local_ids`: A replicated document gets its own local IDs and keeps its DocID

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_GenesisWithEmptyDocID_ResolvesDocIDAndFieldMappings"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the DocID is derived from the genesis composite block's CID (DocID V0) and must be the same on a Go Shinzo node and a Rust node for the same blocks"

A document created on one node as `{"name":"John","age":30}` and merged on another from its blocks reads back on the receiver as `{_docID: <same DocID>, name: "John", age: 30}`, even though the two nodes allocate their internal document numbers independently. On the receiver, the document's field blocks are owned by that DocID.

# `go.shinzo.merge.batch_isolation`: One event that cannot merge does not stop the others

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMergeBatch_OneEventCannotMerge_OthersStillLand", "internal/db/merge_test.go::TestMergeBatch_FailureInLaterChunk_ReportsFailureAgainstTheRightEvent"]
- **Go-Ref:** "shinzo-develop 2a899b860"

In a batch where one event's head has a parent this node never received, every other event is stored and its outcome is `MergeCommitted`; the bad event's outcome is `MergeDropped` and it is not readable. This holds wherever the bad event sits: first in a batch of two, or at position 9 of 18. The returned error contains `could not find <missing CID>` and the bad event's DocID, wrapped as `merge event dropped: <cause>. DocID: <docID>, CID: <head CID>`. The outcomes agree with what a read returns for each document.

# `go.shinzo.merge.retry`: Retries are bounded, and exhaustion is reported as a failure

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_ExhaustedRetries_ReportsFailure", "internal/db/merge_test.go::TestMergeBatch_ExhaustedRetries_ReportsNothingMerged", "internal/db/merge_test.go::TestMerge_ZeroMaxRetriesStillAttempts", "internal/db/merge_test.go::TestMergeBatch_ZeroMaxRetriesStillAttempts"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go's text is `reached maximum transaction reties` (a typo for retries); a Rust node should match the text clients match on, so decide which spelling the requirement fixes."

When every attempt to commit a merge conflicts, `Merge` returns an error containing `maximum transaction` (Go: `merge event dropped: reached maximum transaction reties: <conflict>. DocID: <docID>, CID: <head CID>`), and the batch path reports `[MergeDropped]` with an error; in both cases the document is not readable. A retry budget (`MaxTxnRetries`) of 0 still makes one attempt: merging a head whose block is absent fails with `failed to load block for merge`, not with exhausted retries, on both paths.

# `go.shinzo.merge.signature_owner`: A skipped field's signature block is still owned

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_FieldNotInCollection_StillOwnsSignatureBlock"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With block signing on, a document `{"name":"John","age":30}` created on a node whose `User` has `age`, merged on a node whose `User` has only `name` (a rolling-upgrade schema skew), merges without error; on the receiver the signature block of the skipped `age` field block is owned by the document's DocID and reads as merged, so the orphan sweep never deletes it.

# `go.shinzo.merge.merged_state`: Whether a block is merged follows the store

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/datastore/blockstore_test.go::TestIsMerged_RolledBackTxnLeavesBlockUnmerged", "internal/datastore/blockstore_test.go::TestIsMerged_CommittedTxnMarksBlockMerged", "internal/datastore/blockstore_test.go::TestIsMerged_DeletedBlockIsNotMerged", "internal/datastore/blockstore_test.go::TestIsMerged_ReputtingAMergedBlockLeavesItMerged", "internal/datastore/blockstore_test.go::TestIsMerged_UnmergedAndAbsentBlocks"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A block fetched from a peer but not yet merged reads as not merged. Marking it merged takes effect only when that transaction commits; a discarded transaction leaves it unmerged. A block that was never stored, or was merged and then deleted (by a history prune), reads as not merged, so it is fetched again. Storing a block the node already holds as merged, through a single put or a batch put (as a CAR import does), leaves it merged.

# `go.shinzo.merge.refetch`: Re-receiving an unmerged block keeps it out of the orphan sweep

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/datastore/blockstore_refresh_test.go::TestRefresh_MovesMarkerWhileUnmerged", "internal/datastore/blockstore_refresh_test.go::TestRefresh_LeavesMergedBlockMerged", "internal/datastore/blockstore_refresh_test.go::TestRefresh_ConcurrentMergeLeavesBlockMerged", "internal/datastore/blockstore_refresh_test.go::TestRefresh_PutManyGuardsEachBlock"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Storing again a fetched block that is still waiting to merge restarts its age, so a block that keeps arriving is not swept as abandoned (`go.shinzo.orphan_gc`). Storing again a block that has merged leaves it merged. A merge and a re-receive of the same block racing each other always leave it merged (Go checks 300 races, zero may lose the merge). A batch put holding one waiting and one merged block restarts the first's age and leaves the second merged.

# `go.shinzo.orphan_gc`: Reclaiming fetched blocks that never merged

- **Parents:** ["go.shinzo"]

Blocks fetched during P2P sync whose merge never happened (an abandoned fetch) are reclaimed by a periodic sweep. A history prune never reaches them, since it walks out from documents. The sweep takes a cutoff time, a resume key and a scan limit, and reports `NextKey`, `Scanned`, `Reclaimed`, `Repaired`, `Conflicts` and `Unparsed`.

# `go.shinzo.orphan_gc.reclaim`: Only unowned blocks fetched before the cutoff are reclaimed

- **Parents:** ["go.shinzo.orphan_gc"]
- **Go-Tests:** ["internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocks", "internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksKeepsBlockOwnedByDocument", "internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksMixesReclaimAndRepairInOneBatch"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With one block fetched an hour before the cutoff, one fetched an hour after it, one fetched with no recorded time, and one merged block, a sweep deletes only the first (block and its waiting state), scans 3 (merged blocks are not scanned), reports `Reclaimed: 1`, `Repaired: 0`, `Conflicts: 0` and no `NextKey`. A block past the cutoff that a document owns is kept and is marked merged instead (`Repaired: 1`). Three unowned and one owned expired block in one sweep give `Reclaimed: 3`, `Repaired: 1`, `Scanned: 4`.

# `go.shinzo.orphan_gc.race`: A merge committing during the sweep keeps its block

- **Parents:** ["go.shinzo.orphan_gc"]
- **Go-Tests:** ["internal/datastore/blockstore_gc_test.go::TestReclaimBatchSkipsMarkerClearedAfterScan"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A block nominated by the sweep whose merge commits before the sweep deletes it is kept, and is not counted as reclaimed.

# `go.shinzo.orphan_gc.resume`: The sweep resumes, and stops when cancelled

- **Parents:** ["go.shinzo.orphan_gc"]
- **Go-Tests:** ["internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksResumesAcrossCalls", "internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksStopsOnCancelledContext"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With five expired blocks and a scan limit of 2, repeated sweeps, each starting from the previous `NextKey`, take more than one call, scan each block exactly once (5 in total), reclaim all 5, and end with no `NextKey`. A sweep whose context is already cancelled returns `context canceled`, scans nothing and deletes nothing.

# `go.shinzo.block_owner`: Which documents own a block

- **Parents:** ["go.shinzo"]

Each block records the DocIDs that own it. Field blocks are content-addressed and carry no document identity, so ownership is a set.

# `go.shinzo.block_owner.set`: A block can have several owners

- **Parents:** ["go.shinzo.block_owner"]
- **Go-Tests:** ["internal/db/blockowner/blockowner_test.go::TestDocIDs", "internal/db/blockowner/blockowner_test.go::TestHasOwners", "internal/db/id/document_test.go::TestBlockDocIDMappings", "internal/db/id/document_test.go::TestDocIDMappingMissingReturnsNotFound", "internal/db/document_test.go::TestBlockDocIDMappingSharedFieldBlockHasAllOwners"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Two `User` documents created with the same `name: "Shared"` and different `age` have different DocIDs but one shared `name` block, owned by both. Removing one owner leaves the block owned by the other; removing both leaves it unowned. An undefined CID, a block with no recorded owners, and an empty DocID record no owner. Looking up a DocID, document number or block that was never recorded returns not-found, not an error.

# `go.shinzo.block_owner.truncate`: Truncate removes document mappings and block ownership

- **Parents:** ["go.shinzo.block_owner"]
- **Go-Tests:** ["internal/db/collection_truncate_test.go::TestCollectionTruncateRemovesDocIDMappings"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Before `Truncate`, a document's genesis field block is owned by its DocID. After `Truncate`, the DocID no longer resolves on this node and the block has no owner.

# `go.shinzo.docid`: Document identity under concurrency

- **Parents:** ["go.shinzo"]

# `go.shinzo.docid.concurrent_creates`: Concurrent creates all succeed with distinct documents

- **Parents:** ["go.shinzo.docid"]
- **Go-Tests:** ["internal/db/document_test.go::TestDocumentAdd_ConcurrentUsesUniqueShortIDs"]
- **Go-Ref:** "shinzo-develop 2a899b860"

24 `AddDocument` calls for 24 different `User` documents, started together, all return no error, and each document is stored as its own document: no two of them share an internal document number, and each reads back with its own values.

# `go.shinzo.unique_index`: Unique-index errors name only the writer

- **Parents:** ["go.shinzo"]
- **Go-Tests:** ["internal/db/index_backfill_test.go::TestSaveUniqueKey_DoesNotNameTheDocumentHoldingTheValue"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With a unique index on `User.name` and a stored `{"name":"alice","age":1}`, adding `{"name":"alice","age":2}` fails with `can not index a doc's field(s) that violates unique index. DocID: <new document's DocID>, name: alice`. The error never contains the DocID of the document already holding the value, which the writer may not be allowed to read.

# `go.shinzo.telemetry.merge`: Merge and purge counters

- **Parents:** ["go.shinzo"]

Merge and purge activity is counted and reported every 30 seconds; each report resets its counters, so a line carries that interval's numbers.

# `go.shinzo.telemetry.merge.report`: Creates, updates, conflicts and exhausted chunks

- **Parents:** ["go.shinzo.telemetry.merge"]
- **Go-Tests:** ["internal/db/stats_test.go::TestMergeStatsCreateVsUpdate", "internal/db/stats_test.go::TestMergeStatsCountsCommittedMergeOnce", "internal/db/stats_test.go::TestMergeStatsSingleDocumentExhaustion", "internal/db/stats_test.go::TestMergeStatsChunkExhaustion"]
- **Go-Ref:** "shinzo-develop 2a899b860"

The `merge stats` info line carries `creates`, `updates`, `txnConflicts`, `chunkExhausted`, `rejectedBelowFloor` and `rejectedNoHeight`, and is logged only when one of them is non-zero; a separate `purge stats` info line carries `deleted`. A merge of a document with no local heads counts as a create, a later merge of the same document as an update. A merge counts once, when it commits: an event merged in a batch attempt that then failed and was re-run alone counts one create. Every abandoned transaction counts one `txnConflicts`. A single-event `Merge` that exhausts its attempts counts `txnConflicts` equal to its attempt count, no `chunkExhausted`, one `retryExhausted` drop, and its error names `DocID: <docID>` and `CID: <head CID>`. A batch of 8 whose shared attempt exhausts counts `chunkExhausted: 1` whether its per-event re-runs then all succeed (no drops) or all fail (8 `retryExhausted` drops); a batch of one that exhausts counts no `chunkExhausted` and one `retryExhausted` drop.

# `go.shinzo.telemetry.merge.drops`: Dropped events are counted by cause

- **Parents:** ["go.shinzo.telemetry.merge"]
- **Go-Tests:** ["internal/db/stats_test.go::TestMergeDropReason", "internal/db/stats_test.go::TestMergeStatsDrainDropReasons", "internal/db/stats_test.go::TestMergeStatsCollectionLookupCauses"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A dropped event (one not stored) is counted under one cause: `missingBlock` (a block of the DAG could not be found), `uniqueIndex` (`can not index a doc's field(s) that violates unique index`), `retryExhausted`, `collectionNotFound` (the event names a collection this node does not hold; `Merge` returns `collection not found`), `contextDone` (cancelled or past its deadline, including a collection lookup that fails because the node is shutting down, on both `Merge` and `MergeBatchWithTxn`, where the outcome is `MergeDropped`), and `other` for anything else. The causes that occurred in an interval are logged on their own `merge drops` error line, one count per cause (e.g. `missingBlock: 2, uniqueIndex: 1`), then reset; an interval with no drops logs no such line.

# `go.shinzo.save_many`: Save many documents in one transaction

- **Parents:** ["go.shinzo"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`Collection.SaveManyDocuments(docs)` saves each document in order, as `SaveDocument` does: a document whose DocID names a stored document is updated with the fields it gives, any other is added. Every document is saved in one transaction, so either all are stored or none is. It is checked against the update-document node permission. Go's HTTP client and CLI refuse it with `ErrNotSupportedViaHTTP` (a stateless request cannot hold the shared transaction), so it is reached in-process only; Go has no tests for it, and these requirements are written from `internal/db/document.go` (`SaveManyDocuments`, `SaveDocument`).

# `go.shinzo.kv_snapshot`: Export and import a collection's documents as a KV snapshot

- **Parents:** ["go.shinzo"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`DB.ExportDocKVs(collection, docIDs, w, datastoreOnly)` writes the named documents of one collection to a byte stream; `DB.ExportFieldMapping(collection)` writes the JSON that lets another node read it; `DB.ImportRawKVs(r)` and `DB.ImportRawKVsWithMapping(r, mapping)` load a stream into a node; `DB.RebuildCollectionIndexes(collection)` restores that collection's secondary indexes afterwards. Short IDs are local to a node, so a snapshot moves between nodes only through the field mapping. Go reaches these only in-process: its CLI `client kv export|import` always runs through the HTTP client, which refuses all five with `ErrNotSupportedViaHTTP`. Go has no tests for them, and these requirements are written from `internal/db/kv_export.go`, `internal/db/kv_import.go`, `cli/kv_export.go` and `cli/kv_import.go`. defradb.rs offers `DB::export_kv_snapshot_as` and `DB::import_kv_snapshot_as`; `tools/go-shinzo-kv` runs Go's side in-process for the interop tests.

# `go.shinzo.kv_snapshot.store_options`: Shinzo's Badger tuning on regolith

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go Shinzo hard-codes three Badger options in `node/store_badger.go`, none of them configurable: `ValueThreshold = 256` (values of 256 bytes or more go to the value log), `Compression = ZSTD` and `ZSTDCompressionLevel = 1`. All three are dropped. regolith 0.2.3 keeps values in its SSTables, with no key-value separation, so the value threshold has nothing to set. Its block compression is `Options::compression`, whose codecs are none, Snappy and LZ4 (the default), with no ZSTD codec and no level setting, so compression stays at regolith's LZ4 default. None of the three changes what a node stores or returns.
