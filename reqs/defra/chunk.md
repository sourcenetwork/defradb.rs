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

Listing the collections named `Article__body_chunk` returns one active, materialized collection whose `ChunkOf` is `{"Collection": "Article", "Field": "body"}`, with the fields `_docID`, `_sourceID`, `generation`, `hash`, `source` and `text` and an index on `_sourceID` and `hash`. The CLI cannot list by name, so this is not checked there.

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

# `defra.chunk.protect.admin_ops`: Admin operations cannot change the chunk collection

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_admin_ops::index_changes_to_the_chunk_collection_are_refused", "chunk::protect_admin_ops::collection_changes_to_the_chunk_collection_are_refused"]

Creating an index or an encrypted index on `Article__body_chunk`, or dropping its generated index, fails with `Article__body_chunk holds the chunks of Article.body and is changed only through Article`, and the generated index is still listed. Making its version active and setting a migration from it fail with the same error. An index on `Article` is still created. Truncating `Article__body_chunk` is covered by `defra.chunk.protect.chunk_documents`.

# `defra.chunk.protect.rename`: A patch cannot rename a parent or its chunk collection

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_rename::renaming_the_parent_or_the_chunk_collection_is_refused"]

Chunks find their parent by name. A patch that renames `Article` to `Story`, or `Article__body_chunk` to `Story`, fails with an error containing `Changed: the collection name`.

# `defra.chunk.protect.chunk_documents`: Chunk documents and chunk settings cannot be changed

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_chunk_documents::chunk_documents_and_settings_are_read_only"]

For a chunk document of an existing article, updating it or deleting it fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`, and truncating `Article__body_chunk` fails with `Article__body_chunk holds the chunks of Article.body and is changed only through Article`. The GraphQL mutation `add_Article__body_chunk` fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`, and `update_Article` with `_body_chunks` in its input fails with `Article._body_chunks is read-only: it is set from body by every write that changes it`. Patching `/Article/ChunkedFields/0/MaxChars` fails with `invalid patch: collection chunk settings cannot be mutated. Collection: Article`. After all of these the chunks, sorted by hash, are exactly as they were.

# `defra.chunk.write`: Writing a chunked field

- **Parents:** ["defra.chunk"]

Every write to an `Article` writes, replaces or removes its chunks in the same transaction. The parent's `_body_chunks` lists the hash of the whole text and then the hash of each chunk in order, and `Article__body_chunk` holds exactly those chunks: a chunk the new text no longer uses is deleted, so storage follows the size of the text, not the number of edits.

# `defra.chunk.write.graphql`: GraphQL create, update and delete keep the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_graphql::graphql_create_update_and_delete_keep_chunks_exact"]

Creating an `Article` with a six-paragraph body gives a `_body_chunks` list of seven hashes, and `Article__body_chunk` holds exactly the six hashes after the first. Updating the body to four paragraphs gives a list of five hashes and exactly those four chunks. Deleting the article leaves `Article__body_chunk` empty.

# `defra.chunk.write.transaction`: An explicit transaction writes chunks when it commits

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_transaction::an_explicit_transaction_writes_chunks_on_commit", "chunk::write_transaction::an_explicit_transaction_replaces_and_deletes_chunks_on_commit", "chunk::write_transaction::a_discarded_transaction_leaves_the_chunks_as_they_were"]

An `add_Article` with a five-paragraph body inside an explicit transaction leaves `Article__body_chunk` empty outside it. After the transaction commits, `_body_chunks` lists six hashes and `Article__body_chunk` holds exactly the five after the first. An update of an existing article to two paragraphs inside a transaction shows the two new chunks inside it and the old four outside it until it commits, and then only the two. A delete inside a transaction shows no chunks inside it and the two outside it until it commits, and then none. An update and a delete in transactions that are discarded leave the original `_body_chunks` list and chunks.

# `defra.chunk.write.graphql_batch`: GraphQL batch writes and upserts keep every parent's chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_graphql_batch::graphql_batch_writes_keep_each_parents_chunks_exact"]

An `add_Article` with two articles of three and two paragraphs gives lists of four and three hashes, and `Article__body_chunk` holds the chunks of both texts. An `update_Article` with an empty filter that sets both bodies to the same two paragraphs gives each a list of three hashes, and each parent has its own two chunks. A `delete_Article` by filter leaves only the other parent's chunks. An `upsert_Article` that finds an article replaces its chunks, and one that finds none creates an article with its chunks. A `delete_Article` with an empty filter leaves `Article__body_chunk` empty.

# `defra.chunk.write.document_api`: The document API keeps the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_document_api::document_api_writes_keep_chunks_exact"]

Adding an `Article` with a five-paragraph body through the document API gives six hashes in `_body_chunks` and exactly the five chunks after the first. Updating its body to three paragraphs gives four hashes and exactly those three chunks. Deleting the document leaves `Article__body_chunk` empty.

# `defra.chunk.write.document_api_batch`: Document API batch writes and saves keep every parent's chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_document_api_batch::document_api_batch_writes_keep_each_parents_chunks_exact", "chunk::write_document_api_batch::a_document_api_save_keeps_chunks_exact"]

Adding two articles in one document API call gives the chunks of both texts. An update by filter of one replaces only its chunks, and a delete by filter of each removes only its chunks, until `Article__body_chunk` is empty. A save without a docID creates an article with its chunks, and a save with that docID replaces them. The CLI has no save command, so the save is not checked there.

# `defra.chunk.write.encrypted`: An encrypted text gives encrypted chunks

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_encrypted::an_encrypted_field_gives_encrypted_chunks", "chunk::write_encrypted::an_encrypted_document_gives_encrypted_chunks"]

An article added with `body` encrypted, or with the whole document encrypted, has a chunk whose `text` reads in the clear and whose stored `text` delta in `_commits` is not the plain encoding of the text. After an update of the body without encryption options, the new chunk's stored `text` is still encrypted.

# `defra.chunk.write.events`: A parent write publishes its chunks

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_events::a_parent_create_publishes_its_chunks"]

A GraphQL subscription on `Article__body_chunk { text }` gets one event for each of the three chunks of a new three-paragraph article. The CLI has no subscription stream, so this is not checked there.

# `defra.chunk.write.cleanup`: An edit deletes the chunks its text no longer uses

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_cleanup::an_edit_deletes_the_chunks_it_no_longer_uses"]

An article with a three-paragraph body about `rivers` has three chunk documents, each with a null `generation`. Updating the body to its first paragraph followed by two paragraphs about `mountains` keeps the first paragraph's chunk with its `_docID` and deletes the other two. Updating it back to the `rivers` body keeps the first chunk again, and makes the two deleted ones again with new `_docID`s and `generation` 1.

# `defra.chunk.write.revert`: Changing a text back makes its deleted chunks again

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_revert::reverting_the_text_makes_its_chunks_again_under_a_new_generation"]

A deleted document's ID can never be used again, so a chunk made again takes the next `generation`, which gives it a new `_docID`. An article with a five-paragraph body has five chunk documents with a null `generation`. Updating the body to three other paragraphs leaves those three chunk documents, with `_docID`s none of the first five had. Updating it back to the first body shows five chunk documents with the original `hash` and `text`, new `_docID`s and `generation` 1. Changing it away and back once more gives `generation` 2 and new `_docID`s again.

# `defra.chunk.write.locality`: Editing one paragraph changes one chunk

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_edit_locality::editing_one_paragraph_changes_one_chunk"]

With `@chunk(maxChars: 160)` and a 40-paragraph body, changing one word in paragraph 20 replaces the one chunk document that holds paragraph 20 with a new one, and every other chunk document keeps its `_docID`.

# `defra.chunk.write.signed`: A node that signs its blocks keeps the chunks exact

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_signed::a_signing_node_stores_the_list_of_the_first_update", "chunk::write_signed::a_signing_node_stores_the_lists_of_an_array_create", "chunk::write_signed::a_signing_node_stores_the_list_of_an_upsert_create"]

On a node that signs its blocks, creating an `Article` with a six-paragraph body gives a signed `_body_chunks` block. Updating the body to four paragraphs then gives a `_body_chunks` list of five hashes: the whole text's, then the four chunks'. `Article__body_chunk` holds exactly those four, and the `text` block of a chunk is not signed, so every node gives the chunk of one text the same `_docID`. The same update gives a list of five hashes for each article of a two-article `add_Article` and for an article an `upsert_Article` created.

# `defra.chunk.write.concurrent`: Concurrent writes leave the chunks of the winning text

- **Parents:** ["defra.chunk.write"]
- **Scenarios:** ["chunk::write_concurrent::concurrent_updates_leave_the_chunks_of_the_winning_text", "chunk::write_concurrent::concurrent_updates_and_a_delete_leave_no_stray_chunk"]

After eight updates of one article run at the same time, `body` is the text of one of them, `_body_chunks` has one hash for the text and one per paragraph, and `Article__body_chunk` holds exactly the paragraphs of that text, each with its blank line except the last. After six such updates and a delete run at the same time, either the article and all its chunks are gone, or the state is that of one winning update. The text the article started with never remains.

# `defra.chunk.purge`: Removing a parent removes its chunks

- **Parents:** ["defra.chunk"]

Chunks live only as long as the parent that has them.

# `defra.chunk.purge.truncate`: Truncating the parent empties its chunks

- **Parents:** ["defra.chunk.purge"]
- **Scenarios:** ["chunk::purge_truncate::truncating_the_parent_empties_its_chunks", "chunk::purge_truncate::a_filtered_truncate_empties_only_the_matching_parents_chunks"]

After truncating `Article`, `Article__body_chunk` is empty. A truncate filtered on one article's title leaves only the other article's chunks.

# `defra.chunk.purge.docs`: Purging a parent removes its chunks

- **Parents:** ["defra.chunk.purge"]
- **Scenarios:** ["chunk::purge_docs::purging_a_parent_removes_only_its_chunks"]

Purging one of two articles by docID purges one document and leaves only the other article's chunks.

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

# `defra.chunk.replicate.concurrent`: Peers that edit apart end with the chunks of the merged text

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_concurrent::peers_that_edit_apart_end_with_the_chunks_of_the_merged_text", "chunk::replicate_concurrent::a_peer_that_merges_a_newer_text_builds_its_chunks"]

Two unconnected nodes add the same article. One updates its body to three paragraphs about `north`, the other to three about `south`. After they connect and each syncs the article from the other, both show the `south` body and exactly its chunks. The text and the list break their ties apart, so both show the `_body_chunks` list the `north` node wrote: the same list on both nodes, which readers do not trust for the chunks. A node that syncs a newer body it has no chunks for shows that body's list and exactly its chunks.

# `defra.chunk.replicate.cleanup`: A peer deletes and makes again the chunks the writer does

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_cleanup::a_replicated_peer_follows_an_edit_and_its_revert", "chunk::replicate_cleanup::peers_that_edit_apart_and_revert_hold_the_same_chunks"]

With `Article` replicated from one node to another, an article with a three-paragraph body about `rivers` shows the same chunk documents on both. After an update to two paragraphs about `mountains` on the first node, both show exactly the `mountains` chunks under the same `_docID`s. After an update back to `rivers`, both show the `rivers` chunks at `generation` 1 under the same new `_docID`s. A peer's delete of a chunk merges once its parent's text here no longer uses it, and waits while it does.

Two unconnected nodes add the same two-paragraph article. One updates it to three paragraphs about `north`, the other to three about `south`. After each syncs the article from the other, both show the `south` chunks under the same `_docID`s. When the first node then changes the body back to the starting text and the second syncs it, both show the starting chunks at `generation` 1 under the same `_docID`s, since both had deleted the first ones.

# `defra.chunk.replicate.signed`: Signing peers that write the same text show each chunk once

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_signed::signing_peers_that_write_the_same_text_show_each_chunk_once"]

Two nodes that sign their blocks share an article. While disconnected, both update its body to the same three paragraphs. After they reconnect and the second syncs the article and the first node's chunk documents, the second shows the body and exactly its three chunks, each once.

# `defra.chunk.replicate.doc_sync`: A document sync brings the parent's chunks

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_doc_sync::a_document_sync_brings_the_parents_chunks"]

A second node that syncs one article by its docID from the first gets the article and exactly its chunks in `Article__body_chunk`.

# `defra.chunk.replicate.filtered`: A filtered replicator ships a matching parent's chunks

- **Parents:** ["defra.chunk.replicate"]
- **Scenarios:** ["chunk::replicate_filtered::a_filtered_replicator_ships_the_matching_parents_chunks", "chunk::replicate_filtered::a_filtered_replicator_replays_the_matching_parents_chunks"]

A replicator for `Article` with the filter `{kind: {_eq: "public"}}` gives the second node the public article and exactly its chunks, and neither the private article nor its chunks. It holds whether the articles are written after the replicator is added or before, when adding it replays them.

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
