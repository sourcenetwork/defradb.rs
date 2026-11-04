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
