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
