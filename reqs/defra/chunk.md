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

Adding a document to `Article__body_chunk` fails with `Article__body_chunk holds the chunks of Article.body and is written only by writes to Article`. Adding an `Article` with `_body_chunks`, or updating one to set it, fails with `Article._body_chunks is read-only: it is set from body by every write that changes it`. After these failures `Article__body_chunk` is still empty and the article's `_body_chunks` is `null`.

# `defra.chunk.protect.admin_ops`: Admin operations cannot change the chunk collection

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_admin_ops::index_changes_to_the_chunk_collection_are_refused", "chunk::protect_admin_ops::collection_changes_to_the_chunk_collection_are_refused"]

Creating an index or an encrypted index on `Article__body_chunk`, or dropping its generated index, fails with `Article__body_chunk holds the chunks of Article.body and is changed only through Article`, and the generated index is still listed. Truncating `Article__body_chunk`, making its version active, and setting a migration from it fail with the same error. An index on `Article` is still created and `Article` can still be truncated.

# `defra.chunk.protect.rename`: A patch cannot rename a parent or its chunk collection

- **Parents:** ["defra.chunk.protect"]
- **Scenarios:** ["chunk::protect_rename::renaming_the_parent_or_the_chunk_collection_is_refused"]

Chunks find their parent by name. A patch that renames `Article` to `Story`, or `Article__body_chunk` to `Story`, fails with an error containing `Changed: the collection name`.
