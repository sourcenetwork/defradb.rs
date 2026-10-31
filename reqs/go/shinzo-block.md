# `go.shinzo.signing`: Batch signing of written documents

- **Parents:** ["go.shinzo"]

An embedder turns on batch signing for a write by attaching a CID collector to it (`node.NewBatchCIDCollector`, `node.ContextWithBatchSigning`). Instead of signing every block, the node records the documents it writes and signs them once, over a Merkle root, with `node.SignBatch`.

# `go.shinzo.signing.batch_collects_composites`: Batch mode collects each document's composite CID and leaves blocks unsigned

- **Parents:** ["go.shinzo.signing"]
- **Go-Tests:** ["internal/core/block/store_test.go::TestAddDelta_WithBatchCollector_CollectsCompositeCIDsOnly", "internal/core/block/batch_signing_test.go::TestBatchCIDCollector_LenTracksAddedCIDs"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "The BatchSignature layout and Merkle root must match Go byte for byte, so a Go Shinzo node verifies a Rust batch with VerifyBatchSignature and a Rust node verifies a Go one."
- **Go-Bug-Suspect:** "Go's verifier helper CollectDocumentCIDs scans the whole headstore prefix of each document (`HeadstoreDocKey{DocShortID}` with no field ID), so it returns field heads as well as the composite head, while batch mode collects only composite CIDs: a root recomputed from CollectDocumentCIDs differs from the signed root whenever the document has field heads. VerifyBatchSignature also ignores `cidCount`, and the odd-layer duplication means `[a, b, c]` and `[a, b, c, c]` have the same root. SignBatch, VerifyBatchSignature and ComputeMerkleRoot have no Go test."

With batch signing on and a signing identity in context, writing a document stores its blocks with no per-block `signature` link, and the collector holds exactly one CID per document: its composite (`C`) block. Field blocks are not collected, so a field value shared by two documents adds nothing. The collector's length counts every CID added, in order. Without a collector, a signing-enabled write signs each block as before (`go.shinzo.signing.per_block_without_batch`).

`SignBatch` builds the root over the collected CIDs: sort them by their CID bytes, hash each with SHA-256 to make the leaf layer, then repeatedly hash the concatenation `left || right` of each pair (an odd last node is paired with itself) until one 32-byte hash remains; no CIDs gives `SHA-256("")`. It signs the root with the identity's key and returns `BatchSignature { header: SignatureHeader { type, identity }, value, merkleRoot, cidCount }` (IPLD schema `type BatchSignature struct { header SignatureHeader value Bytes merkleRoot Bytes cidCount Int }`): `type` is `ES256K` for a secp256k1 key and `EdDSA` for Ed25519, `identity` is the public key as lowercase hex (compressed SEC1 for secp256k1, raw 32 bytes for Ed25519), and `cidCount` is the number of collected CIDs. With no signing identity in context it fails with `batch signing requires an identity in context`; another key type fails with `unsupported key type for signing`. `VerifyBatchSignature(sig, cids)` recomputes the root from `cids`, returns false when it differs from `merkleRoot`, and otherwise verifies `value` over `merkleRoot` with the key named by the header. It does not compare `cidCount` with the number of `cids`: the count is recorded, not checked, so for a batch signed over `[a, b, c]` the list `[a, b, c, c]` recomputes the same root and verifies too.

# `go.shinzo.signing.per_block_without_batch`: Without a collector, signing-enabled writes sign each block

- **Parents:** ["go.shinzo.signing"]
- **Go-Tests:** ["internal/core/block/store_test.go::TestAddDelta_WithoutBatchCollector_SignsBlock"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "Per-block signatures are unchanged from develop; a block written with signing on and no batch collector carries a signature link a Go node verifies."

With signing enabled, a secp256k1 identity in context and no batch collector, the block written for a collection (branchable) delta carries a `signature` link.

# `go.shinzo.signing.batch_rollback`: A batch can be rolled back to an earlier mark

- **Parents:** ["go.shinzo.signing"]
- **Go-Tests:** ["internal/core/block/batch_signing_test.go::TestBatchCIDCollector_TruncateRollsBackToMark"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Nothing in shinzo-develop calls Truncate, so a write that fails or retries after adding CIDs leaves them in the collector and the signed root covers blocks that were never committed."

Taking the collector's length as a mark, adding more CIDs, then truncating to the mark leaves exactly the CIDs added before the mark, in their original order. The batch signature then covers only the writes kept.

# `go.shinzo.store`: Block storage changes

- **Parents:** ["go.shinzo"]

What Shinzo changes about how blocks are stored, encrypted and marked merged.

# `go.shinzo.store.signature_block_merged`: Merging a signed block marks its signature block merged

- **Parents:** ["go.shinzo.store"]
- **Go-Tests:** ["internal/core/block/store_test.go::TestProcessBlock_ClearsSignatureMarker"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A signature block received over P2P starts out marked to-merge. Merging a block whose `signature` link points at it clears that mark in the same transaction, so the orphan block sweep (`go.shinzo.store.orphan_sweep_config`) never reclaims the signature of a merged block.

# `go.shinzo.store.orphan_sweep_config`: The orphan block sweep is configured from the environment

- **Parents:** ["go.shinzo.store"]
- **Go-Tests:** ["node/store_badger_test.go::TestDurationFromEnvFallsBackWhenUnsetOrInvalid", "node/store_badger_test.go::TestBoolFromEnv", "node/store_badger_test.go::TestNewBadgerStoreResolvesOrphanGCDisabled", "node/store_badger_test.go::TestNewBadgerStoreAppliesEnvOverrides", "node/store_badger_test.go::TestNewBadgerStoreDefaultsOrphanGCSettings", "node/store_badger_test.go::TestNewBadgerStoreOrphanGCDisabledLeavesOrphansAlone"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Scope:** "Go runs the sweep only for a persistent Badger store; an in-memory store never sweeps."

A persistent store periodically deletes orphan blocks: blocks fetched over P2P whose to-merge mark is older than the TTL. Three environment variables control it, read once when the store opens:

- `DEFRA_ORPHAN_GC_DISABLED`: `1`, `t`, `T`, `TRUE`, `true`, `True` turn the sweep off; `0`, `f`, `F`, `FALSE`, `false`, `False`, unset, or any other value (`yes`, `off`, empty) leave it on. A value that is set but not a boolean is logged as `Ignoring malformed boolean environment variable` with its name and value.
- `DEFRA_ORPHAN_GC_INTERVAL`: a duration (`3m`, `90s`) between sweeps; default 5 minutes.
- `DEFRA_ORPHAN_BLOCK_TTL`: a duration a to-merge block is kept; default 30 minutes.

An unset, empty, unparsable (`not-a-duration`), zero or negative duration falls back to its default instead of failing the open. The first sweep waits one TTL after the store opens; each sweep examines at most 100,000 marks and resumes where the previous one stopped. With the sweep on, a block put through the P2P blockstore and never merged is deleted once its mark ages past the TTL; with it off, no block is ever deleted by the sweep.

# `go.shinzo.heads`: Document and collection heads within one transaction

- **Parents:** ["go.shinzo"]

# `go.shinzo.heads.within_txn`: Heads read inside a transaction reflect that transaction's writes

- **Parents:** ["go.shinzo.heads"]
- **Go-Tests:** ["internal/core/block/heads_cache_test.go::TestHeadsList_CreateFlow", "internal/core/block/heads_cache_test.go::TestHeadsReplace_KeepsCacheConsistent", "internal/core/block/heads_cache_test.go::TestHeadsList_NewDocCreateMode_ReadsCollectionHead"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go caches heads per transaction; a Write to a namespace not yet listed in the transaction caches only the new head, hiding heads already in the store from a later List in the same transaction. InitHeadsCache also keeps an existing cache although InitContext promises fresh caches per transaction, so a context reused for a retry carries the aborted attempt's heads."

Creating a document reports no earlier heads for its fields; after the create writes a head at height 1, listing heads again in the same transaction returns exactly that head at height 1. Replacing a head with a new one at height 2 leaves only the new head, at height 2. While creating a new document, a branchable collection's head is still read, so every document created links the collection chain to the previous collection head (one existing head at height 1 is returned).

# `go.shinzo.document`: Building documents from client values

- **Parents:** ["go.shinzo"]

# `go.shinzo.document.typed_nillable_arrays`: Typed lists fill nillable-element arrays

- **Parents:** ["go.shinzo.document"]
- **Go-Tests:** ["client/document_test.go::TestNewDocFromMap_TypedStringSliceForNillableArray_Persisted", "client/document_test.go::TestNewDocFromMap_TypedTimeSliceForNillableArray_Persisted"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A `[String]` field given a typed string list (`["bafyA", "bafyB", "bafyC"]`) and a `[DateTime]` field given a typed time list (`2026-07-07T12:00:00Z`, `2026-07-08T12:00:00Z`) are kept in the built document with every element present, in order: the Go tests build the document with `NewDocFromMap` and read the field back, without saving it. Before Shinzo, Go dropped such a value silently.

