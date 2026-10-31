# `defra`: Behaviour the Rust version defines

Requirements that start in defradb.rs rather than in a Go integration test.

# `defra.index`: Secondary indexes

- **Parents:** ["defra"]

Index behaviour beyond what Go's integration suite checks.

# `defra.index.unique_writer`: Unique-index errors name only the writer

- **Parents:** ["defra.index"]
- **Scenarios:** ["index::unique_writer::a_unique_index_error_names_the_writer_and_its_value"]

With a unique index on `User.name` and a stored `{"name":"alice","age":1}`, adding `{"name":"alice","age":2}` fails with `can not index a doc's field(s) that violates unique index. DocID: <new document's DocID>, name: alice`. Updating another document's `name` to `alice` fails with `failed to update index: can not index a doc's field(s) that violates unique index. DocID: <updated document's DocID>, name: alice. IndexName: User_name_ASC`. The error never contains the DocID of the document already holding the value, which the writer may not be allowed to read. Go develop builds the same text (`NewErrCanNotIndexNonUniqueFields`, `NewErrUpdateIndex`).

# `defra.collection`: Collection-wide operations

- **Parents:** ["defra"]

Operations on a collection's documents as a whole.

# `defra.collection.truncate_ownership`: Truncate removes document mappings and block ownership

- **Parents:** ["defra.collection"]
- **Scenarios:** ["db::collection::truncate::truncate_removes_doc_id_mappings_and_block_ownership"]

Each block records the DocIDs that own it; field blocks are content-addressed and carry no document identity, so ownership is a set. Before `Truncate`, a document's genesis field block is owned by its DocID. After `Truncate`, the DocID no longer resolves on this node and the block has no owner.

# `defra.save_many`: Save many documents in one transaction

- **Parents:** ["defra"]

`Node::save_docs` creates or updates a batch of documents of one collection in one transaction, through the same request path as a GraphQL mutation (NAC, document ACP, signing, encryption). It is a core API with no HTTP or CLI route.

# `defra.save_many.create_or_update`: Each document is added or updated

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::save_many_updates_a_stored_document_and_adds_a_new_one"]

With `User { name: String, age: Int }` holding Alice (`age: 40`), saving `[{_docID: <alice>, age: 41}, {name: "Bob", age: 50}]` leaves Alice at `age: 41` with her name unchanged and adds Bob. The call returns the saved documents' DocIDs in the order given.

# `defra.save_many.atomic`: One document that fails saves none

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::one_document_that_fails_saves_none"]

When one document of the batch cannot be saved, the call fails with that document's error and no document of the batch is stored, including the ones before it: saving `[{name: "Bob"}, {name: "Carol", nickname: "C"}]` on a `User` with no `nickname` field fails, and neither Bob nor Carol exists afterwards.

# `defra.save_many.deleted`: A deleted document is not saved again

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::a_deleted_document_is_not_saved_again"]

A document whose DocID names a document that was deleted fails the batch with `a document with the given ID has been deleted`, and nothing of the batch is stored. A DocID naming a stored document the caller cannot see fails with `document not found or not authorized to access`, so a save never bypasses document ACP.

# `defra.save_many.transaction`: In a caller's transaction, the batch lands on commit

- **Parents:** ["defra.save_many"]
- **Scenarios:** ["mutation::save_many::a_batch_in_a_transaction_lands_on_commit", "mutation::save_many::a_discarded_batch_stores_nothing", "mutation::save_many::an_empty_batch_stores_nothing"]

Saved in a caller's transaction, the documents are not visible outside it until it commits, and are never stored if it is discarded. An empty batch succeeds and stores nothing.

# `defra.purge`: Hard delete of documents by DocID

- **Parents:** ["defra"]

`DB::purge_by_doc_ids_as` (and `DbTransactionRegistry::purge_by_doc_ids_as`, in a caller's transaction) removes documents from this node only: no CRDT operation is written and nothing is replicated. It is checked against the node permission of a collection truncate. A filtered truncate removes each chunk of matching documents through the same purge.

# `defra.purge.removes_document`: A purged document leaves nothing to find

- **Parents:** ["defra.purge"]
- **Scenarios:** ["mutation::purge::purge_removes_the_document", "mutation::purge::purge_removes_the_unique_index_entry"]

After purging alice from a `User { name: String, age: Int }` collection, a full scan of `User` returns no row for alice and no trace of a document that cannot be resolved. With a unique index on `name`, the purged document's index entries are gone too: adding a new document with `name: "alice"` afterwards succeeds instead of failing with `can not index a doc's field(s) that violates unique index`.

# `defra.purge.mappings`: Purge removes every DocID alias of the document

- **Parents:** ["defra.purge"]
- **Scenarios:** ["mutation::purge::purged_doc_id_no_longer_resolves", "db::collection::purge::purge_drops_the_doc_id_mappings_and_owner_edges"]

Once a document is purged its DocID no longer resolves on this node, and neither does any other DocID that pointed at the same document (an older alias). Looking any of them up returns not-found. No block's ownership record names a purged DocID any more, including the blocks a purge keeps.

# `defra.purge.history`: A purge deletes the document's history, unless the collection is branchable

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::purge_deletes_history_and_keeps_a_block_another_document_owns", "db::collection::purge::purging_every_owner_together_deletes_the_shared_block", "db::collection::purge::a_block_a_merge_adopts_during_the_purge_is_kept", "mutation::purge::a_purged_document_can_be_added_again", "db::collection::purge::purge_keeps_a_branchable_collections_history"]

A purge always deletes every block reachable from the document's heads that no other document owns: for a document created as `{"name":"Alice","age":40}` and updated once, both composite blocks, both first-version field blocks and the updated field block (five blocks) are gone. A field block is content-addressed, so two documents with the same `name` share one `name` block. Purging one of them keeps the shared block; purging the other later removes it. Purging both in one call removes it as well, whether the shared block is a current head or is reached only through an older version. A block another document comes to own while the purge runs (a merge that finds the block already stored) is kept. In a `@branchable` collection the history stays: the collection DAG links every version's composite (one collection block per document commit), so the purge keeps every block of the document's history, in the caller's transaction as in its own, and the collection's `_commits` and its sync stay whole; only the purged owner edges go. There is no option to keep or prune history.

# `defra.purge.no_conflict`: A purge does not fail on a concurrent block store or owner write

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::a_sync_storing_a_walked_block_again_does_not_fail_the_purge", "db::collection::purge::a_truncate_chunk_reruns_after_a_conflicting_owner_write"]

A purge commits successfully even when another transaction stores one of the blocks the purge is walking (as a P2P sync recreating a block would) and commits first: the attempt runs again. A truncate chunk, filtered or not, does the same when a merge adopts a block it decided to delete.

# `defra.purge.count`: Purged documents are counted once they commit

- **Parents:** ["defra.purge"]
- **Scenarios:** ["db::collection::purge::the_count_names_each_stored_document_once", "mutation::purge::purge_counts_each_stored_document_once", "mutation::purge::a_discarded_purge_removes_nothing", "mutation::purge::a_purge_in_a_transaction_lands_on_commit", "mutation::purge::a_purge_in_a_transaction_is_invisible_until_commit", "db::collection::purge::a_purge_in_a_transaction_conflicts_with_a_definition_change"]

A DocID that names no document is not an error and is not counted: purging `[alice, bob, alice]` succeeds and counts 2. A purge run in a caller's transaction removes nothing until that transaction commits, and nothing at all if it is discarded; it validates the collection's definition as every write there does, so a definition change committed first makes the transaction's commit conflict.
