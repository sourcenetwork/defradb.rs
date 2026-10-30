# Shinzo tests that do not apply to defradb.rs

Go `shinzo-develop` tests (commit `2a899b860`) that check only a Go-specific mechanism, so they have no `go.shinzo` requirement. Keys use the same form as `Go-Tests` in `reqs/go/shinzo.md`.

| Go test | Why it does not apply |
|---|---|
| `internal/core/block/batch_signing_test.go::TestBatchCIDCollector_TruncatePanicsOutOfRange` | Go internal API contract (panic and mutex release on a caller bug); no user-visible behaviour. |
| `internal/core/block/heads_cache_test.go::TestHeadsCache_SetGet` | Go heads cache map internals. |
| `internal/core/block/heads_cache_test.go::TestHeadsCache_UpdateOnReplace` | Go heads cache ordering and maxHeight internals. |
| `internal/core/block/heads_cache_test.go::TestHeadsCache_UpdateOnWrite` | Go heads cache ordering and maxHeight internals. |
| `internal/core/block/heads_cache_test.go::TestHeadsList_NewDocCreateMode_SkipsScan` | Asserts Go skips a scan even when a head exists; an optimisation for Go's headstore iterator, not behaviour. |
| `internal/core/block/heads_cache_test.go::TestHeadsList_ServesFromCache` | Asserts Go returns a stale cached head after the store changed; cache mechanism, not behaviour. |
| `internal/core/block/heads_cache_test.go::TestInitHeadsCache_Idempotent` | Go heads cache plumbing on context.Context; no defradb.rs counterpart. |
| `internal/core/block/heads_cache_test.go::TestInitHeadsCache_SetsCache` | Go per-transaction heads cache on context.Context; defradb.rs reads head sets with regolith point reads. |
| `internal/core/block/heads_cache_test.go::TestNewDocCreateMode` | Go context flag for skipping a headstore scan; an optimisation for Go's iterator cost. |
| `internal/datastore/blockstore_gc_chunk_test.go::TestReclaimOrphanBlocksThroughChunkedStore` | Go's chunked blockstore (`blockStoreChunkSize`) splitting values across suffixed keys; defradb.rs has no chunked store. |
| `internal/datastore/blockstore_gc_test.go::TestReclaimBatchSkipsUntimestampedMarker` | Same older-Go-build marker guard, in the delete phase. |
| `internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksCountsUnparsedMarker` | A Go marker key that does not decode as a CID; regolith's typed key families cannot hold one. |
| `internal/datastore/blockstore_gc_test.go::TestReclaimOrphanBlocksNeverReclaimsUntimestampedMarker` | Protects blocks marked by an older Go build that did not index signature owners; no such defradb.rs store exists. |
| `internal/datastore/blockstore_gc_test.go::TestStoreDetectsConflictOnMarkerReadThenDelete` | Asserts Badger's read-set conflict detection; regolith validates commits itself, and the user-visible guarantee is `go.shinzo.orphan_gc.race`. |
| `internal/datastore/blockstore_gc_test.go::TestToMergeTimeRejectsLegacyMarker` | Recognises single-byte markers written by older Go builds; defradb.rs never opens a Go Badger store. |
| `internal/datastore/blockstore_gc_test.go::TestToMergeValueRoundTrip` | Go's on-disk to-merge marker encoding (8-byte big-endian Unix seconds); store-local, never exchanged with peers. |
| `internal/db/blockowner/blockowner_test.go::TestHasOwnersExceptExcludedSoleOwner` | Go helper that hides owner edges a separate uncommitted Go transaction deleted; defradb.rs decides ownership inside the purge's own regolith transaction. The behaviour is in `go.shinzo.purge.prune_history`. |
| `internal/db/blockowner/blockowner_test.go::TestHasOwnersExceptKeepsUnexcludedOwner` | Same Go-only excluded-edge workaround; behaviour covered by `go.shinzo.purge.prune_history`. |
| `internal/db/blockowner/blockowner_test.go::TestHasOwnersExceptStopsAtFirstUnexcludedOwner` | Counts Go iterator advances in the excluded-edge workaround; Go-internal scan cost. |
| `internal/db/blockowner/blockowner_test.go::TestHasOwnersStopsAtFirstOwner` | Counts Go iterator reads inside its owner-index helper; a Go-internal scan cost, not behaviour. |
| `internal/db/collection_purge_bench_test.go::BenchmarkPurgeByDocIDsChunkSize` | Tunes Go's purge chunk size against Badger's pending-write re-sort cost; defradb.rs reports its own before-and-after numbers. |
| `internal/db/collection_purge_test.go::TestPurgeByDocIDsChunksPurgeOverTransactionLimit` | Proves Go chunks a purge to stay under Badger's per-transaction size limit (`Txn is too big`); regolith has no such limit. |
| `internal/db/p2p/car_cache_test.go::TestCARCache_BuildThatPanicsFailsItsWaiters` | Go panic propagation through a goroutine-shared build; defradb.rs does not unwind panics into waiting tasks. |
| `internal/db/p2p/car_cache_test.go::TestCARCache_WaiterGivesUpOnItsContext` | Go context cancellation of a goroutine parked on a channel; defradb.rs waits carry their own deadline per the house rules. |
| `internal/db/p2p/p2p_test.go::TestQueueByteBudget_DerivesFromMemoryLimit` | Derives the queue byte budget from Go's GOMEMLIMIT (`debug.SetMemoryLimit`), which has no Rust counterpart. |
| `internal/request/graphql/parser_test.go::TestParseWhileSetSchemaCommits` | Go data race on the parser's schema manager pointer (fixed with atomic.Pointer); defradb.rs publishes schema snapshots as an `Atom`, so the race cannot occur. |
| `node/store_badger_test.go::TestBadgerStoreCloseStopsGCAndIsIdempotent` | Go goroutine and WaitGroup shutdown of Badger maintenance loops. |
| `node/store_badger_test.go::TestBadgerStoreReclaimValueLogTerminates` | Badger RunValueLogGC / ErrNoRewrite loop. |
| `node/store_badger_test.go::TestNewBadgerStoreClosesCleanlyWithOrphanGCDisabled` | Go WaitGroup miscount guard on Badger store Close. |
| `node/store_badger_test.go::TestNewStoreBadgerGCWrapping` | Badger value-log GC wrapper chosen by in-memory vs persistent; regolith has no value log GC to schedule. |
| `results_test.go::TestCapturedCursor_StoresValuesPerNode` | Checks only the Go test harness's cursor-capturing matcher, not DefraDB behaviour. |
