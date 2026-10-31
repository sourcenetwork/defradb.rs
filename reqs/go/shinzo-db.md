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

# `go.shinzo.kv_snapshot`: Export and import a collection's documents as a KV snapshot

- **Parents:** ["go.shinzo"]
- **Go-Ref:** "shinzo-develop 2a899b860"

`DB.ExportDocKVs(collection, docIDs, w, datastoreOnly)` writes the named documents of one collection to a byte stream; `DB.ExportFieldMapping(collection)` writes the JSON that lets another node read it; `DB.ImportRawKVs(r)` and `DB.ImportRawKVsWithMapping(r, mapping)` load a stream into a node; `DB.RebuildCollectionIndexes(collection)` restores that collection's secondary indexes afterwards. Short IDs are local to a node, so a snapshot moves between nodes only through the field mapping. Go reaches these only in-process and through `defradb client kv export|import` against a local node; its HTTP client refuses all five. Go has no tests for them, and these requirements are written from `internal/db/kv_export.go`, `internal/db/kv_import.go` and `cli/kv_export.go`, `cli/kv_import.go`.

# `go.shinzo.kv_snapshot.format`: The snapshot stream is length-prefixed frames ending in a zero length

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a stream written by either runtime is read by the other, frame for frame"

A snapshot is a sequence of frames `[key length: u32 big-endian][key][value length: u32 big-endian][value]`, ended by a key length of 0. A key's first byte names its store: `d` the datastore, `h` the headstore, `b` the blockstore; the rest is the key inside that store, in Go's encoding. A `b` key is the block's CID bytes and its value is the block's bytes. Reading a stream that ends cleanly before the sentinel is accepted as complete; a stream that ends inside a frame fails with an error and imports nothing past the last whole frame. Decoding a stream and encoding the frames again gives the same bytes.

# `go.shinzo.kv_snapshot.export`: Export writes each named document's values, heads and history

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the `b` and `h` frames match a Go Shinzo export of the same documents byte for byte; `d` frames match once each node's document short IDs are accounted for"

For each DocID, in the order given, the export writes the document's datastore entries (its field values, field priorities and deletion marker, as Go stores them under `/<collection short ID>/{v,p,d}/<doc short ID>/<field short ID>`), then its headstore entries (`/<doc short ID>/<field>/<CID>`), then every block reachable from those heads through block links, each block once. A DocID this node does not have is skipped, not an error. With `datastoreOnly`, only the datastore entries are written. It returns the number of frames written, not counting the sentinel. A collection that does not exist fails with `collection not found`, and an empty name with `collection name can't be empty`. The export reads one snapshot and writes nothing.

- **Go-Bug-Suspect:** "`kv export` documents `--doc-ids` as 'exports all if omitted', but `ExportDocKVs` with no DocIDs writes only the sentinel; defradb.rs exports every document of the collection when no DocID is given"

# `go.shinzo.kv_snapshot.field_mapping`: The field mapping names the collection and every stored field's short ID

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "the JSON is the shape Go writes and reads"

`ExportFieldMapping(collection)` returns `{"collectionName": <name>, "collectionShortID": <this node's collection short ID>, "fields": [{"name": <field>, "fieldID": <global field ID>, "shortID": <this node's field short ID>}, …]}`, one entry per field of the collection's current version that has a field ID, in the version's field order. Virtual fields (the secondary side of a relation) have no field ID and are left out. `client.CollectionFieldMapping` (snake-case keys, a short-ID-to-field-ID map) is declared in Go but nothing reads or writes it, so it is not part of the format.

# `go.shinzo.kv_snapshot.import`: Import loads a snapshot and the documents read back the same

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "a snapshot exported from a Go Shinzo node imports into a Rust node, and one exported from a Rust node imports into a fresh Go Shinzo node, and a query of the collection returns the same documents and values on both"

Importing a full snapshot (not `datastoreOnly`) with the source's field mapping into a node that defines the same collection makes every exported document readable there, with the same DocID, field values and version history (`_commits`) as on the source. Collection and field short IDs are translated from the source's to this node's through the mapping. It returns the number of frames read, not counting the sentinel. A mapping that is not valid JSON fails before anything is written; a mapping that names a collection this node does not have fails with `collection not found`. Importing the same snapshot twice leaves the same documents as importing it once.

- **Go-Bug-Suspect:** "Go writes the frames into the rootstore verbatim and exports no DocID-to-short-ID mappings, so on a node whose document short IDs differ from the source's (any node that already holds documents of the collection) the imported entries resolve to no DocID or land on another document's short ID; defradb.rs gives each imported document its own short ID on the destination"
- **Go-Bug-Suspect:** "a mapped field whose field ID the destination does not have keeps the source's field short ID, so its value is written under whatever field owns that short ID locally; defradb.rs refuses the import with an error naming the field"

# `go.shinzo.kv_snapshot.batches`: A failed import keeps what it committed, and a re-run completes it

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go commits an import in transactions of 1000 frames, so an import that fails part-way (a truncated stream, a storage error) keeps the frames it committed before the failure. Re-running the import of the whole snapshot then finishes it, with the same result as one uninterrupted import. Writes made to the collection on the node while an import runs are kept as well.

# `go.shinzo.kv_snapshot.indexes`: Secondary indexes cover the imported documents

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"

After an import, a query that uses a secondary index of the collection finds the imported documents, and a unique index rejects a new document that repeats an imported document's value with `can not index a doc's field(s) that violates unique index`. In Go this holds once `RebuildCollectionIndexes(collection)` (`kv import --rebuild-indexes`) has dropped and recreated every index of the collection, keeping each index's name and fields; that call fails with `collection not found` for an unknown collection.

# `go.shinzo.kv_snapshot.store_options`: Shinzo's Badger tuning on regolith

- **Parents:** ["go.shinzo.kv_snapshot"]
- **Go-Ref:** "shinzo-develop 2a899b860"

Go Shinzo hard-codes three Badger options in `node/store_badger.go`, none of them configurable: `ValueThreshold = 256` (values of 256 bytes or more go to the value log), `Compression = ZSTD` and `ZSTDCompressionLevel = 1`. regolith 0.2.3 has no key-value separation, so the value threshold has no equivalent and is dropped. Its block compression is `Options::compression`, whose codecs are none, Snappy and LZ4 (the default); with no ZSTD codec and no level setting, both stay at regolith's LZ4 default and are dropped. None of the three changes what a node stores or returns.
