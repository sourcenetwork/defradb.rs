# `go.shinzo.signing`: Signing of written documents

- **Parents:** ["go.shinzo"]

How a signing-enabled write signs its blocks. Go Shinzo's batch collector is not part of defradb.rs: `reqs/not-applying/shinzo.md` says why.

# `go.shinzo.signing.per_block_without_batch`: Without a collector, signing-enabled writes sign each block

- **Parents:** ["go.shinzo.signing"]
- **Go-Tests:** ["internal/core/block/store_test.go::TestAddDelta_WithoutBatchCollector_SignsBlock"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "Per-block signatures are unchanged from develop; a block written with signing on and no batch collector carries a signature link a Go node verifies."

With signing enabled, a secp256k1 identity in context and no batch collector, the block written for a collection (branchable) delta carries a `signature` link.

# `go.shinzo.heads`: Document and collection heads within one transaction

- **Parents:** ["go.shinzo"]

# `go.shinzo.heads.within_txn`: Heads read inside a transaction reflect that transaction's writes

- **Parents:** ["go.shinzo.heads"]
- **Go-Tests:** ["internal/core/block/heads_cache_test.go::TestHeadsList_CreateFlow", "internal/core/block/heads_cache_test.go::TestHeadsReplace_KeepsCacheConsistent", "internal/core/block/heads_cache_test.go::TestHeadsList_NewDocCreateMode_ReadsCollectionHead"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go caches heads per transaction; a Write to a namespace not yet listed in the transaction caches only the new head, hiding heads already in the store from a later List in the same transaction. InitHeadsCache also keeps an existing cache although InitContext promises fresh caches per transaction, so a context reused for a retry carries the aborted attempt's heads."

Creating a document reports no earlier heads for its fields; after the create writes a head at height 1, listing heads again in the same transaction returns exactly that head at height 1. Replacing a head with a new one at height 2 leaves only the new head, at height 2. While creating a new document, a branchable collection's head is still read, so every document created links the collection chain to the previous collection head (one existing head at height 1 is returned).
