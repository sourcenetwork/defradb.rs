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

# `go.shinzo.merge.merged_state`: Whether a block is merged follows the store

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/datastore/blockstore_test.go::TestIsMerged_RolledBackTxnLeavesBlockUnmerged", "internal/datastore/blockstore_test.go::TestIsMerged_CommittedTxnMarksBlockMerged", "internal/datastore/blockstore_test.go::TestIsMerged_DeletedBlockIsNotMerged", "internal/datastore/blockstore_test.go::TestIsMerged_ReputtingAMergedBlockLeavesItMerged", "internal/datastore/blockstore_test.go::TestIsMerged_UnmergedAndAbsentBlocks"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A block fetched from a peer but not yet merged reads as not merged. Marking it merged takes effect only when that transaction commits; a discarded transaction leaves it unmerged. A block that was never stored, or was merged and then deleted (by a history prune), reads as not merged, so it is fetched again. Storing a block the node already holds as merged, through a single put or a batch put (as a CAR import does), leaves it merged.

# `go.shinzo.block_owner`: Which documents own a block

- **Parents:** ["go.shinzo"]

Each block records the DocIDs that own it. Field blocks are content-addressed and carry no document identity, so ownership is a set.

# `go.shinzo.block_owner.set`: A block can have several owners

- **Parents:** ["go.shinzo.block_owner"]
- **Go-Tests:** ["internal/db/blockowner/blockowner_test.go::TestDocIDs", "internal/db/blockowner/blockowner_test.go::TestHasOwners", "internal/db/id/document_test.go::TestBlockDocIDMappings", "internal/db/id/document_test.go::TestDocIDMappingMissingReturnsNotFound", "internal/db/document_test.go::TestBlockDocIDMappingSharedFieldBlockHasAllOwners"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Two `User` documents created with the same `name: "Shared"` and different `age` have different DocIDs but one shared `name` block, owned by both. Removing one owner leaves the block owned by the other; removing both leaves it unowned. An undefined CID, a block with no recorded owners, and an empty DocID record no owner. Looking up a DocID, document number or block that was never recorded returns not-found, not an error.

# `go.shinzo.docid`: Document identity under concurrency

- **Parents:** ["go.shinzo"]

# `go.shinzo.docid.concurrent_creates`: Concurrent creates all succeed with distinct documents

- **Parents:** ["go.shinzo.docid"]
- **Go-Tests:** ["internal/db/document_test.go::TestDocumentAdd_ConcurrentUsesUniqueShortIDs"]
- **Go-Ref:** "shinzo-develop 2a899b860"

24 `AddDocument` calls for 24 different `User` documents, started together, all return no error, and each document is stored as its own document: no two of them share an internal document number, and each reads back with its own values.

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
