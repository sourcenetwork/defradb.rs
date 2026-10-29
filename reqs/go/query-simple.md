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

# `go.query.simple.select.unknown-field`: Selecting an undefined field is rejected

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/simple_test.go::TestQuerySimpleWithUndefinedField", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityOnQuery_ShouldError"]

Selecting a field the type does not define fails validation with `Cannot query field "ThisFieldDoesNotExists" on type "Users".`; `query { SIMILARITY }` at the root fails with `Cannot query field "SIMILARITY" on type "Query".`.

# `go.query.simple.select.collection-isolation`: A collection query sees only its own documents

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/simple_test.go::TestQuerySimple_WithDeletedDocsInCollection2_ShouldNotYieldDeletedDocsOnCollection1Query", "query/simple/with_multiple_types_test.go::TestSimple_WithSevenDummyTypesBefore", "query/simple/with_multiple_types_test.go::TestSimple_WithEightDummyTypesBefore", "query/simple/with_multiple_types_test.go::TestSimple_WithEightDummyTypesBeforeInSplitDeclaration", "query/simple/with_multiple_types_test.go::TestSimple_WithEightDummyTypesAfter", "query/simple/with_multiple_types_test.go::TestSimple_WithSevenDummyTypesBeforeAndOneAfter"]

`query { User { ... } }` returns exactly that collection's live documents however many other collections exist and in whatever order or SDL block they were declared (seven or eight extra types before or after, Go #2810); deleted documents of the next collection never appear in it (Go #3242).

# `go.query.simple.select.alias`: Field and root aliases rename result keys

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/simple_test.go::TestQuerySimpleWithAlias", "query/simple/with_operation_alias_test.go::TestQuerySimpleWithOperationAlias"]

`Users { username: Name age: Age }` returns the values under the alias keys, and `allUsers: Users { ... }` returns the document list under `allUsers`.

# `go.query.simple.select.typename`: `__typename` meta field

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/with_typename_test.go::TestQuerySimpleWithTypeName", "query/simple/with_typename_test.go::TestQuerySimpleWithAliasedTypeName", "query/simple/with_group_typename_test.go::TestQuerySimpleWithGroupByWithTypeName", "query/simple/with_group_typename_test.go::TestQuerySimpleWithGroupByWithChildTypeName"]

Selecting `__typename` (also under an alias such as `t1: __typename`) returns `"Users"` on every document, on every group row of a `groupBy` query, and on every document inside `GROUP`.

# `go.query.simple.select.show-deleted`: `showDeleted` and the `_deleted` field

- **Parents:** ["go.query.simple.select"]
- **Go-Tests:** ["query/simple/with_deleted_field_test.go::TestQuerySimple_WithDeletedField", "query/simple/with_cid_test.go::TestQuerySimple_WithCidAfterDeleteOperation_ShouldReturnUser"]

`User(showDeleted: true) { _deleted name }` returns deleted documents alongside live ones, with `_deleted: true` on the deleted ones; querying with the CID of the delete commit and `showDeleted: true` returns the document with `_deleted: true`.

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

# `go.query.simple.filter.equality.eq`: `_eq` on a non-null value

- **Parents:** ["go.query.simple.filter.equality"]
- **Go-Tests:** ["query/simple/with_filter/with_eq_int_test.go::TestQuerySimpleWithIntEqualsFilterBlock", "query/simple/with_filter/with_eq_float_test.go::TestQuerySimpleWithFloatEqualsFilterBlock", "query/simple/with_filter/with_eq_string_test.go::TestQuerySimpleWithStringFilterBlock", "query/simple/with_filter/with_eq_string_test.go::TestQuerySimpleWithStringFilterBlockAndSelect_SelectSameFieldAsFilterWithMatch", "query/simple/with_filter/with_eq_string_test.go::TestQuerySimpleWithStringFilterBlockAndSelect_SelectDifferentFieldThanFilterWithMatch", "query/simple/with_filter/with_eq_string_test.go::TestQuerySimpleWithStringFilterBlockAndSelect_SelectMultipleFieldsButNoMatch", "query/simple/with_filter/with_eq_datetime_test.go::TestQuerySimpleWithDateTimeEqualsFilterBlock", "query/simple/with_filter/with_eq_datetime_test.go::TestQuerySimple_WithNilDateTimeEqualsAndNonNilFilterBlock_ShouldSucceed", "query/simple/with_filter/with_eq_blob_test.go::TestQuerySimple_WithEqOpOnBlobField_ShouldFilter"]

`filter: {Age: {_eq: 21}}` (and the same on Float, String, DateTime and Blob hex such as `"00FF99AA"`) returns only documents whose field equals the value; documents with the field unset never match, the filtered field need not be selected, and no match returns an empty list.

# `go.query.simple.filter.equality.eq-null`: `_eq: null` matches unset fields

- **Parents:** ["go.query.simple.filter.equality"]
- **Go-Tests:** ["query/simple/with_filter/with_eq_int_test.go::TestQuerySimpleWithIntEqualsNilFilterBlock", "query/simple/with_filter/with_eq_float_test.go::TestQuerySimpleWithFloatEqualsNilFilterBlock", "query/simple/with_filter/with_eq_string_test.go::TestQuerySimpleWithStringEqualsNilFilterBlock", "query/simple/with_filter/with_eq_datetime_test.go::TestQuerySimpleWithDateTimeEqualsNilFilterBlock"]

`filter: {Age: {_eq: null}}` on Int, Float, String and DateTime fields returns exactly the documents whose field is null or unset.

# `go.query.simple.filter.equality.neq`: `_neq` on a non-null value

- **Parents:** ["go.query.simple.filter.equality"]
- **Go-Tests:** ["query/simple/with_filter/with_neq_int_test.go::TestQuerySimpleWithIntNotEqualsFilterBlock", "query/simple/with_filter/with_neq_float_test.go::TestQuerySimpleWithFloatNotEqualsFilterBlock", "query/simple/with_filter/with_neq_string_test.go::TestQuerySimpleWithStringNotEqualsFilterBlock", "query/simple/with_filter/with_neq_datetime_test.go::TestQuerySimpleWithDateTimeNotEqualsFilterBlock", "query/simple/with_filter/with_neq_datetime_test.go::TestQuerySimple_WithNilDateTimeNotEqualAndNonNilFilterBlock_ShouldSucceed", "query/simple/with_filter/with_neq_bool_test.go::TestQuerySimpleWithBoolNotEqualsTrueFilterBlock", "query/simple/with_filter/with_neq_bool_test.go::TestQuerySimpleWithBoolNotEqualsFalseFilterBlock"]

`filter: {X: {_neq: v}}` on Int, Float, String, DateTime and Boolean returns every document whose field differs from `v`, including documents where the field is unset (e.g. `Verified: {_neq: true}` returns the `false` and the unset documents).

# `go.query.simple.filter.equality.neq-null`: `_neq: null` matches set fields

- **Parents:** ["go.query.simple.filter.equality"]
- **Go-Tests:** ["query/simple/with_filter/with_neq_int_test.go::TestQuerySimpleWithIntNotEqualsNilFilterBlock", "query/simple/with_filter/with_neq_float_test.go::TestQuerySimpleWithFloatNotEqualsNilFilterBlock", "query/simple/with_filter/with_neq_string_test.go::TestQuerySimpleWithStringNotEqualsNilFilterBlock", "query/simple/with_filter/with_neq_datetime_test.go::TestQuerySimpleWithDateTimeNotEqualsNilFilterBlock", "query/simple/with_filter/with_neq_bool_test.go::TestQuerySimpleWithBoolNotEqualsNilFilterBlock"]

`filter: {X: {_neq: null}}` on Int, Float, String, DateTime and Boolean returns exactly the documents whose field has a value (a Boolean `false` counts as a value).

# `go.query.simple.filter.range`: `_gt`, `_geq`, `_lt`, `_leq`

- **Parents:** ["go.query.simple.filter"]

Ordered comparison on Int, Float and DateTime fields.

# `go.query.simple.filter.range.gt`: `_gt` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Go-Tests:** ["query/simple/with_filter/with_gt_int_test.go::TestQuerySimpleWithIntGreaterThanFilterBlock_ReturnOneAsOneMatches", "query/simple/with_filter/with_gt_int_test.go::TestQuerySimpleWithIntGreaterThanFilterBlock_ReturnNoneAsNoMatch", "query/simple/with_filter/with_gt_int_test.go::TestQuerySimpleWithIntGreaterThanFilterBlock_ReturnAllMultiMatches", "query/simple/with_filter/with_gt_float_test.go::TestQuerySimpleWithFloatGreaterThanFilterBlock_OneMatchingResult", "query/simple/with_filter/with_gt_float_test.go::TestQuerySimpleWithFloatGreaterThanFilterBlock_NoMatchingResult", "query/simple/with_filter/with_gt_float_test.go::TestQuerySimpleWithFloatGreaterThanFilterBlock_AllMatchingResult", "query/simple/with_filter/with_gt_float_test.go::TestQuerySimpleWithFloatGreaterThanFilterBlockWithIntFilterValue", "query/simple/with_filter/with_gt_datetime_test.go::TestQuerySimpleWithDateTimeGTFilterBlockWithEqualValue", "query/simple/with_filter/with_gt_datetime_test.go::TestQuerySimpleWithDateTimeGTFilterBlockWithGreaterValue", "query/simple/with_filter/with_gt_datetime_test.go::TestQuerySimpleWithDateTimeGTFilterBlockWithLesserValue", "query/simple/with_filter/with_gt_datetime_test.go::TestQuerySimple_WithNilDateTimeGTAndNonNilFilterBlock_ShouldSucceed"]

`filter: {Age: {_gt: 20}}` returns documents whose value is strictly greater (zero, one or all may match); a Float field accepts an Int operand (`HeightM: {_gt: 2}`), DateTime compares RFC 3339 instants across offsets, and documents with the field unset never match a non-null operand.

# `go.query.simple.filter.range.geq`: `_geq` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Go-Tests:** ["query/simple/with_filter/with_geq_int_test.go::TestQuerySimpleWithIntGEFilterBlockWithEqualValue", "query/simple/with_filter/with_geq_int_test.go::TestQuerySimpleWithIntGEFilterBlockWithGreaterValue", "query/simple/with_filter/with_geq_float_test.go::TestQuerySimpleWithHeightMGEFilterBlockWithEqualValue", "query/simple/with_filter/with_geq_float_test.go::TestQuerySimpleWithHeightMGEFilterBlockWithLesserValue", "query/simple/with_filter/with_geq_float_test.go::TestQuerySimpleWithHeightMGEFilterBlockWithLesserIntValue", "query/simple/with_filter/with_geq_datetime_test.go::TestQuerySimpleWithDateTimeGEFilterBlockWithEqualValue", "query/simple/with_filter/with_geq_datetime_test.go::TestQuerySimpleWithDateTimeGEFilterBlockWithGreaterValue", "query/simple/with_filter/with_geq_datetime_test.go::TestQuerySimpleWithDateTimeGEFilterBlockWithLesserValue", "query/simple/with_filter/with_geq_datetime_test.go::TestQuerySimple_WithNilDateTimeGEAndNonNilFilterBlock_ShouldSucceed"]

`filter: {Age: {_geq: 32}}` returns documents whose value is greater than or equal to the operand, an equal value included; Float accepts Int operands and fine-grained values like `2.0999999999999`; documents with the field unset do not match.

# `go.query.simple.filter.range.lt`: `_lt` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Go-Tests:** ["query/simple/with_filter/with_lt_int_test.go::TestQuerySimpleWithIntLessThanFilterBlockWithGreaterValue", "query/simple/with_filter/with_lt_float_test.go::TestQuerySimpleWithFloatLessThanFilterBlockWithGreaterValue", "query/simple/with_filter/with_lt_float_test.go::TestQuerySimpleWithFloatLessThanFilterBlockWithGreaterIntValue", "query/simple/with_filter/with_lt_datetime_test.go::TestQuerySimpleWithDateTimeLTFilterBlockWithGreaterValue", "query/simple/with_filter/with_lt_datetime_test.go::TestQuerySimple_WithNilDateTimeLTAndNonNilFilterBlock_ShouldSucceed"]

`filter: {Age: {_lt: 22}}` returns documents whose value is strictly less than the operand (Int operand accepted on Float); documents with the field unset do not match a non-null operand.

# `go.query.simple.filter.range.leq`: `_leq` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Go-Tests:** ["query/simple/with_filter/with_leq_int_test.go::TestQuerySimpleWithIntLEFilterBlockWithEqualValue", "query/simple/with_filter/with_leq_int_test.go::TestQuerySimpleWithIntLEFilterBlockWithGreaterValue", "query/simple/with_filter/with_leq_float_test.go::TestQuerySimpleWithFloatLEFilterBlockWithEqualValue", "query/simple/with_filter/with_leq_float_test.go::TestQuerySimpleWithFloatLEFilterBlockWithGreaterValue", "query/simple/with_filter/with_leq_float_test.go::TestQuerySimpleWithFloatLEFilterBlockWithGreaterIntValue", "query/simple/with_filter/with_leq_datetime_test.go::TestQuerySimpleWithDateTimeLEFilterBlockWithEqualValue", "query/simple/with_filter/with_leq_datetime_test.go::TestQuerySimpleWithDateTimeLEFilterBlockWithGreaterValue", "query/simple/with_filter/with_leq_datetime_test.go::TestQuerySimple_WithNilDateTimeLEAndNonNilFilterBlock_ShouldSucceed"]

`filter: {Age: {_leq: 21}}` returns documents whose value is less than or equal to the operand, an equal value included (Int operand accepted on Float); documents with the field unset do not match a non-null operand.

# `go.query.simple.filter.range.null-operand`: Range operators with a `null` operand

- **Parents:** ["go.query.simple.filter.range"]
- **Go-Tests:** ["query/simple/with_filter/with_gt_int_test.go::TestQuerySimpleWithIntGreaterThanFilterBlockWithNullFilterValue", "query/simple/with_filter/with_geq_int_test.go::TestQuerySimpleWithIntGEFilterBlockWithNilValue", "query/simple/with_filter/with_lt_int_test.go::TestQuerySimpleWithIntLessThanFilterBlockWithNullValue", "query/simple/with_filter/with_leq_int_test.go::TestQuerySimpleWithIntLEFilterBlockWithNullValue", "query/simple/with_filter/with_gt_float_test.go::TestQuerySimpleWithFloatGreaterThanFilterBlockWithNullFilterValue", "query/simple/with_filter/with_geq_float_test.go::TestQuerySimpleWithHeightMGEFilterBlockWithNilValue", "query/simple/with_filter/with_lt_float_test.go::TestQuerySimpleWithFloatLessThanFilterBlockWithNullValue", "query/simple/with_filter/with_leq_float_test.go::TestQuerySimpleWithFloatLEFilterBlockWithNullValue", "query/simple/with_filter/with_gt_datetime_test.go::TestQuerySimpleWithDateTimeGTFilterBlockWithNilValue", "query/simple/with_filter/with_geq_datetime_test.go::TestQuerySimpleWithDateTimeGEFilterBlockWithNilValue", "query/simple/with_filter/with_lt_datetime_test.go::TestQuerySimpleWithDateTimeLTFilterBlockWithNullValue", "query/simple/with_filter/with_leq_datetime_test.go::TestQuerySimpleWithDateTimeLEFilterBlockWithNullValue"]

On Int, Float and DateTime, `null` sorts below every value: `{_gt: null}` returns documents with the field set, `{_geq: null}` returns all documents, `{_lt: null}` returns none, and `{_leq: null}` returns only documents with the field unset.

# `go.query.simple.filter.membership`: `_in` and `_nin`

- **Parents:** ["go.query.simple.filter"]
- **Go-Tests:** ["query/simple/with_filter/with_in_test.go::TestQuerySimpleWithIntInFilter", "query/simple/with_filter/with_in_test.go::TestQuerySimpleWithIntInFilterOnFloat", "query/simple/with_filter/with_in_test.go::TestQuerySimpleWithIntInFilterWithNullValue", "query/simple/with_filter/with_in_blob_test.go::TestQuerySimple_WithInOpOnBlobField_ShouldFilter", "query/simple/with_filter/with_nin_test.go::TestQuerySimpleWithNotInFilter"]

`filter: {Age: {_in: [19, 40, 55]}}` returns documents whose value is in the list; a `null` element also matches documents with the field unset, Int elements match equal Float values (`HeightM: {_in: [21, 21.2]}` matches 21.0 and 21.2), and Blob hex strings work. `_nin: [19, 40, 55, null]` returns documents whose value is set and not in the list.

# `go.query.simple.filter.pattern`: `_like` family

- **Parents:** ["go.query.simple.filter"]

SQL-style `%` wildcard matching on strings and blobs.

# `go.query.simple.filter.pattern.like`: `_like` and `_ilike`

- **Parents:** ["go.query.simple.filter.pattern"]
- **Go-Tests:** ["query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockContainsString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimple_WithCaseInsensitiveLike_ShouldMatchString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockAsPrefixString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimple_WithCaseInsensitiveLikeString_ShouldMatchPrefixString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockAsSuffixString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimple_WithCaseInsensitiveLikeString_ShouldMatchSuffixString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockExactString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimple_WithCaseInsensitiveLikeString_ShouldMatchExactString", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockContainsStringMuplitpleResults", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockHasStartAndEnd", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockHasBoth", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockHasEither", "query/simple/with_filter/with_like_string_test.go::TestQuerySimpleWithLikeStringContainsFilterBlockPropNotSet", "query/simple/with_filter/with_like_blob_test.go::TestQuerySimple_WithLikeOpOnBlobField_ShouldFilter"]

`_like` matches contains (`"%Stormborn%"`), prefix (`"Viserys%"`), suffix (`"%Andals"`), exact and start-and-end (`"Daenerys%Name"`) patterns case-sensitively; `_ilike` does the same ignoring case. Patterns combine under `_and`/`_or`, documents with the field unset never match, and `_like: "%FF99%"` matches a Blob by its hex form.

# `go.query.simple.filter.pattern.nlike`: `_nlike` and `_nilike`

- **Parents:** ["go.query.simple.filter.pattern"]
- **Go-Tests:** ["query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockContainsString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimple_WithNotCaseInsensitiveLikeString_ShouldMatchString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockAsPrefixString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimple_WithNotCaseInsensitiveLikeString_ShouldMatchPrefixString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockAsSuffixString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimple_WithNotCaseInsensitiveLikeString_ShouldMatchSuffixString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockExactString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimple_WithNotCaseInsensitiveLikeString_MatchExactString", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockContainsStringMuplitpleResults", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockHasStartAndEnd", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockHasBoth", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockHasEither", "query/simple/with_filter/with_nlike_string_test.go::TestQuerySimpleWithNotLikeStringContainsFilterBlockPropNotSet"]

`_nlike` (case-sensitive) and `_nilike` (case-insensitive) return the documents that the matching `_like`/`_ilike` pattern would not, for contains, prefix, suffix, exact and start-and-end patterns and under `_and`/`_or`.

# `go.query.simple.filter.logical`: `_and`, `_or`, `_not`

- **Parents:** ["go.query.simple.filter"]

Combining filter conditions.

# `go.query.simple.filter.logical.and-or`: `_and` and `_or`, including list-field `_all`/`_any`

- **Parents:** ["go.query.simple.filter.logical"]
- **Go-Tests:** ["query/simple/with_filter/with_and_test.go::TestQuerySimpleWithIntGreaterThanAndIntLessThanFilter", "query/simple/with_filter/with_and_test.go::TestQuerySimple_WithInlineIntArray_GreaterThanAndLessThanFilter_Succeeds", "query/simple/with_filter/with_or_test.go::TestQuerySimpleWithIntEqualToXOrYFilter", "query/simple/with_filter/with_or_test.go::TestQuerySimple_WithInlineIntArray_EqualToXOrYFilter_Succeeds"]

`_and: [{Age: {_gt: 20}}, {Age: {_lt: 50}}]` returns documents matching every condition and `_or: [...]` documents matching any; on an `[Int]` field, `{FavoriteNumbers: {_all: {...}}}` requires every element to match and `{_any: {...}}` at least one.

# `go.query.simple.filter.logical.not`: `_not`

- **Parents:** ["go.query.simple.filter.logical"]
- **Go-Tests:** ["query/simple/with_filter/with_not_test.go::TestQuerySimple_WithNotEqualToXFilter_NoError", "query/simple/with_filter/with_not_test.go::TestQuerySimple_WithNotAndComparisonXFilter_NoError", "query/simple/with_filter/with_not_test.go::TestQuerySimple_WithNotEqualToXorYFilter_NoError", "query/simple/with_filter/with_not_test.go::TestQuerySimple_WithNotEqualToXAndNotYFilter_NoError", "query/simple/with_filter/with_not_test.go::TestQuerySimple_WithEmptyNotFilter_ReturnError"]
- **Go-Bug-Suspect:** "'_not: {}' returns an empty list with no error although the test is named '..._ReturnError', so the intended result is unclear"

`_not: {cond}` returns the documents not matching `cond`, including compound `_or` conditions and a nested `_not` beside a field condition. `_not: {}` returns no documents.

# `go.query.simple.filter.alias`: Filtering on aliased fields with `_alias`

- **Parents:** ["go.query.simple.filter"]
- **Go-Tests:** ["query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithAliasEqualsFilterBlock_ShouldFilter", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithEmptyAlias_ShouldNotFilter", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithNullAlias_ShouldFilterAll", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithNonObjectAlias_ShouldFilterAll", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithNonExistantAlias_ShouldReturnError", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithNonAliasedField_ShouldMatchFilter", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithCompoundAlias_ShouldMatchFilter", "query/simple/with_filter/with_alias_test.go::TestQuerySimple_WithAliasWithCompound_ShouldMatchFilter"]
- **Go-Bug-Suspect:** "'_alias: null' and '_alias: 1' silently return no documents instead of being ignored or rejected, while the order counterpart '_alias: 1' errors with 'invalid order input'"

`filter: {_alias: {UserAge: {_eq: 21}}}` filters on the selection aliased `UserAge` (or on an unaliased field name such as `Age`), and `_alias` works both inside `_and` and with `_and` inside it. `_alias: {}` does not filter, `_alias: null` and `_alias: 1` return an empty list, and an alias that is not selected fails with `field or alias not found. Name: UserAge`.

# `go.query.simple.filter.null-input`: `null` passed as an argument or filter value

- **Parents:** ["go.query.simple.filter"]

How explicit `null` inputs are treated.

# `go.query.simple.filter.null-input.arguments`: `null` top-level arguments are ignored

- **Parents:** ["go.query.simple.filter.null-input"]
- **Go-Tests:** ["query/simple/with_null_input_test.go::TestQuerySimple_WithNullFilter_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullOrder_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullOrderFields_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullLimit_Succeed", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullOffset_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullDocID_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullDocIDs_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullCID_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullGroupBy_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithNullShowDeleted_Succeeds"]

`Users(filter: null)`, `order: null`, `order: {Name: null}`, `limit: null`, `offset: null`, `docID: null`, `cid: null`, `groupBy: null` and `showDeleted: null` each behave as if the argument were absent and return the documents without error.

# `go.query.simple.filter.null-input.logical`: `null` logical operators and list elements

- **Parents:** ["go.query.simple.filter.null-input"]
- **Go-Tests:** ["query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullOr_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullAnd_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullNot_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullOrElement_ReturnsError", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullAndElement_ReturnsError"]

`filter: {_or: null}`, `{_and: null}` and `{_not: null}` are ignored and return the documents; a `null` element inside the list (`_or: [null]`, `_and: [null]`) fails validation with `Expected "UsersFilterArg!", found null`.

# `go.query.simple.filter.null-input.field`: `null` as a field condition

- **Parents:** ["go.query.simple.filter.null-input"]
- **Go-Tests:** ["query/simple/with_null_input_test.go::TestQuerySimple_WithNullFilterFields_Succeeds", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullOrField_ReturnsError", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullAndField_ReturnsError", "query/simple/with_null_input_test.go::TestQuerySimple_WithFilterWithNullNotField_Succeeds"]

A field condition given as `null` (`filter: {Name: null}`, also inside `_or: [{Name: null}]` and `_and: [{Name: null}]`) is accepted and returns the document whose `Name` is null; `_not: {Name: null}` returns a document whose `Name` is set. Two tests are named `..._ReturnsError` but assert success.

# `go.query.simple.doc-id`: Selecting documents by DocID

- **Parents:** ["go.query.simple"]

The `docID` argument and `_docID` filter.

# `go.query.simple.doc-id.argument`: `docID` argument, single or list

- **Parents:** ["go.query.simple.doc-id"]
- **Go-Tests:** ["query/simple/with_doc_id_test.go::TestQuerySimpleWithDocIDFilter_TargetNotFound", "query/simple/with_doc_id_test.go::TestQuerySimpleWithDocIDFilter_SingleDocumentTargetFound", "query/simple/with_doc_id_test.go::TestQuerySimpleWithDocIDFilter_MultipleDocumentsTargetFound", "query/simple/with_doc_ids_test.go::TestQueryWithDocIDsFilter_SingleTargetNotFound", "query/simple/with_doc_ids_test.go::TestQueryWithDocIDsFilter_SingleTargetFound", "query/simple/with_doc_ids_test.go::TestQueryWithDocIDsFilter_DuplicateDocIDs", "query/simple/with_doc_ids_test.go::TestQuerySimpleWithDocIDsFilter_OneFoundFromMultipleTargets", "query/simple/with_doc_ids_test.go::TestQuerySimpleWithDocIDsFilter_AllFoundFromMultipleTargets", "query/simple/with_doc_ids_test.go::TestQuerySimpleReturnsNothinGivenEmptyDocIDsFilter"]

`Users(docID: "bae-...")` or `docID: [...]` returns only the listed documents that exist, once each even when an ID is repeated; an unknown DocID returns an empty list without error, and `docID: []` returns an empty list.

# `go.query.simple.doc-id.filter`: `_docID` in `filter`

- **Parents:** ["go.query.simple.doc-id"]
- **Go-Tests:** ["query/simple/with_doc_id_filter_test.go::TestQuerySimpleWithDocIDFilterBlock", "query/simple/with_doc_id_filter_test.go::TestQuerySimpleWithDocIDFilterInBlock"]

`filter: {_docID: {_eq: "<id>"}}` returns that document and `filter: {_docID: {_in: [...]}}` returns the listed ones.

# `go.query.simple.order`: The `order` argument

- **Parents:** ["go.query.simple"]

Sorting query results.

# `go.query.simple.order.single-field`: Ordering by one field

- **Parents:** ["go.query.simple.order"]
- **Go-Tests:** ["query/simple/with_order_test.go::TestQuerySimpleWithEmptyOrder", "query/simple/with_order_test.go::TestQuerySimpleWithNumericOrderAscending", "query/simple/with_order_test.go::TestQuerySimpleWithFloat32OrderAscending", "query/simple/with_order_test.go::TestQuerySimpleWithFloat64OrderAscending", "query/simple/with_order_test.go::TestQuerySimpleWithBlobOrderAscending", "query/simple/with_order_test.go::TestQuerySimpleWithDateTimeOrderAscending", "query/simple/with_order_test.go::TestQuerySimpleWithNumericOrderDescending", "query/simple/with_order_test.go::TestQuerySimpleWithFloat32OrderDescending", "query/simple/with_order_test.go::TestQuerySimpleWitFloat64OrderDescending", "query/simple/with_order_test.go::TestQuerySimpleWithBlobOrderDescending", "query/simple/with_order_test.go::TestQuerySimpleWithDateTimeOrderDescending", "query/simple/with_doc_id_filter_test.go::TestQuerySimpleOrdersByDocID", "query/simple/with_order_filter_test.go::TestQuerySimpleWithNumericGreaterThanFilterAndNumericOrderDescending"]

`order: {Age: ASC}` / `DESC` sorts by Int, Float32, Float64, Blob, DateTime or `_docID` in the requested direction; ordering combines with `filter`, and `order: {}` returns documents unsorted without error.

# `go.query.simple.order.multi-field`: Ordering by a list of fields

- **Parents:** ["go.query.simple.order"]
- **Go-Tests:** ["query/simple/with_order_test.go::TestQuerySimpleWithNumericOrderDescendingAndBooleanOrderAscending", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFieldsASCAndASC_ShouldOrderCorrectly", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFieldsACSAndDESC_ShouldOrderCorrectly", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFieldsDESCAndASC_ShouldOrderCorrectly", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFieldsDECSAndDESC_ShouldOrderCorrectly"]

`order: [{Name: ASC}, {Age: DESC}]` sorts by the first entry and breaks ties with the next, in every ASC/DESC combination.

# `go.query.simple.order.validation`: Invalid order arguments are rejected

- **Parents:** ["go.query.simple.order"]
- **Go-Tests:** ["query/simple/with_order_test.go::TestQuerySimple_WithInvalidOrderEnum_ReturnsError", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFields_ReturnsError", "query/simple/with_order_test.go::TestQuerySimple_WithMultipleOrderFieldsNestedWithinMultpleFields_ReturnsError"]

`order: {Age: INVALID}` fails with `Argument "order" has invalid value {Age: INVALID}`; an order object naming two fields (`{Age: ASC, Name: DESC}`, also as a list element) fails with `each order argument can only define one field`.

# `go.query.simple.order.alias`: Ordering by aliased fields with `_alias`

- **Parents:** ["go.query.simple.order"]
- **Go-Tests:** ["query/simple/with_order_test.go::TestQuerySimple_WithAliasOrder_ShouldOrderResults", "query/simple/with_order_test.go::TestQuerySimple_WithAliasOrderOnNonAliasedField_ShouldOrderResults", "query/simple/with_order_test.go::TestQuerySimple_WithAliasOrderOnNonExistantField_ShouldError", "query/simple/with_order_test.go::TestQuerySimple_WithInvalidAliasOrder_ShouldError", "query/simple/with_order_test.go::TestQuerySimple_WithEmptyAliasOrder_ShouldDoNothing", "query/simple/with_order_test.go::TestQuerySimple_WithNullAliasOrder_ShouldDoNothing", "query/simple/with_order_test.go::TestQuerySimple_WithIntAliasOrder_ShouldError", "query/simple/with_order_test.go::TestQuerySimple_WithCompoundAliasOrder_ShouldOrderResults"]

`order: {_alias: {UserAge: ASC}}` sorts by the selection aliased `UserAge` (or an unaliased field name), and a list of `_alias` entries sorts by each in turn. `_alias: {}` and `_alias: null` leave order unchanged; an unselected alias fails with `field or alias not found. Name: UserAge`, a bad direction with `invalid order direction`, and `_alias: 1` with `invalid order input`.

# `go.query.simple.order.aggregate-alias`: Ordering by an aliased per-document aggregate

- **Parents:** ["go.query.simple.order"]
- **Go-Tests:** ["query/simple/with_average_order_test.go::TestQuerySimpleWithAverageWithOrder_Succeeds", "query/simple/with_max_order_test.go::TestQuerySimpleWithMaxWithOrder_Succeeds", "query/simple/with_min_order_test.go::TestQuerySimpleWithMinWithOrder_Succeeds", "query/simple/with_sum_order_test.go::TestQuerySimpleWithSumWithOrder_Succeeds"]

Inside a document, `total: AVG(HeightM: {}, Age: {})` (and `MAX`, `MIN`, `SUM`) aggregates over the listed scalar fields of that document, and `order: {_alias: {total: DESC}}` / `ASC` sorts documents by that computed value.

# `go.query.simple.limit-offset`: `limit` and `offset`

- **Parents:** ["go.query.simple"]
- **Go-Tests:** ["query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit0", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit1", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit2", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimitBiggerThanTotalDocuments", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithOffset0", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithOffset1", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithOffset2", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithOffsetBiggerThanTotalDocuments", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit0AndOffset0", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit1AndOffset1", "query/simple/with_limit_offset_test.go::TestQuerySimpleWithLimit2AndOffset2"]

`Users(limit: n, offset: m)` skips the first `m` documents and returns at most `n`; `limit: 0` means no limit, `offset: 0` skips nothing, and a limit or offset beyond the document count returns the remaining documents (or none).

# `go.query.simple.aggregate`: Top-level aggregates over a collection

- **Parents:** ["go.query.simple"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX` as root query fields.

# `go.query.simple.aggregate.validation`: Aggregates without a target are rejected

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_average_test.go::TestQuerySimpleWithAverageOnUndefinedObject", "query/simple/with_count_test.go::TestQuerySimpleWithCountOnUndefined", "query/simple/with_sum_test.go::TestQuerySimpleWithSumOnUndefinedObject", "query/simple/with_min_test.go::TestQuerySimple_WithMinOnUndefinedObject_ReturnsError", "query/simple/with_max_test.go::TestQuerySimple_WithMaxOnUndefinedObject_ReturnsError", "query/simple/with_average_test.go::TestQuerySimpleWithAverageOnUndefinedField", "query/simple/with_sum_test.go::TestQuerySimpleWithSumOnUndefinedField", "query/simple/with_min_test.go::TestQuerySimple_WithMinOnUndefinedField_ReturnsError", "query/simple/with_max_test.go::TestQuerySimple_WithMaxOnUndefinedField_ReturnsError"]

A bare `query { AVG }` (likewise `COUNT`, `SUM`, `MIN`, `MAX`) fails with `aggregate must be provided with a property to aggregate`; `AVG(Users: {})` (likewise `SUM`, `MIN`, `MAX`) without `field` fails with `Argument "Users" has invalid value {}.\nIn field "field": Expected "UsersNumericFieldsArg!", found null.`.

# `go.query.simple.aggregate.count`: `COUNT`

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_count_test.go::TestQuerySimpleWithCountOnEmptyCollection", "query/simple/with_count_test.go::TestQuerySimpleWithCount", "query/simple/with_count_test.go::TestQuerySimple_WithAliasedCount_OnEmptyCollection_Succeeds", "query/simple/with_count_filter_test.go::TestQuerySimpleWithCountWithFilter", "query/simple/with_count_filter_test.go::TestQuerySimpleWithCountWithDateTimeFilter"]

`COUNT(Users: {})` returns the number of documents (0 on an empty collection, also under an alias such as `number:`), and `COUNT(Users: {filter: {...}})` counts only documents matching the filter, DateTime comparisons included.

# `go.query.simple.aggregate.count-group-by`: `COUNT` with `groupBy` counts groups

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_count_groupby_test.go::TestQuerySimple_WithCountWithGroupBy_OnEmptyCollection", "query/simple/with_count_groupby_test.go::TestQuerySimple_WithCountWithGroupBy_OnSingleField", "query/simple/with_count_groupby_test.go::TestQuerySimple_WithCountWithGroupBy_OnMultipleFields", "query/simple/with_count_groupby_test.go::TestQuerySimple_WithCountWithGroupBy_AllSameGroupValue", "query/simple/with_count_groupby_test.go::TestQuerySimple_WithCountWithGroupByAndFilter"]

`COUNT(Users: {groupBy: [Age]})` returns the number of distinct groups (distinct value tuples for `[Age, Name]`), 0 on an empty collection, 1 when all documents share a value, and applies `filter` before grouping.

# `go.query.simple.aggregate.sum`: `SUM`

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_sum_test.go::TestQuerySimpleWithSumOnEmptyCollection", "query/simple/with_sum_test.go::TestQuerySimpleWithSum", "query/simple/with_sum_filter_test.go::TestQuerySimpleWithSumWithFilter"]

`SUM(Users: {field: Age})` returns the total of the field over all documents (0 on an empty collection), restricted to filter matches when `filter` is given.

# `go.query.simple.aggregate.average`: `AVG`

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_average_test.go::TestQuerySimpleWithAverageOnEmptyCollection", "query/simple/with_average_test.go::TestQuerySimpleWithAverage", "query/simple/with_average_test.go::TestQuerySimple_WithAliasedAverage_OnEmptyCollection_Succeeds", "query/simple/with_average_filter_test.go::TestQuerySimpleWithAverageWithFilter", "query/simple/with_average_filter_test.go::TestQuerySimpleWithAverageWithDateTimeFilter"]

`AVG(Users: {field: Age})` returns the mean as a Float (`0` on an empty collection, also under an alias), restricted to filter matches when `filter` is given, DateTime filters included.

# `go.query.simple.aggregate.min-max`: `MIN` and `MAX`

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_max_test.go::TestQuerySimple_WithMaxOnEmptyCollection_Succeeds", "query/simple/with_max_test.go::TestQuerySimple_WithMax_Succeeds", "query/simple/with_max_test.go::TestQuerySimple_WithAliasedMaxOnEmptyCollection_Succeeds", "query/simple/with_max_filter_test.go::TestQuerySimple_WithMaxFilter_Succeeds", "query/simple/with_min_test.go::TestQuerySimple_WithMinOnEmptyCollection_Succeeds", "query/simple/with_min_test.go::TestQuerySimple_WithMin_Succeeds", "query/simple/with_min_test.go::TestQuerySimple_WithAliasedMinOnEmptyCollection_Succeeds", "query/simple/with_min_filter_test.go::TestQuerySimple_WithMinFilter_Succeeds"]

`MAX(Users: {field: Age})` / `MIN(...)` return the largest / smallest value of the field (Int stays Int), restricted to filter matches when `filter` is given; on an empty collection they return `null`, also under an alias.

# `go.query.simple.aggregate.int64-extreme`: `MAX` of a 64-bit Int

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_max_test.go::TestQuerySimple_WithMaxAndMaxValueInt_Succeeds", "query/simple/with_min_test.go::TestQuerySimple_WithMinAndMaxValueInt_Succeeds"]
- **Scope:** "client types: go, cli, http (not JS); mutation types: collection save and collection named (not GraphQL), because the document is created with Age = math.MaxInt64"
- **Go-Bug-Suspect:** "'TestQuerySimple_WithMinAndMaxValueInt_Succeeds' queries 'MAX', not 'MIN', so 'MIN' at MaxInt64 is untested"

With a document whose `Age` is `9223372036854775807`, `MAX(Users: {field: Age})` returns exactly `9223372036854775807` with no float precision loss.

# `go.query.simple.aggregate.non-nillable`: Aggregates over non-nullable fields

- **Parents:** ["go.query.simple.aggregate"]
- **Go-Tests:** ["query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithSumOnNonNillableInt", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithAverageOnNonNillableInt", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMaxOnNonNillableInt", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMinOnNonNillableInt", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithCountOnNonNillableInt", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithSumOnNonNillableFloat64", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithAverageOnNonNillableFloat64", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMaxOnNonNillableFloat64", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMinOnNonNillableFloat64", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithCountOnNonNillableFloat64", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithSumOnNonNillableFloat32", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithAverageOnNonNillableFloat32", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMaxOnNonNillableFloat32", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithMinOnNonNillableFloat32", "query/simple/with_aggregate_nonnillable_test.go::TestQuerySimpleWithCountOnNonNillableFloat32"]
- **Go-Bug-Suspect:** "on 'score: Int!' the tests expect 'MAX'/'MIN' as float64 (30, 10) and 'SUM' as int64, while 'MAX' on nullable 'Age: Int' expects int64; possibly a type quirk or only harness numeric normalisation"

`SUM`, `AVG`, `MIN`, `MAX` and `COUNT` work on `Int!`, `Float64!` and `Float32!` fields (e.g. scores 10, 20, 30 give SUM 60, AVG 20, MIN 10, MAX 30, COUNT 3).

# `go.query.simple.group`: `groupBy`

- **Parents:** ["go.query.simple"]

Grouping documents and the `GROUP` sub-selection.

# `go.query.simple.group.basic`: Grouping by one or more fields

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_test.go::TestQuerySimpleWithGroupByEmpty", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByNumber", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByDateTime", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByNumberWithGroupString", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByWithoutGroupedFieldSelectedWithInnerGroup", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByString", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBoolean", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByStringThenBoolean", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByBooleanThenNumber"]

`Users(groupBy: [Age]) { Age GROUP { Name } }` returns one row per distinct value (Int, String, Boolean, DateTime) with the grouped documents under `GROUP`; several fields group by value tuple, `GROUP(groupBy: [...])` nests a further grouping, and `groupBy: []` puts every document in one group.

# `go.query.simple.group.unselected-key`: Grouping without selecting the grouped field

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_unselected_field_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupField", "query/simple/with_group_unselected_field_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupField"]

`Users(groupBy: [Age]) { GROUP { Name } }` (also for a String key such as `Email`) still returns one row per distinct key, each holding only its `GROUP` documents.

# `go.query.simple.group.undefined-value`: Documents with the grouped field unset

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_test.go::TestQuerySimpleWithGroupByNumberOnUndefined", "query/simple/with_group_test.go::TestQuerySimpleWithGroupByNumberOnUndefinedWithChildren"]

Documents whose grouped field is unset form one group with key `null` (e.g. `{"Age": null}`), alongside the groups of set values.

# `go.query.simple.group.validation`: Invalid grouped selections are rejected

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_test.go::TestQuerySimpleErrorsWithNonGroupFieldsSelected", "query/simple/with_group_count_test.go::TestQuerySimpleWithoutGroupByWithCountOnGroup", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithCountOnInnerNonExistantGroup", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithUndefinedField", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndAverageOfUndefined", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndSumOfUndefined", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndMinOfUndefined_ReturnsError", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndMaxOfUndefined_ReturnsError"]

Selecting a non-grouped field at group level fails with `cannot select a non-group-by field at group-level`; `COUNT(GROUP: {})` outside a `groupBy` level (also inside `GROUP` with no inner groupBy) fails with `group may only be referenced when within a groupBy request`; a bare `COUNT`, `AVG`, `SUM`, `MIN` or `MAX` at group level fails with `aggregate must be provided with a property to aggregate`.

# `go.query.simple.group.filter`: Filtering groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_filter_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberFilter", "query/simple/with_group_filter_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberWithParentFilter", "query/simple/with_group_filter_test.go::TestQuerySimpleWithGroupByStringWithUnrenderedGroupNumberWithParentFilter", "query/simple/with_group_filter_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanThenInnerNumberFilterThatExcludesAll", "query/simple/with_group_filter_test.go::TestQuerySimpleWithGroupByStringWithMultipleGroupNumberFilter"]

A root `filter` applies before grouping, so groups with no matching documents disappear; `GROUP(filter: {...})` filters only the documents inside each group (groups stay, possibly with an empty `GROUP`), independently for each aliased `GROUP` and at nested levels.

# `go.query.simple.group.order`: Ordering groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberWithGroupOrder", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithGroupNumberWithGroupOrderDescending", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringAndOrderDescendingWithGroupNumberWithGroupOrder", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanThenInnerOrderDescending", "query/simple/with_group_order_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndOrderAscendingThenInnerOrderDescending"]

Root `order` on a grouped field sorts the group rows; `GROUP(order: {Age: ASC|DESC})` sorts documents within each group, at nested levels too, independently of the parent order.

# `go.query.simple.group.limit-offset`: Paging groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_limit_test.go::TestQuerySimpleWithGroupByNumberWithGroupLimit", "query/simple/with_group_limit_test.go::TestQuerySimpleWithGroupByNumberWithMultipleGroupsWithDifferentLimits", "query/simple/with_group_limit_test.go::TestQuerySimpleWithGroupByNumberWithLimitAndGroupWithHigherLimit", "query/simple/with_group_limit_test.go::TestQuerySimpleWithGroupByNumberWithGroupLimitAndRootOrder", "query/simple/with_group_limit_test.go::TestQuerySimpleWithGroupByNumberWithLimitAndGroupWithLowerLimit", "query/simple/with_group_limit_offset_test.go::TestQuerySimpleWithGroupByNumberWithGroupLimitAndOffset", "query/simple/with_group_limit_offset_test.go::TestQuerySimpleWithGroupByNumberWithLimitAndOffsetAndWithGroupLimitAndOffset"]

Root `limit`/`offset` page the group rows; `GROUP(limit: n, offset: m)` pages documents within each group, independently per aliased `GROUP` and combined with root `order`.

# `go.query.simple.group.doc-id`: `docID` on `GROUP`

- **Parents:** ["go.query.simple.group"]
- **Go-Tests:** ["query/simple/with_group_doc_id_test.go::TestQuerySimpleWithGroupByWithGroupWithDocID", "query/simple/with_group_doc_ids_test.go::TestQuerySimpleWithGroupByWithGroupWithDocIDs"]

`GROUP(docID: "<id>")` or `GROUP(docID: [...])` keeps only those documents inside each group; every group row is still returned, with an empty `GROUP` where none match.

# `go.query.simple.group.aggregate`: Aggregates over `GROUP`

- **Parents:** ["go.query.simple.group"]

`COUNT(GROUP: ...)`, `SUM`, `AVG`, `MIN`, `MAX` computed per group.

# `go.query.simple.group.aggregate.count`: `COUNT(GROUP: {})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCount", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCountOnEmptyCollection", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupAndChildCount", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndAliasesChildCount", "query/simple/with_group_count_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndDuplicatedAliasedChildCounts"]

`COUNT(GROUP: {})` returns each group's document count, with or without `GROUP` also selected and under one or several aliases; an empty collection returns no rows.

# `go.query.simple.group.aggregate.sum`: `SUM(GROUP: {field})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerSumOnEmptyCollection", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerSum", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildNilSum", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildEmptyFloatSum", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildFloatSum"]

`SUM(GROUP: {field: Age})` returns each group's total over Int (negative values included) or Float fields, ignoring documents where the field is unset; an empty collection returns no rows.

# `go.query.simple.group.aggregate.average`: `AVG(GROUP: {field})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverageOnEmptyCollection", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverage", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildNilAverage", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildEmptyFloatAverage", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildFloatAverage"]

`AVG(GROUP: {field: Age})` returns each group's mean over Int (negative values included) or Float fields, ignoring documents where the field is unset; a group with no set values averages to 0, and an empty collection returns no rows.

# `go.query.simple.group.aggregate.min-max`: `MIN` and `MAX` over `GROUP`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMaxOnEmptyCollection_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMax_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildNilMax_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildEmptyFloatMax_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildFloatMax_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMinOnEmptyCollection_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMin_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildNilMin_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildEmptyFloatMin_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildFloatMin_Succeeds"]

`MAX(GROUP: {field: Age})` / `MIN(...)` return each group's largest / smallest value over Int (negative values included) or Float fields, ignoring unset values; a group with no set values returns `null`, and an empty collection returns no rows.

# `go.query.simple.group.aggregate.multiple`: Several different aggregates on one group

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_count_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverageAndCount", "query/simple/with_group_average_max_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerAverageAndMax_Succeeds", "query/simple/with_group_average_min_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerAverageAndMin_Succeeds", "query/simple/with_group_average_sum_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverageAndSum"]

`AVG(GROUP: {field: Age})` selected together with `COUNT(GROUP: {})`, `MAX`, `MIN` or `SUM` on the same group returns each value correctly, negative values included.

# `go.query.simple.group.aggregate.of-same`: Aggregate of the same aggregate over inner groups

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndAverageOfAverageOfInt", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndAverageOfAverageOfFloat", "query/simple/with_group_average_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndAverageOfAverageOfAverageOfFloat", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndSumOfSumOfInt", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndSumOfSumOfFloat", "query/simple/with_group_sum_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndSumOfSumOfSumOfFloat", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMinOfMinOfInt_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMinOfMinOfFloat_Succeeds", "query/simple/with_group_min_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMinOfMinOfMinOfFloat_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMaxOfMaxOfInt_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMaxOfMaxOfFloat_Succeeds", "query/simple/with_group_max_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMaxOfMaxOfMaxOfFloat_Succeeds"]

With `GROUP(groupBy: [Verified]) { AVG(GROUP: {field: Age}) }`, an outer `AVG(GROUP: {field: AVG})` aggregates the inner groups' values; the same holds for `SUM` of `SUM`, `MIN` of `MIN` and `MAX` of `MAX`, over Int and Float and across three nesting levels.

# `go.query.simple.group.aggregate.of-other`: Aggregate of a different aggregate over inner groups

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_max_test.go::TestQuery_SimpleWithGroupByStringWithInnerGroupBooleanAndMaxOfAverageOfInt_Succeeds", "query/simple/with_group_average_min_test.go::TestQuery_SimpleWithGroupByStringWithInnerGroupBooleanAndMinOfAverageOfInt_Succeeds", "query/simple/with_group_average_sum_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndSumOfCountOfInt", "query/simple/with_group_count_max_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMaxOfCount_Succeeds", "query/simple/with_group_count_min_test.go::TestQuerySimple_WithGroupByStringWithInnerGroupBooleanAndMinOfCount_Succeeds", "query/simple/with_group_count_sum_test.go::TestQuerySimpleWithGroupByStringWithInnerGroupBooleanAndSumOfCount"]

`MAX`, `MIN` and `SUM(GROUP: {field: AVG})` aggregate the inner groups' `AVG` values, and `MAX`, `MIN` and `SUM(GROUP: {field: COUNT})` aggregate the inner groups' `COUNT` values.

# `go.query.simple.group.aggregate.filtered-average`: `AVG(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildAverageWithFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithRenderedGroupAndChildAverageWithFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithRenderedGroupAndChildAverageWithDateTimeFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithRenderedGroupWithFilterAndChildAverageWithMatchingFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithRenderedGroupWithFilterAndChildAverageWithMatchingDateTimeFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithRenderedGroupWithFilterAndChildAverageWithDifferentFilter", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildAveragesWithDifferentFilters", "query/simple/with_group_average_filter_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildAverageWithFilterAndNilItem"]

`AVG(GROUP: {field: Age, filter: {...}})` averages only the group's documents matching the filter (Int and DateTime conditions), unaffected by a different `GROUP(filter: ...)` selected alongside or by other aliased averages with different filters, and ignoring documents where the field is unset.

# `go.query.simple.group.aggregate.filtered-count`: `COUNT(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_count_filter_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCountWithFilter", "query/simple/with_group_count_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupAndChildCountWithFilter", "query/simple/with_group_count_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithFilterAndChildCountWithMatchingFilter", "query/simple/with_group_count_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithFilterAndChildCountWithDifferentFilter", "query/simple/with_group_count_filter_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCountsWithDifferentFilters"]

`COUNT(GROUP: {filter: {...}})` counts only the group's documents matching the filter, independently of any `GROUP(filter: ...)` selected alongside and of other aliased counts with different filters.

# `go.query.simple.group.aggregate.filtered-sum`: `SUM(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_sum_filter_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildSumWithFilter", "query/simple/with_group_sum_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupAndChildSumWithFilter", "query/simple/with_group_sum_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithFilterAndChildSumWithMatchingFilter", "query/simple/with_group_sum_filter_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithFilterAndChildSumWithDifferentFilter", "query/simple/with_group_sum_filter_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildSumsWithDifferentFilters"]

`SUM(GROUP: {field: Age, filter: {...}})` sums only the group's documents matching the filter (0 when none match), independently of any `GROUP(filter: ...)` and of other aliased sums with different filters.

# `go.query.simple.group.aggregate.filtered-min-max`: `MIN`/`MAX(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_max_filter_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupAndChildMaxWithFilter_Succeeds", "query/simple/with_group_max_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupAndChildMaxWithFilter_Succeeds", "query/simple/with_group_max_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupWithFilterAndChildMaxWithMatchingFilter_Succeeds", "query/simple/with_group_max_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupWithFilterAndChildMaxWithDifferentFilter_Succeeds", "query/simple/with_group_max_filter_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupAndChildMaxWithDifferentFilters_Succeeds", "query/simple/with_group_min_filter_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupAndChildMinWithFilter_Succeeds", "query/simple/with_group_min_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupAndChildMinWithFilter_Succeeds", "query/simple/with_group_min_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupWithFilterAndChildMinWithMatchingFilter_Succeeds", "query/simple/with_group_min_filter_test.go::TestQuerySimple_WithGroupByNumberWithRenderedGroupWithFilterAndChildMinWithDifferentFilter_Succeeds", "query/simple/with_group_min_filter_test.go::TestQuerySimple_WithGroupByNumberWithoutRenderedGroupAndChildMinWithDifferentFilters_Succeeds"]

`MAX`/`MIN(GROUP: {field: Age, filter: {...}})` consider only the group's documents matching the filter (`null` when none match), independently of any `GROUP(filter: ...)` and of other aliased aggregates with different filters.

# `go.query.simple.group.aggregate.limit-offset`: `limit`/`offset` inside a group aggregate

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_average_limit_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverageWithLimit", "query/simple/with_group_average_limit_offset_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerAverageWithLimitAndOffset", "query/simple/with_group_count_limit_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCountWithLimit", "query/simple/with_group_count_limit_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithLimitAndChildCountWithLimit", "query/simple/with_group_count_limit_offset_test.go::TestQuerySimpleWithGroupByNumberWithoutRenderedGroupAndChildCountWithLimitAndOffset", "query/simple/with_group_count_limit_offset_test.go::TestQuerySimpleWithGroupByNumberWithRenderedGroupWithLimitAndChildCountWithLimitAndOffset", "query/simple/with_group_max_limit_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMaxWithLimit_Succeeds", "query/simple/with_group_max_limit_offset_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMaxWithLimitAndOffset_Succeeds", "query/simple/with_group_min_limit_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMinWithLimit_Succeeds", "query/simple/with_group_min_limit_offset_test.go::TestQuerySimple_WithGroupByStringWithoutRenderedGroupAndChildIntegerMinWithLimitAndOffset_Succeeds", "query/simple/with_group_sum_limit_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerSumWithLimit", "query/simple/with_group_sum_limit_offset_test.go::TestQuerySimpleWithGroupByStringWithoutRenderedGroupAndChildIntegerSumWithLimitAndOffset"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX(GROUP: {..., offset: m, limit: n})` aggregate only the window of the group's documents after skipping `m` and taking `n`, independently of a `GROUP(limit: ...)` selected alongside.

# `go.query.simple.group.aggregate.alias-filter`: Filtering group rows by an aliased aggregate

- **Parents:** ["go.query.simple.group.aggregate"]
- **Go-Tests:** ["query/simple/with_group_aggregate_alias_filter_test.go::TestQuerySimple_WithGroupAverageAliasFilter_FiltersResults", "query/simple/with_group_aggregate_alias_filter_test.go::TestQuerySimple_WithGroupSumAliasFilter_FiltersResults", "query/simple/with_group_aggregate_alias_filter_test.go::TestQuerySimple_WithGroupMinAliasFilter_FiltersResults", "query/simple/with_group_aggregate_alias_filter_test.go::TestQuerySimple_WithGroupMaxAliasFilter_FiltersResults", "query/simple/with_group_aggregate_alias_filter_test.go::TestQuerySimple_WithGroupCountAliasFilter_FiltersResults"]

`Users(groupBy: [Name], filter: {_alias: {totalScore: {_eq: 40}}}) { Name totalScore: SUM(GROUP: {field: Score}) }` returns only the groups whose aliased aggregate matches; likewise for `AVG`, `MIN`, `MAX` and `COUNT`.

# `go.query.simple.cid`: Querying a past version with `cid`

- **Parents:** ["go.query.simple"]

The `cid` argument selects document state at a commit.

# `go.query.simple.cid.doc-version`: `cid` returns the document state at that commit

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_cid_test.go::TestQuerySimpleWithCid", "query/simple/with_cid_test.go::TestQuerySimpleWithCid_MultipleDocs", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithCidAndDocID", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithUpdateAndFirstCidAndDocID", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithUpdateAndLastCidAndDocID", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithUpdateAndMiddleCidAndDocID", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithUpdateAndFirstCidAndDocIDAndSchemaVersion", "query/simple/with_cid_shared_field_test.go::TestQuerySimple_WithSharedFieldCid_ReturnsAllOwners"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers (CIDs are templated/hardcoded)"

`Users(cid: "<composite CID>")` (with or without a matching `docID`) returns only that document, with its field values and `_version` as of that commit, even after later updates and when other documents exist. A field-level CID shared by several documents with identical field content returns every owning document.

# `go.query.simple.cid.counter`: `cid` on documents with counter fields

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_cid_doc_id_test.go::TestCidAndDocIDQuery_ContainsPNCounterWithIntKind_NoError", "query/simple/with_cid_doc_id_test.go::TestCidAndDocIDQuery_ContainsPNCounterWithFloatKind_NoError", "query/simple/with_cid_doc_id_test.go::TestCidAndDocIDQuery_ContainsPCounterWithIntKind_NoError", "query/simple/with_cid_doc_id_test.go::TestCidAndDocIDQuery_ContainsPCounterWithFloatKind_NoError", "query/simple/with_cid_test.go::TestQuerySimple_WithCIDAndCounterAfterUpdate_ShouldSucceed"]
- **Scope:** "excluded from the secondary-index multiplier (counter fields cannot be indexed, Go #4439) and the encrypted-docs and signed-docs multipliers"

For a `@crdt(type: pncounter)` or `pcounter` field of Int or Float kind, querying with a CID and `docID` returns the counter value accumulated as of that commit (e.g. the first CID returns the initial value, a later CID the incremented value), without error.

# `go.query.simple.cid.list`: `cid` given as a list

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_cid_test.go::TestQuerySimple_ListOfOneCID", "query/simple/with_cid_test.go::TestQuerySimple_MultipleCIDs", "query/simple/with_cid_test.go::TestQuerySimple_DuplicateCIDsForSameDoc", "query/simple/with_cid_test.go::TestQuerySimple_MultipleCIDsForSameDoc"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers"

`Users(cid: ["<a>", "<b>"])` returns one result per distinct CID: each document at its version, two versions of the same document as two results, and a repeated CID only once.

# `go.query.simple.cid.doc-id-match`: `cid` combined with a `docID` list

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_version_cid_test.go::TestQuerySimpleWithVersionAndCid", "query/simple/with_version_cid_doc_ids_test.go::TestQuerySimpleWithVersionAndCidAndCorrectDocID", "query/simple/with_version_cid_doc_ids_test.go::TestQuerySimpleWithVersionAndCidAndCorrectAndIncorrectDocID", "query/simple/with_version_cid_doc_ids_test.go::TestQuerySimpleWithVersionAndCidAndIncorrectDocID"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers"

`Users(cid: "<c>", docID: [...])` returns the document at that commit with its `_version` when the CID's document is among the listed DocIDs (other listed IDs are ignored), and an empty list when it is not.

# `go.query.simple.cid.branchable`: Collection-level CIDs of a `@branchable` collection

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_cid_branchable_test.go::TestQuerySimpleWithCidOfBranchableCollection_FirstCid", "query/simple/with_cid_branchable_test.go::TestQuerySimpleWithCidOfBranchableCollection_MiddleCid", "query/simple/with_cid_branchable_test.go::TestQuerySimpleWithCidOfBranchableCollection_LastCid", "query/simple/with_cid_doc_id_branchable_test.go::TestQuerySimpleWithCidOfBranchableCollectionAndDocID"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers (hardcoded collection CIDs, Go #4744)"

On a `@branchable` collection, `Users(cid: "<collection commit CID>")` returns every document as of that collection commit (after create `Fred` → `Fred`; after update → `Freddddd`; after a second create → `John` and `Freddddd`); adding `docID` restricts the result to that document at that commit.

# `go.query.simple.cid.errors`: Invalid, unknown and foreign CIDs

- **Parents:** ["go.query.simple.cid"]
- **Go-Tests:** ["query/simple/with_cid_test.go::TestQuerySimpleWithInvalidCid", "query/simple/with_cid_test.go::TestQuerySimple_UnknownCid", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithInvalidCidAndInvalidDocID", "query/simple/with_cid_doc_id_test.go::TestQuerySimpleWithUnknownCidAndInvalidDocID", "query/simple/with_cid_test.go::TestQuerySimple_WithCidFromAnotherCollection_ReturnsEmpty"]
- **Go-Bug-Suspect:** "an unknown but well-formed CID errors with 'failed to get block in blockstore: ipld: could not find'; the test comment says this is documented only and the desired behaviour is an empty result"

A string that is not a CID fails with `invalid cid: selected encoding not supported`; a well-formed CID with no block fails with `failed to get block in blockstore: ipld: could not find` (with or without a `docID`); a CID belonging to another collection's document returns an empty list without panicking.

# `go.query.simple.version`: The embedded `_version` commit field

- **Parents:** ["go.query.simple"]

Commit metadata selected per document.

# `go.query.simple.version.latest-commit`: `_version` returns the latest composite commit

- **Parents:** ["go.query.simple.version"]
- **Go-Tests:** ["query/simple/with_version_test.go::TestQuerySimpleWithNestedLatestCommit", "query/simple/with_version_test.go::TestQuery_AddDocWithNestedLatestCommit", "query/simple/with_version_test.go::TestQuery_UpdateDocWithNestedLatestCommit", "query/simple/with_version_test.go::TestQuerySimpleWithEmbeddedLatestCommitWithCollectionVersionID", "query/simple/with_version_test.go::TestQuerySimpleWithEmbeddedLatestCommitWithDocID", "query/simple/with_version_test.go::TestQuery_WithAllCommitFields_NoError", "query/simple/with_version_test.go::TestQuery_WithAllCommitFieldsWithUpdate_NoError", "query/simple/with_version_order_test.go::TestQuerySimpleWithVersionAndOrder"]
- **Scope:** "the tests asserting CIDs and link order are excluded from the encrypted-docs and signed-docs multipliers"

`Users { _version { cid delta docID fieldName height links { cid fieldName } heads { cid } collectionVersionId } }` returns the document's composite commits, newest first, each with its CID, delta, DocID, `fieldName: "_C"`, height and collection version ID. A new document has one entry (height 1) that links every field commit, sorted by child CID, with empty `heads`. After one update there are two: the update commit (height 2) links only the changed field's commit and lists the create commit under `heads`, followed by the create commit. It combines with `order`.

# `go.query.simple.version.aliased`: Aliased and filtered `links`/`heads`

- **Parents:** ["go.query.simple.version"]
- **Go-Tests:** ["query/simple/with_version_test.go::TestQuerySimpleWithMultipleAliasedEmbeddedLatestCommit", "query/simple/with_version_test.go::TestQuerySimpleWithMultipleAliasedInterleavedNestedLatestCommit", "query/simple/with_version_test.go::TestQuery_WithMultipleAliasedFilteredEmbeddedLatestCommit"]
- **Scope:** "the tests asserting link order are excluded from the encrypted-docs and signed-docs multipliers"

Within `_version`, `links` and `heads` can be selected several times under different aliases with different sub-fields, interleaved, and `links(filter: {fieldName: {_eq: "Age"}})` returns only the matching links.

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

# `go.query.simple.similarity.full-scan`: `SIMILARITY` values without an index

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityOnEmptyCollection_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithIntSimilarity_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithFloat32Similarity_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithFloat64Similarity_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithJSONDocCreationSimilarity_ShouldSucceed"]

`User { name SIMILARITY(pointsList: {vector: [1, 2, 0]}) }` returns, per document, the cosine similarity between the `[Int!]`, `[Float32!]` or `[Float64!]` field and the vector, also for documents created from JSON; an empty collection returns an empty list.

# `go.query.simple.similarity.alias`: Filtering and ordering by an aliased `SIMILARITY`

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityAndFilteringOnSimilarityResult_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithSimilarityAndOrderingWithLimitOnSimilarityResult_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithTwoSimilarityAndFilteringOnSecond_ShouldSucceed", "query/simple/with_similarity_test.go::TestQuerySimple_WithTwoSimilarityAndFilteringOnBoth_ShouldSucceed"]
- **Go-Bug-Suspect:** "'filter: {_or: [{_alias: {sim2: {_gt: 20}}}, {_alias: {sim: {_lt: 10}}}]}' is asserted to return no documents although every cosine similarity is below 10; Go #3468 documents this as a bug"

`filter: {_alias: {sim: {_lt: 0.9}}}` and `order: {_alias: {sim: DESC}}, limit: 2` filter and sort by an aliased `SIMILARITY`; with two similarity selections, filtering on the second works independently of the first.

# `go.query.simple.similarity.vector-index`: k-nearest search through a vector index

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_ReturnsKNearest", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_ReflectsUpdatedVector", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_ExcludesDeletedDoc", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_IsMagnitudeInvariant", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_RespectsOffset"]

On a field with `@index(vector: {dimensions: 3, hnsw: {metric: COSINE}})`, `User(order: {_alias: {sim: DESC}}, limit: k, offset: m)` with `sim: SIMILARITY(vector: {...})` returns the k nearest documents after skipping m, reading only those through the index (`@explain(type: execute)` shows 1 index fetch and k doc fetches). Updated vectors are re-ranked, deleted documents are excluded, and similarity ignores magnitude (`[10,0,0]` and `[1,0,0]` both score 1).

# `go.query.simple.similarity.vector-index-fallback`: Similarity queries the vector index cannot serve

- **Parents:** ["go.query.simple.similarity"]
- **Go-Tests:** ["query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_AscendingOrderFullScans", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_NoOrderDoesNotUseIndex", "query/simple/with_similarity_vector_index_test.go::TestQuerySimple_WithSimilarityOnVectorIndex_WrongLengthQueryErrors"]

On a vector-indexed field, a similarity query not ordered by similarity `DESC` does not use the index (no index fetch under explain) and returns correct results with a `WarningCodeVectorIndexUnused` warning. Ordering by similarity `ASC` scans the whole collection; with no order, a `limit` may stop the scan early, so the query need not read every document; a query vector whose length differs from the index dimensions fails with `source and vector must be of the same length`.
