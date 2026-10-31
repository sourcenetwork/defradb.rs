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
