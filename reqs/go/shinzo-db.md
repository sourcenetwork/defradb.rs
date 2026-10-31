# `go.shinzo.merge`: Merging replicated documents

- **Parents:** ["go.shinzo"]

`DB.Merge(event)` merges one replicated head; `DB.MergeBatchWithTxn(events)` merges many and returns one outcome per event, in the caller's order: `MergeCommitted` (stored), `MergeRejected` (refused by the retention rule) or `MergeDropped` (not stored). Callers relay only committed events onward.

# `go.shinzo.merge.signature_owner`: A skipped field's signature block is still owned

- **Parents:** ["go.shinzo.merge"]
- **Go-Tests:** ["internal/db/merge_test.go::TestMerge_FieldNotInCollection_StillOwnsSignatureBlock"]
- **Go-Ref:** "shinzo-develop 2a899b860"

With block signing on, a document `{"name":"John","age":30}` created on a node whose `User` has `age`, merged on a node whose `User` has only `name` (a rolling-upgrade schema skew), merges without error; on the receiver the signature block of the skipped `age` field block is owned by the document's DocID and reads as merged, so the orphan sweep never deletes it.

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

# `go.shinzo.block_owner.truncate`: Truncate removes document mappings and block ownership

- **Parents:** ["go.shinzo.block_owner"]
- **Go-Tests:** ["internal/db/collection_truncate_test.go::TestCollectionTruncateRemovesDocIDMappings"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Before `Truncate`, a document's genesis field block is owned by its DocID. After `Truncate`, the DocID no longer resolves on this node and the block has no owner.

