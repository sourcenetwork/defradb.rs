# `go.query`: GraphQL query behaviour

- **Parents:** ["go"]

GraphQL query behaviour.

# `go.query.simple`: Queries on a single collection

- **Parents:** ["go.query"]

Queries against one collection with no relations: selection, filtering, ordering, pagination, aggregates, grouping, version and CID access, and similarity. Unless a test defines its own schema, the collection is `Users { Name: String, Email: String, Age: Int, HeightM: Float, Verified: Boolean, CreatedAt: DateTime }`.

# `go.query.simple.select`: Field selection

- **Parents:** ["go.query.simple"]

What a plain collection query returns and which selections it rejects.

# `go.query.simple.select.fields`: Plain query returns every document with the selected fields

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/simple_test.go::TestQuerySimple", "query/simple/simple_test.go::TestQuerySimpleWithMultipleRows", "query/simple/simple_test.go::TestQuerySimpleWithSomeDefaultValues", "query/simple/simple_test.go::TestQuerySimpleWithDefaultValue", "query/simple/with_restart_test.go::TestQuerySimpleWithRestart"]

`query { Users { Name Age } }` returns one object per document holding exactly the selected fields (including `_docID` when selected); fields never set on a document come back as `null`. Documents added after a node restart are returned like any others.

# `go.query.simple.graphql`: GraphQL document features

- **Parents:** ["go.query.simple"]

Operation names, fragments and variables in the request document.

# `go.query.simple.graphql.fragments`: Named and inline fragments

- **Parents:** ["go.query.simple.graphql"]
- **Go-Tests:** ["query/simple/with_fragments_test.go::TestQuerySimple_WithFragments_Succeeds", "query/simple/with_fragments_test.go::TestQuerySimple_WithNestedFragments_Succeeds", "query/simple/with_fragments_test.go::TestQuerySimple_WithFragmentSpreadAndSelect_Succeeds", "query/simple/with_fragments_test.go::TestQuerySimple_WithMissingFragment_ReturnsError", "query/simple/with_fragments_test.go::TestQuerySimple_WithFragmentWithInvalidField_ReturnsError", "query/simple/with_fragments_test.go::TestQuerySimple_WithFragmentWithAggregate_Succeeds", "query/simple/with_fragments_test.go::TestQuerySimple_WithFragmentWithVariables_Succeeds", "query/simple/with_fragments_test.go::TestQuerySimple_WithInlineFragment_Succeeds"]

`fragment X on Users { ... }` spreads, nested spreads, spreads mixed with plain fields, `... on Users { }` inline fragments and `fragment X on Query { COUNT(Users: {}) }` / `Users(filter: $filter)` with variables all expand as if written inline. A spread of an undefined fragment fails with `Unknown fragment "UserInfo".`; a fragment selecting an undefined field fails with `Cannot query field "Score" on type "Users".`.

# `go.query.simple.graphql.operation-name`: Choosing among multiple operations

- **Parents:** ["go.query.simple.graphql"]
- **Go-Tests:** ["query/simple/with_operation_name_test.go::TestQuerySimpleMultipleOperationsWithOperationName", "query/simple/with_operation_name_test.go::TestQuerySimpleMultipleOperationsWithNoOperationName_ReturnsError"]

A document with `query UsersByName {...} query UsersByAge {...}` executes only the operation named by the request's operation name; without one it fails with `Must provide operation name if query contains multiple operations.`.

# `go.query.simple.graphql.variables`: Query variables

- **Parents:** ["go.query.simple.graphql"]
- **Go-Tests:** ["query/simple/with_variables_test.go::TestQuerySimpleWithNonNullVariable", "query/simple/with_variables_test.go::TestQuerySimpleWithVariableDefaultValue", "query/simple/with_variables_test.go::TestQuerySimpleWithNonNullVariable_ReturnsErrorWhenNull", "query/simple/with_variables_test.go::TestQuerySimpleWithVariableDefaultValueOverride", "query/simple/with_variables_test.go::TestQuerySimpleWithOrderVariable", "query/simple/with_variables_test.go::TestQuerySimpleWithAggregateCountVariable"]

Variables of type `Int!`, `Ordering!`, `[UsersOrderArg]` and `Users__CountSelector` substitute into `filter`, `order` and `COUNT(Users: $v)`; declared defaults (`$age: Int = 50`) apply when the variable is absent and are overridden when supplied. A missing required variable fails with `Variable "$age" of required type "Int!" was not provided.`.

# `go.query.simple.filter`: The `filter` argument

- **Parents:** ["go.query.simple"]

Document filtering with comparison, pattern, set and logical operators.

# `go.query.simple.filter.equality`: `_eq` and `_neq`

- **Parents:** ["go.query.simple.filter"]

Equality operators on scalar fields.

# `go.query.simple.filter.range`: `_gt`, `_geq`, `_lt`, `_leq`

- **Parents:** ["go.query.simple.filter"]

Ordered comparison on Int, Float and DateTime fields.

# `go.query.simple.filter.pattern`: `_like` family

- **Parents:** ["go.query.simple.filter"]

SQL-style `%` wildcard matching on strings and blobs.

# `go.query.simple.filter.logical`: `_and`, `_or`, `_not`

- **Parents:** ["go.query.simple.filter"]

Combining filter conditions.

# `go.query.simple.filter.null-input`: `null` passed as an argument or filter value

- **Parents:** ["go.query.simple.filter"]

How explicit `null` inputs are treated.

# `go.query.simple.doc-id`: Selecting documents by DocID

- **Parents:** ["go.query.simple"]

The `docID` argument and `_docID` filter.

# `go.query.simple.order`: The `order` argument

- **Parents:** ["go.query.simple"]

Sorting query results.

# `go.query.simple.aggregate`: Top-level aggregates over a collection

- **Parents:** ["go.query.simple"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX` as root query fields.

# `go.query.simple.group`: `groupBy`

- **Parents:** ["go.query.simple"]

Grouping documents and the `GROUP` sub-selection.

# `go.query.simple.group.unselected-key`: Grouping without selecting the grouped field

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_unselected_field_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupField", "query/simple/with_group_unselected_field_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupField"]

`Users(groupBy: [Age]) { GROUP { Name } }` (also for a String key such as `Email`) still returns one row per distinct key, each holding only its `GROUP` documents.

# `go.query.simple.group.order`: Ordering groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberWithGroupOrder", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberWithGroupOrderDescending", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringAndOrderDescendingWithGroupNumberWithGroupOrder", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanThenInnerOrderDescending", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndOrderAscendingThenInnerOrderDescending"]

Root `order` on a grouped field sorts the group rows; `GROUP(order: {Age: ASC|DESC})` sorts documents within each group, at nested levels too, independently of the parent order.

# `go.query.simple.group.aggregate`: Aggregates over `GROUP`

- **Parents:** ["go.query.simple.group"]

`COUNT(GROUP: ...)`, `SUM`, `AVG`, `MIN`, `MAX` computed per group.

# `go.query.simple.cid`: Querying a past version with `cid`

- **Parents:** ["go.query.simple"]

The `cid` argument selects document state at a commit.

# `go.query.simple.version`: The embedded `_version` commit field

- **Parents:** ["go.query.simple"]

Commit metadata selected per document.

# `go.query.simple.similarity`: The `SIMILARITY` field

- **Parents:** ["go.query.simple"]

Cosine similarity between a numeric array field and a query vector.

# `go.query.simple.similarity.validation`: Invalid `SIMILARITY` arguments

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityOnUndefinedField_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityAndWrongVectorValueType_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityAndNoArguments_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithIntSimilarityDifferentVectorLength_ShouldError"]

`SIMILARITY(pointsList: ...)` on an undefined field fails with `Unknown argument "pointsList" on field "SIMILARITY" of type "User".`; float elements for an `[Int!]` field fail with `Expected type "Int", found 1.1.` per element; `SIMILARITY` with no argument fails with `similarity requires a target field argument`; a vector of a different length fails with `source and vector must be of the same length. Source: 3, Vector: 4`.

# `go.query.simple.similarity.non-numeric-target`: `SIMILARITY` on a non-numeric array field

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityAndWrongFieldType_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityInFragmentAndWrongFieldType_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityInInlineFragmentAndWrongFieldType_ShouldError", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityInMutationAndWrongFieldType_ShouldError"]

`SIMILARITY(pets: {vector: [...]})` targeting a `[String!]` field fails with `similarity can only target a numeric array field. Field: pets, Type: [String!]`, whether written directly, in a named fragment on a related type, in an inline fragment, or in a mutation's selection set.

# `go.query.simple.similarity.vector-index-fallback`: Similarity queries the vector index cannot serve

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_AscendingOrderFullScans", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_NoOrderDoesNotUseIndex", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_WrongLengthQueryErrors"]

On a vector-indexed field, a similarity query not ordered by similarity `DESC` does not use the index (no index fetch under explain) and returns correct results with a `WarningCodeVectorIndexUnused` warning. Ordering by similarity `ASC` scans the whole collection; with no order, a `limit` may stop the scan early, so the query need not read every document; a query vector whose length differs from the index dimensions fails with `source and vector must be of the same length`.
