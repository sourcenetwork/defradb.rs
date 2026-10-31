# `go.shinzo.signing.per_block_without_batch`: Without a collector, signing-enabled writes sign each block

- **Parents:** ["go.shinzo.signing"]
- **Scenarios:** ["db::merge::collection_heads::a_signed_write_without_a_collector_signs_the_collection_block"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Interop:** "Per-block signatures are unchanged from develop; a block written with signing on and no batch collector carries a signature link a Go node verifies."

With signing enabled, a secp256k1 identity in context and no batch collector, the block written for a collection (branchable) delta carries a `signature` link.

# `go.shinzo.heads.within_txn`: Heads read inside a transaction reflect that transaction's writes

- **Parents:** ["go.shinzo.heads"]
- **Scenarios:** ["db::block::heads::heads_read_inside_a_transaction_reflect_its_writes", "db::merge::collection_heads::each_create_links_the_collection_chain_to_the_previous_head"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Go-Bug-Suspect:** "Go caches heads per transaction; a Write to a namespace not yet listed in the transaction caches only the new head, hiding heads already in the store from a later List in the same transaction. InitHeadsCache also keeps an existing cache although InitContext promises fresh caches per transaction, so a context reused for a retry carries the aborted attempt's heads."

Creating a document reports no earlier heads for its fields; after the create writes a head at height 1, listing heads again in the same transaction returns exactly that head at height 1. Replacing a head with a new one at height 2 leaves only the new head, at height 2. While creating a new document, a branchable collection's head is still read, so every document created links the collection chain to the previous collection head (one existing head at height 1 is returned).

# `go.shinzo.heads`: Document and collection heads within one transaction

- **Parents:** ["go.shinzo"]

# `go.shinzo.store.orphan_sweep_config`: The orphan block sweep is configured from the environment

- **Parents:** ["go.shinzo.store"]
- **Scenarios:** ["db::block::orphan_gc::the_sweep_is_configured_from_the_environment", "db::block::orphan_gc::with_the_sweep_on_an_abandoned_fetch_goes_once_its_mark_ages"]
- **Go-Ref:** "shinzo-develop 2a899b860"
- **Scope:** "Go runs the sweep only for a persistent Badger store; an in-memory store never sweeps."

A persistent store periodically deletes orphan blocks: blocks fetched over P2P whose to-merge mark is older than the TTL. Three environment variables control it, read once when the store opens:

- `DEFRA_ORPHAN_GC_DISABLED`: `1`, `t`, `T`, `TRUE`, `true`, `True` turn the sweep off; `0`, `f`, `F`, `FALSE`, `false`, `False`, unset, or any other value (`yes`, `off`, empty) leave it on. A value that is set but not a boolean is logged as `Ignoring malformed boolean environment variable` with its name and value.
- `DEFRA_ORPHAN_GC_INTERVAL`: a duration (`3m`, `90s`) between sweeps; default 5 minutes.
- `DEFRA_ORPHAN_BLOCK_TTL`: a duration a to-merge block is kept; default 30 minutes.

An unset, empty, unparsable (`not-a-duration`), zero or negative duration falls back to its default instead of failing the open. The first sweep waits one TTL after the store opens; each sweep examines at most 100,000 marks and resumes where the previous one stopped. With the sweep on, a block put through the P2P blockstore and never merged is deleted once its mark ages past the TTL; with it off, no block is ever deleted by the sweep.

# `go.shinzo.store.signature_block_merged`: Merging a signed block marks its signature block merged

- **Parents:** ["go.shinzo.store"]
- **Scenarios:** ["db::merge::signature_blocks::a_merged_document_owns_its_signature_blocks_and_they_read_as_merged"]
- **Go-Ref:** "shinzo-develop 2a899b860"

A signature block received over P2P starts out marked to-merge. Merging a block whose `signature` link points at it clears that mark in the same transaction, so the orphan block sweep (`go.shinzo.store.orphan_sweep_config`) never reclaims the signature of a merged block.

# `go.shinzo.store`: Block storage changes

- **Parents:** ["go.shinzo"]

What Shinzo changes about how blocks are stored, encrypted and marked merged.
