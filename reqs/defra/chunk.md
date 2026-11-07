# `defra.chunk`: Chunked String fields

- **Parents:** ["defra"]

A `@chunk` String field is split into a generated read-only chunk collection, with a `_<field>_chunks` list on the parent.

# `defra.chunk.declare`: Declaring a chunked field

- **Parents:** ["defra.chunk"]

What `@chunk` generates, and where it is allowed.

# `defra.chunk.declare.collection`: A `@chunk` field generates a chunk collection

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_collection::chunk_generates_a_queryable_collection"]

Adding `type Article { title: String body: String @chunk(maxChars: 64) }` creates the collection `Article__body_chunk`, which is queryable. Before any document is added, `Article__body_chunk { _docID _sourceID text hash }` and `Article { _body_chunks }` both return `[]`.

# `defra.chunk.declare.definition`: The chunk collection is listed with its source

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_definition::the_chunk_collection_is_listed_with_its_source"]

Listing the collections named `Article__body_chunk` returns one active, materialized collection whose `ChunkOf` is `{"Collection": "Article", "Field": "body"}`, with the fields `_docID`, `_sourceID`, `source`, `hash` and `text` and an index on `_sourceID` and `hash`. The CLI cannot list by name, so this is not checked there.

# `defra.chunk.declare.string_only`: `@chunk` is refused off a String field

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_string_only::chunk_is_refused_off_a_string_field"]

Adding `type Counter { n: Int @chunk }` fails with `@chunk is only allowed on a last-writer-wins String field. Field: Counter.n`.

# `defra.chunk.declare.list_name`: `@chunk` is refused where its list field name would be invalid

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_list_name::chunk_is_refused_on_a_field_whose_list_name_would_start_with_two_underscores"]

GraphQL names cannot start with `__`, so a field whose name starts with `_` cannot be chunked. Adding `type Note { _x: String @chunk }` fails with `@chunk is not allowed on a field whose name starts with "_", because its list field __x_chunks would start with "__". Field: Note._x`. Nothing is stored: `type Note { _x: String }` can be added afterwards.

# `defra.chunk.declare.unknown_argument`: `@chunk` refuses an argument it does not take

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_unknown_argument::chunk_is_refused_with_an_unknown_argument"]

Adding `type Note { body: String @chunk(size: 100) }` fails with `@chunk has no argument size. It takes maxChars and overlap. Field: Note.body`. Nothing is stored: `type Note { body: String }` can be added afterwards.

# `defra.chunk.declare.repeated_argument`: `@chunk` refuses an argument given twice

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_repeated_argument::chunk_is_refused_with_an_argument_given_twice"]

Adding `type Note { body: String @chunk(maxChars: 64, maxChars: 10) }` fails with `@chunk has maxChars twice. Field: Note.body`. Nothing is stored: `type Note { body: String }` can be added afterwards.

# `defra.chunk.declare.once`: `@chunk` may appear once on a field

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_once::a_second_chunk_on_one_field_is_refused"]

Adding `type Note { body: String @chunk(maxChars: 64) @chunk(maxChars: 100) }` fails with `@chunk can only be used once on a field. Field: Note.body`. Nothing is stored: `type Note { body: String }` can be added afterwards.

# `defra.chunk.declare.no_view`: `@chunk` is refused in a view

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_no_view::chunk_is_refused_in_a_view_and_nothing_is_stored"]

A view's documents come from its query, so it has no writes to cut chunks on. Adding a view over `Source { title body }` with the SDL `type SourceView { title: String body: String @chunk(maxChars: 64) }` fails with `@chunk is not allowed in a view. Field: SourceView.body`. Nothing is stored: `type SourceView { title: String } type SourceView__body_chunk { text: String }` can be added afterwards.

# `defra.chunk.declare.already_exists`: Re-adding a chunked type fails like a plain one

- **Parents:** ["defra.chunk.declare"]
- **Scenarios:** ["chunk::declare_already_exists::re_adding_a_chunked_type_reports_that_the_collection_already_exists"]

Adding the `Article` schema with `body: String @chunk(maxChars: 64)` a second time fails with `collection already exists. Name: Article`, the same error as re-adding a type without `@chunk`.

# `defra.chunk.protect`: Chunks are read-only for users

- **Parents:** ["defra.chunk"]

Users cannot write a chunk collection or a chunk list.

# `defra.chunk.protect.no_mutations`: The chunk collection has no mutations

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_no_mutations::generated_collection_has_no_mutations"]

The GraphQL mutation type has the mutations of `Article` and none for `Article__body_chunk`.

# `defra.chunk.protect.list_input`: The chunk list is not a mutation input

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_list_input::the_chunk_list_is_not_a_mutation_input"]

The `ArticleMutationInputArg` type has the input field `body` and no `_body_chunks`.

# `defra.chunk.protect.api_writes`: The collection API cannot write chunks or the chunk list

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_api_writes::the_collection_api_cannot_write_chunks_or_the_chunk_list"]

Adding a document to `Article__body_chunk` fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`. Adding an `Article` with `_body_chunks`, or updating one to set it, fails with `Article._body_chunks is read-only: it is set from body by every write that changes it`. After these failures `Article__body_chunk` holds only the chunk of the article's own text `Some text.`, and the article's `_body_chunks` lists that chunk's hash twice, as the text's hash and as the one chunk.

# `defra.chunk.protect.chunk_documents`: Chunk documents and chunk settings cannot be changed

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_chunk_documents::chunk_documents_and_settings_are_read_only"]

For a chunk document of an existing article, updating it or deleting it fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`, and truncating `Article__body_chunk` fails with `Article__body_chunk holds generated chunks and is emptied only with its parent`. The GraphQL mutation `add_Article__body_chunk` fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`, and `update_Article` with `_body_chunks` in its input fails with `Article._body_chunks is read-only: it is set from body by every write that changes it`. Patching `/Article/ChunkedFields/0/MaxChars` fails with `invalid patch: collection chunk settings cannot be mutated. Collection: Article`. After all of these the chunks, sorted by hash, are exactly as they were.

# `defra.chunk.write`: Writing a chunked field

- **Parents:** ["defra.chunk"]

Every write to an `Article` writes, replaces or removes its chunks in the same transaction. The parent's `_body_chunks` lists the hash of the whole text and then the hash of each chunk in order, and `Article__body_chunk` holds exactly those chunks.

# `defra.chunk.write.graphql`: GraphQL create, update and delete keep the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_graphql::graphql_create_update_and_delete_keep_chunks_exact"]

Creating an `Article` with a six-paragraph body gives a `_body_chunks` list of seven hashes, and `Article__body_chunk` holds exactly the six hashes after the first. Updating the body to four paragraphs gives a list of five hashes and exactly those four chunks. Deleting the article leaves `Article__body_chunk` empty.

# `defra.chunk.write.transaction`: An explicit transaction writes chunks when it commits

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_transaction::an_explicit_transaction_writes_chunks_on_commit"]

An `add_Article` with a five-paragraph body inside an explicit transaction leaves `Article__body_chunk` empty outside it. After the transaction commits, `_body_chunks` lists six hashes and `Article__body_chunk` holds exactly the five after the first.

# `defra.chunk.write.document_api`: The document API keeps the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_document_api::document_api_writes_keep_chunks_exact"]

Adding an `Article` with a five-paragraph body through the document API gives six hashes in `_body_chunks` and exactly the five chunks after the first. Updating its body to three paragraphs gives four hashes and exactly those three chunks. Deleting the document leaves `Article__body_chunk` empty.

# `defra.chunk.write.revert`: Changing a text back shows the original chunks again

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_revert::reverting_the_text_reuses_the_original_chunks"]

An article with a five-paragraph body has five chunk documents. Updating the body to three other paragraphs leaves three chunk documents, none with a `_docID` of the first five. Updating it back to the first body shows the same five chunk documents again, each with its original `_docID`, `hash` and `text`.

# `defra.chunk.write.locality`: Editing one paragraph changes one chunk

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_edit_locality::editing_one_paragraph_changes_one_chunk"]

With `@chunk(maxChars: 160)` and a 40-paragraph body, changing one word in paragraph 20 replaces the one chunk document that holds paragraph 20 with a new one, and every other chunk document keeps its `_docID`.

# `defra.chunk.write.signed`: A node that signs its blocks keeps the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_signed::a_signing_node_stores_the_list_of_the_first_update", "chunk::write_signed::a_signing_node_stores_the_lists_of_an_array_create", "chunk::write_signed::a_signing_node_stores_the_list_of_an_upsert_create"]

On a node that signs its blocks, creating an `Article` with a six-paragraph body gives a signed `_body_chunks` block. Updating the body to four paragraphs then gives a `_body_chunks` list of five hashes: the whole text's, then the four chunks'. `Article__body_chunk` holds exactly those four. The same update gives a list of five hashes for each article of a two-article `add_Article` and for an article an `upsert_Article` created.

# `defra.chunk.write.concurrent`: Concurrent writes leave the chunks of the winning text

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_concurrent::concurrent_updates_leave_the_chunks_of_the_winning_text", "chunk::write_concurrent::concurrent_updates_and_a_delete_leave_no_stray_chunk"]

After eight updates of one article run at the same time, `body` is the text of one of them, `_body_chunks` has one hash for the text and one per paragraph, and `Article__body_chunk` holds exactly the paragraphs of that text, each with its blank line except the last. After six such updates and a delete run at the same time, either the article and all its chunks are gone, or the state is that of one winning update. The text the article started with never remains.

# `defra.chunk.purge`: Removing a parent removes its chunks

- **Parents:** ["defra.chunk"]

Chunks live only as long as the parent that has them.

# `defra.chunk.purge.truncate`: Truncating the parent empties its chunks

- **Parents:** ["defra.chunk.purge"]
- **Scenarios:** ["chunk::purge_truncate::truncating_the_parent_empties_its_chunks"]

After truncating `Article`, `Article__body_chunk` is empty.

# `defra.chunk.replicate`: Chunks replicate with the parent

- **Parents:** ["defra.chunk"]

A peer gets the same chunk documents as the node that wrote them.

# `defra.chunk.replicate.chunks`: A peer has the same chunks

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_chunks::a_peer_has_the_same_chunk_documents"]

With `Article` and `Article__body_chunk` replicated from one node to another, an article created on the first shows the same `_body_chunks` list and the same chunk hashes on the second. After an update to a shorter text, the second node shows the new list and exactly the new chunks.

# `defra.chunk.replicate.delete`: A parent delete replicates with its chunks

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_delete::a_parent_delete_replicates_with_its_chunks"]

After an article and its chunks reach the second node, deleting the article on the first leaves the second with no `Article` and an empty `Article__body_chunk`.

# `defra.chunk.replicate.parent_only`: A replicator of the parent ships its chunks

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_parent_only::a_replicator_of_the_parent_ships_its_chunks"]

A replicator set up for `Article` alone gives the second node the same `_body_chunks` list and exactly the same chunks. The CLI sets the replicator with `--collection Article`.

# `defra.chunk.replicate.filtered`: A filtered replicator ships the chunks of the parents it matches

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_filtered::a_filtered_replicator_ships_the_chunks_of_a_matching_parent", "chunk::replicate_filtered::a_filtered_replicator_ships_the_new_chunks_of_an_edited_matching_parent", "chunk::replicate_filtered::a_filtered_replicator_replays_the_chunks_of_existing_matching_parents", "chunk::replicate_filtered::a_matching_parent_delete_removes_its_chunks_on_the_filtered_peer", "chunk::replicate_filtered::an_unmatched_parent_edit_and_delete_send_nothing", "chunk::replicate_filtered::an_explicit_transaction_ships_the_chunks_of_a_matching_parent", "chunk::replicate_filtered::an_explicit_transaction_edit_and_delete_reach_the_filtered_peer"]

With `type Article { kind: String @immutable body: String @chunk(maxChars: 64) }` and a replicator for `Article` with the filter `{"kind": {"_eq": "public"}}`, a public article written on the first node reaches the second with the same `_body_chunks` list and exactly its chunks, and a secret article never reaches it. After an edit the second node shows the new list and exactly the new chunks, and after a delete it shows no article and no chunks. This holds for single writes, for writes in an explicit transaction, and for articles that existed before the replicator was added. Editing and deleting a secret article sends nothing: once a later public article arrives, the second node shows only the public articles. The CLI sets the filter with `--filter`. Go has no filtered replicators, so a Go node reports these scenarios as unsupported.

# `defra.chunk.backup`: Backups and chunks

- **Parents:** ["defra.chunk"]

A backup holds the parent's text and not its chunks.

# `defra.chunk.backup.round_trip`: A backup round trip rebuilds the chunks

- **Parents:** ["defra.chunk.backup"]
- **Scenarios:** ["chunk::backup_round_trip::a_backup_round_trip_rebuilds_the_chunks"]

Exporting a database with one article gives exactly `{"Article":[{"_docID":"<id>","_docIDNew":"<id>","body":"<text>","title":"t"}]}`: no chunk collection and no `_body_chunks`. Importing that content into an empty node with the same schema gives the same `_body_chunks` list and exactly the same chunks.

# `defra.chunk.access`: Access to chunks follows the parent

- **Parents:** ["defra.chunk"]

Under a local policy a chunk is shown to the actors who can read its parent.

# `defra.chunk.access.parent`: Only readers of the parent see its chunks

- **Parents:** ["defra.chunk.access"]
- **Scenarios:** ["chunk::access_parent::chunks_follow_their_parents_access"]

An article created by Alice under a policy shows its chunks to Alice: `Article__body_chunk` lists them, each `source { title }` is the article's, and `_commits` of a chunk, `COUNT`, a read at a commit `cid`, `_documentArrivals`, `_version` and `@explain` all see them. For Bob, who has no relation to the article, and for a request with no identity, `Article__body_chunk` is empty, `_commits` of the chunk is empty, `COUNT` is 0, the read at the commit is empty, `_documentArrivals` has no entries and `_version` is empty. The `@explain(type: execute)` plan for Bob contains a `permissionFilterNode`.

# `defra.chunk.access.grant`: A grant and a delete on the parent carry over to its chunks

- **Parents:** ["defra.chunk.access"]
- **Scenarios:** ["chunk::access_grant::a_reader_of_the_parent_sees_its_chunks_until_it_is_deleted"]

After Alice adds the `reader` relation on her article for Bob, Bob sees the same chunks as Alice and their `_commits`. After Alice deletes the article, she sees no chunks and `_commits` of a chunk is empty.
