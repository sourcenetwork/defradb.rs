# `go.shinzo.purge`: Hard purge of documents by DocID

- **Parents:** ["go.shinzo"]

`Collection.PurgeByDocIDs(docIDs, pruneHistory)` permanently removes documents from this node. Unlike a delete, it writes no CRDT operation and replicates nothing: the documents are gone from this node only. It is checked against the same node permission as `Truncate`.

# `go.shinzo.purge.removes_document`: A purged document leaves nothing to find

- **Parents:** ["go.shinzo.purge"]
- **Go-Tests:** ["internal/db/collection_purge_test.go::TestPurgeByDocIDsRemovesPrimaryKey", "internal/db/collection_purge_test.go::TestPurgeByDocIDsRemovesIndexEntries"]
- **Go-Ref:** "shinzo-develop 2a899b860"

After `PurgeByDocIDs([alice], false)` on a `User { name: String, age: Int }` collection, a full scan of `User` returns no row for alice and no trace of a document that cannot be resolved. With a unique index on `name`, the purged document's index entries are gone too: adding a new document with `name: "alice"` afterwards succeeds instead of failing with `can not index a doc's field(s) that violates unique index`.

# `go.shinzo.purge.mappings`: Purge removes every DocID alias of the document

- **Parents:** ["go.shinzo.purge"]
- **Go-Tests:** ["internal/db/collection_truncate_test.go::TestCollectionDeleteDocIDMappingsRemovesDocAliases"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Without pruneHistory Go keeps the block owner edges that name the purged DocID, so its blocks stay owned by a document that no longer exists and the orphan sweep (go.shinzo.orphan_gc) can never reclaim them. Decide whether a purge without pruning should drop the edges."

Once a document is purged its DocID no longer resolves on this node, and neither does any other DocID that pointed at the same document (an older alias). Looking any of them up returns not-found. The record of which documents own the document's blocks is left as it was; only `pruneHistory` removes it (`go.shinzo.purge.prune_history`).

# `go.shinzo.purge.prune_history`: `pruneHistory` deletes every block only the purged documents own

- **Parents:** ["go.shinzo.purge"]
- **Go-Tests:** ["internal/db/collection_purge_test.go::TestPurgeByDocIDsPruneHistoryKeepsBlockOwnedByAnotherDoc", "internal/db/collection_purge_test.go::TestPurgeByDocIDsPruneHistoryRemovesBlockWhenAllOwnersPurgedTogether", "internal/db/collection_purge_test.go::TestPurgeByDocIDsPruneHistoryVisitsEachBlockOnce"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Scope:** "TestPurgeByDocIDsPruneHistoryVisitsEachBlockOnce also counts Go store reads and owner scans per block (one each); those counts are Go internals and not part of this requirement, only the deletion of all five blocks is"

With `pruneHistory: true`, every block reachable from the document's heads is deleted: for a document created as `{"name":"Alice","age":40}` and updated once, both composite blocks, both first-version field blocks and the updated field block (five blocks) are gone. A field block is content-addressed, so two documents with the same `name` share one `name` block. Purging one of them keeps the shared block; purging the other later removes it. Purging both in one call removes it as well, whether the shared block is a current head or is reached only through an older version.

# `go.shinzo.purge.no_conflict`: Pruning history does not conflict with a concurrent block store

- **Parents:** ["go.shinzo.purge"]
- **Go-Tests:** ["internal/db/collection_purge_conflict_test.go::TestPurgeByDocIDsPruneHistoryReadsBlocksOutsideConflictSet"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Scope:** "the first subtest is a control that shows Badger conflicts when a write transaction reads the block itself; only the second subtest states the requirement"

A `PurgeByDocIDs(..., true)` running in a transaction commits successfully even when another transaction stores one of the blocks the purge is walking (as a P2P sync recreating a block would) and commits first.

# `go.shinzo.purge.count`: Purged documents are counted once they commit

- **Parents:** ["go.shinzo.purge"]
- **Go-Tests:** ["internal/db/collection_purge_test.go::TestPurgeByDocIDsCountsOnlyDocumentsThatExisted", "internal/db/collection_purge_test.go::TestPurgeByDocIDsCountsOnlyCommittedDeletions"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A DocID that names no document is not an error and is not counted: `PurgeByDocIDs([alice, bob, alice])` succeeds and counts 2. A purge run in a caller's transaction counts nothing until that transaction commits, and nothing at all if it is discarded. The count is reported on the periodic `purge stats` log line (`go.shinzo.telemetry.merge.report`).

# `go.shinzo.retention`: Retention floor for replicated documents

- **Parents:** ["go.shinzo"]

A node configured with a `client.RetentionRule` refuses replicated documents below a per-collection height. `RetentionFloor(collectionID) (field string, floor int64, ok bool)` names the integer height field and the floor for a covered collection. The rule applies only to documents arriving by P2P merge, never to local writes.

# `go.shinzo.retention.rule`: The merge refuses what the rule refuses

- **Parents:** ["go.shinzo.retention"]
- **Go-Tests:** ["internal/db/retention_merge_test.go::TestMergeAppliesTheRetentionRule"]
- **Go-Ref:** "shinzo-develop 2a899b860"

For `type Log { blockNumber: Int address: String }` with field `blockNumber` and floor 100, merging a replicated document:

- `blockNumber: 100` (at the floor) is refused with `replicated document is at or below the retention floor`;
- `blockNumber: 101` is stored;
- no `blockNumber`, or `blockNumber: -5`, is refused with `replicated document has no valid retention height` (a null, non-integer or negative value has no valid height);
- with a floor of 0 or less, nothing is refused by height (`blockNumber: 0` is stored), but a document with no valid height is still refused;
- a collection the rule does not cover, or a node with no rule, stores everything.

Both refusals are `refused by the retention rule` errors (`errors.Is` matches either). A refused document is not stored (`document not found or not authorized to access`), its head is not marked merged, and the refusal is counted under `rejectedBelowFloor` or `rejectedNoHeight`, never as a dropped event.

# `go.shinzo.retention.scope`: Branchable and unknown collections are not judged

- **Parents:** ["go.shinzo.retention"]
- **Go-Tests:** ["internal/db/retention_merge_test.go::TestRetentionFloorOf"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A covered collection resolves to its field and floor (`blockNumber`, 100). A `@branchable` collection is never judged, because one of its heads commits many documents at once. A collection ID this node does not hold, or a node with no rule, is not judged either.

# `go.shinzo.retention.delete`: A delete is applied below the floor

- **Parents:** ["go.shinzo.retention"]
- **Go-Tests:** ["internal/db/retention_merge_test.go::TestMergeAppliesADeleteBelowTheFloor"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With floor 100, a replicated delete of a document whose `blockNumber` is 5 merges without error and its head is marked merged: deleted documents are not judged.

# `go.shinzo.retention.batch`: A refusal in a batch refuses only that document

- **Parents:** ["go.shinzo.retention"]
- **Go-Tests:** ["internal/db/retention_merge_test.go::TestMergeBatchRejectsOnlyTheRefusedEvent"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Merging a batch of three Logs at heights 101, 100 and 150 against floor 100 returns no error and the outcomes `[MergeCommitted, MergeRejected, MergeCommitted]`; the 101 and 150 documents are stored, the 100 one is not, one `rejectedBelowFloor` is counted, and no drop is recorded.

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

# `go.shinzo.docid.no_neighbour_conflict`: Reading one document does not conflict with its neighbour

- **Parents:** ["go.shinzo.docid"]
- **Go-Tests:** ["internal/db/fetcher_overread_test.go::TestDocumentFetcher_ReadingOneDocDoesNotConflictWithItsNeighbour"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A transaction that reads document alice and updates her `age` to 10 commits successfully even though another writer rewrote every field of bob (the next document in the collection) and committed in between.

# `go.shinzo.collection_id`: Collection IDs are deterministic

- **Parents:** ["go.shinzo"]
- **Go-Tests:** ["internal/db/collection_id_test.go::TestAddCollectionDeterministicIDsWithSharedNamePrefix"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a collection's version ID is content-addressed and must be identical on a Go Shinzo node and a Rust node given the same SDL, or the nodes do not agree on the collection"

Adding `type Foo { name: String } type FooBar { name: String }` to a fresh node gives `Foo` the same `VersionID` on every run (30 runs, one ID), even though `Foo` is a name prefix of `FooBar`.

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
