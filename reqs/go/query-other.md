# `go.query.commits`: Commit history queries

- **Parents:** ["go.query"]

The `_commits` GraphQL query exposes the Merkle-CRDT block history (field, composite `_C` and collection-level blocks) of documents.

# `go.query.commits.branchable`: Branchable collection commits

- **Parents:** ["go.query.commits"]

Collections declared `@branchable` get collection-level commits in addition to document field and composite commits.

# `go.query.commits.branchable.sync`: Branchable commits replicate over P2P

- **Parents:** ["go.query.commits.branchable"]
- **Go-Tests:** ["query/commits/branchables/peer_test.go::TestQueryCommitsBranchables_SyncsAcrossPeerConnection", "query/commits/branchables/peer_test.go::TestQueryCommitsBranchables_SyncsMultipleAcrossPeerConnection", "query/commits/branchables/peer_update_test.go::TestQueryCommitsBranchables_HandlesConcurrentUpdatesAcrossPeerConnection", "query/commits/branchables/peer_index_test.go::TestQueryCommitsBranchables_SyncsIndexAcrossPeerConnection"]
- **Scope:** "two-node P2P tests (connected peers with collection subscription); TestQueryCommitsBranchables_HandlesConcurrentUpdatesAcrossPeerConnection excludes the signed-docs and encrypted-docs multipliers"

When a node subscribes to a branchable collection, docs created on the peer arrive with their full commit graph, including collection-level commits, so `_commits` returns an identical graph on both nodes (two creates: 8 commits, 4 leaves, 2 composites, a collection root and one collection head). Concurrent offline updates on both nodes converge after sync to the same `_commits` graph and the same latest value (`Users { name }` returns "Addo"), and a synced doc is found through its secondary index (`Users(filter: {name: {_eq: "John"}})`).

# `go.query.commits.links`: Commit links and heads

- **Parents:** ["go.query.commits"]

Commits expose `links` (child blocks) and `heads` (previous versions of the same block).

# `go.query.commits.cid`: Commits by CID

- **Parents:** ["go.query.commits"]

`_commits(cid: ...)` returns specific blocks.

# `go.query.commits.cid.lookup`: Fetching commits by CID

- **Parents:** ["go.query.commits.cid"]
- **Go-Tests:** ["query/commits/with_cid_test.go::TestQueryCommits_WithFirstCommitCid_ShouldSucceed", "query/commits/with_cid_test.go::TestQueryCommits_WithFirstCommitCidForFieldCommit_ShouldSucceed", "query/commits/with_cid_test.go::TestQueryCommits_ListOfOne", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsSameDoc", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsDifferentDocs", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsDifferentDocs_NoAccessToSecondCid", "query/commits/with_cid_shared_field_test.go::TestQueryCommits_WithSharedFieldCidAndDocID", "query/commits/with_cid_shared_field_test.go::TestQueryCommits_WithSharedFieldCidWithoutDocID_ReturnsAllOwners", "query/commits/with_doc_id_cid_test.go::TestQueryCommits_WithDocIDAndCidWithUpdate", "query/commits/with_doc_id_cid_test.go::TestQueryCommitsWithDocIDAndCidWithUpdateAndDepth"]
- **Scope:** "all tests exclude the signed-docs and encrypted-docs multipliers (CIDs are hardcoded); TestQueryCommits_MultipleCidsDifferentDocs_NoAccessToSecondCid uses a DAC policy on the collection"

`_commits(cid: C)` (or `cid: [C]`) returns exactly that commit, for a field or composite block, even after later updates; `cid: [C1, C2]` returns both, across one or several docs, but omits a CID belonging to a doc the requesting identity cannot read under document ACP. A field CID shared by two docs returns one commit per owning doc when queried alone and only the named doc's commit when combined with `docID`. `docID` + `cid` + `depth: 5` returns the commit and its ancestors.

# `go.query.commits.filter`: Filtering commits

- **Parents:** ["go.query.commits"]

`_commits(filter: ...)` filters on commit fields, mainly `fieldName`.

# `go.query.commits.filter.fieldname-cid`: fieldName filter combined with a CID

- **Parents:** ["go.query.commits.filter"]
- **Go-Tests:** ["query/commits/with_field_cid_test.go::TestQueryCommitsWithFieldAndCID", "query/commits/with_field_cid_test.go::TestQueryCommits_WithWrongFieldAndCID_ReturnEmptyList", "query/commits/with_field_cid_test.go::TestQueryCommits_WithInvalidFieldAndCID_ReturnEmptyList"]
- **Scope:** "TestQueryCommitsWithFieldAndCID excludes the signed-docs and encrypted-docs multipliers; the other two exclude the signed-docs multiplier"

`_commits(filter: {fieldName: {_eq: "age"}}, cid: <age field cid>)` returns that commit; the same CID with `fieldName` `"name"` or `"NOT_A_FIELD"` returns an empty list.

# `go.query.commits.group`: Grouping commits

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_group_test.go::TestQueryCommitsWithGroupBy", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByHeightWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByCidWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByDocID", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByFieldName", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByFieldNameWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByEmptyWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByRejectsNonGroupFields", "query/commits/with_doc_id_group_order_test.go::TestQueryCommitsOrderedAndGroupedByDocID"]

`_commits(groupBy: [height|cid|docID|fieldName])` returns one row per distinct value with a `GROUP` sub-list of the member commits, and combines with `order` (`groupBy: [docID], order: {docID: DESC}`); `groupBy: []` returns a single row whose `GROUP` holds every commit. Selecting a non-grouped field at group level (e.g. `groupBy: [cid] { cid height }`, `groupBy: [] { cid }`, `links`/`heads` at group level, or grouped nested `links`/`heads`) errors with `cannot select a non-group-by field at group-level`.

# `go.query.json`: Filtering JSON fields

- **Parents:** ["go.query"]

Filters on a `JSON` scalar field compare whole values or descend into object paths (`custom: {path: {_op: v}}`).

# `go.query.json.ordering`: JSON range comparisons

- **Parents:** ["go.query.json"]

`_gt`, `_geq`, `_lt`, `_leq` on JSON values and paths.

# `go.query.inline_array`: Inline scalar array fields

- **Parents:** ["go.query"]

Fields typed `[T]` / `[T!]` for Boolean, Int, Float, Float32, Float64, String and DateTime.

# `go.query.inline_array.read`: Reading inline arrays

- **Parents:** ["go.query.inline_array"]

Values round-trip through create and query.

# `go.query.inline_array.read.datetime`: DateTime arrays

- **Parents:** ["go.query.inline_array.read"]
- **Go-Tests:** ["query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_Null", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_EmptyList", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_NotEmpty", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithNillableDateTime", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithNillableDateTime_AllNull", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_NonNillable", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_Update", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_UTC_NOW"]
- **Scope:** "TestQueryInlineArrayWithDateTime_UTC_NOW is skipped by the change detector"

`[DateTime!]` and `[DateTime]` fields store RFC 3339 strings and return them as timestamps in order, null, `[]` or with null elements (`[null, null]`); an update replaces the whole array, and the element `"UTC_NOW"` is stored as the current timestamp.

# `go.query.inline_array.non-nillable`: Non-nillable elements and fields

- **Parents:** ["go.query.inline_array"]

Validation of `!` on array elements and on scalar fields.

# `go.query.inline_array.non-nillable.elements-accept`: `[T!]` arrays that are null, omitted or full

- **Parents:** ["go.query.inline_array.non-nillable"]
- **Go-Tests:** ["query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableBooleans_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableBooleans_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableBooleans_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableInts_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableInts_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableInts_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat64s_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat64s_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat64s_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat32s_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat32s_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat32s_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableStrings_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableStrings_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableStrings_Omitted"]

A `[Boolean!]`, `[Int!]`, `[Float64!]`, `[Float32!]` or `[String!]` field (the list itself nillable) accepts a null or omitted list, which reads back as null, and a list of non-null elements, which reads back unchanged.

# `go.query.inline_array.non-nillable.elements-reject`: Invalid array elements rejected on create

- **Parents:** ["go.query.inline_array.non-nillable"]
- **Go-Tests:** ["query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableBooleans_NullElement", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableInts_NullElement", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat64s_NullElement", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableFloat32s_NullElement", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryInlineArrayWithNonNillableStrings_NullElement", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_ErrorMalformed"]

Creating a doc with a null element in a `[T!]` array fails with an error containing `null value provided for non-nillable field` (the Boolean test only checks for `null`); a malformed DateTime element (`["not-a-date"]`) fails with an error containing the offending value `not-a-date`.

# `go.query.inline_array.filter`: Filtering on inline arrays

- **Parents:** ["go.query.inline_array"]

Collection filters on array fields.

# `go.query.inline_array.aggregate`: Aggregates over inline arrays

- **Parents:** ["go.query.inline_array"]

`COUNT`, `SUM`, `AVG`, `MIN`, `MAX` with an array field as the source (`AGG(field: {filter, limit, offset, order})`).

# `go.query.inline_array.aggregate.count`: COUNT over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element counts.

# `go.query.inline_array.aggregate.sum`: SUM over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element sums.

# `go.query.inline_array.aggregate.avg`: AVG over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element averages.

# `go.query.inline_array.aggregate.min`: MIN over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element minima.

# `go.query.inline_array.aggregate.max`: MAX over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element maxima.

# `go.query.one_to_many`: One-to-many relation queries

- **Parents:** ["go.query"]

Schema `Book { author: Author }` / `Author { published: [Book] }` unless stated.

# `go.query.one_to_many.filter`: Filtering across the relation

- **Parents:** ["go.query.one_to_many"]

Filters on parent and child through the relation.

# `go.query.one_to_many.count`: COUNT over a one-to-many relation

- **Parents:** ["go.query.one_to_many"]

`COUNT(published: {...})` on the one side.

# `go.query.one_to_many.aggregate`: SUM, AVG, MIN, MAX over a one-to-many relation

- **Parents:** ["go.query.one_to_many"]

`AGG(published: {field: rating, ...})`.

# `go.query.one_to_many.group`: groupBy with one-to-many relations

- **Parents:** ["go.query.one_to_many"]

Grouping at either level of the relation.

# `go.query.one_to_many.group.join`: Groups that render or filter related objects

- **Parents:** ["go.query.one_to_many.group"]
- **Go-Tests:** ["query/one_to_many/with_group_test.go::TestQueryOneToManyWithInnerJoinGroupNumber", "query/one_to_many/with_group_test.go::TestQueryOneToManyWithParentJoinGroupNumber", "query/one_to_many/with_group_test.go::TestQueryOneToManyWithParentGroupByOnRelationAndDuplicateRelationSelection", "query/one_to_many/with_group_test.go::TestQueryOneToManyWithDuplicateRelationSelectionEachWithInnerGroupByOnRelation", "query/one_to_many/with_group_filter_test.go::TestQueryOneToManyWithParentJoinGroupNumberAndNumberFilterOnJoin", "query/one_to_many/with_group_filter_test.go::TestQueryOneToManyWithParentJoinGroupNumberAndNumberFilterOnGroup", "query/one_to_many/with_group_filter_test.go::TestQueryOneToManyWithParentJoinGroupNumberAndNumberFilterOnGroupAndOnGroupJoin", "query/one_to_many/with_group_filter_test.go::TestQueryOneToManyWithGroupByAndFilterOnParentRelation"]

`published(groupBy: [rating]) { rating GROUP { name } }` groups each author's books; `Author(groupBy: [age]) { age GROUP { name published {...} } }` renders children inside groups, and filters may sit on the parent, on `GROUP(filter: {published: ...})` or on the child selection. A parent filter through the relation (`Book(filter: {author: {name: {_like: "John%"}}}, groupBy: [rating])`) is honoured (regression for Go issue #4880), and selecting the grouped relation twice does not crash; the explain graph for two inner `groupBy: [author]` selections is a `typeJoinMany` over a `groupNode`.

# `go.query.one_to_many.group.by-relation`: Grouping by the relation from the many side

- **Parents:** ["go.query.one_to_many.group"]
- **Go-Tests:** ["query/one_to_many/with_group_related_id_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeIDFromManySide", "query/one_to_many/with_group_related_id_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeIDWithIDSelectionFromManySide", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeFromManySideUsingAlias", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeFromManySideUsingAliasAndRelatedSelection", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeWithIDSelectionFromManySideUsingAlias", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeWithIDSelectionFromManySideUsingAliasAndRelatedSelection", "query/one_to_many/with_group_unselected_field_test.go::TestQueryOneToMany_WithGroupByRelatedIDWithoutRenderedGroupField", "query/one_to_many/with_group_unselected_field_test.go::TestQueryOneToMany_WithGroupByRelationObjectRenderingRelatedID", "query/one_to_many/with_group_unselected_field_test.go::TestQueryOneToMany_WithGroupByRelationObjectWithoutRenderedGroupField"]

`Book(groupBy: [_authorID])` and `Book(groupBy: [author])` produce one group per author; the group row may render `_authorID`, the `author { _docID name }` object, both, or neither (only `GROUP`), and each `GROUP` lists that author's books with their own `author` sub-selection.

# `go.query.one_to_many.group.errors`: Invalid groupBy selections

- **Parents:** ["go.query.one_to_many.group"]
- **Go-Tests:** ["query/one_to_many/with_group_test.go::TestQueryOneToManyWithInnerJoinGroupNumberWithNonGroupFieldsSelected", "query/one_to_many/with_group_test.go::TestQueryOneToManyWithGroupByGenreAndNonGroupAuthorSelected", "query/one_to_many/with_group_related_id_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeFromSingleSide", "query/one_to_many/with_group_related_id_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeWithIDSelectionFromSingleSide", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeFromSingleSideUsingAlias", "query/one_to_many/with_group_related_id_alias_test.go::TestQueryOneToManyWithParentGroupByOnRelatedTypeWithIDSelectionFromSingleSideUsingAlias"]

Selecting a non-grouped scalar or relation at group level (`published(groupBy: [rating]) { rating name }`, `Book(groupBy: [genre]) { genre author { name } }`) errors with `cannot select a non-group-by field at group-level`. On the one side, `Author(groupBy: [_publishedID])` errors with `Argument "groupBy" has invalid value [_publishedID].\nIn element #1: Expected type "AuthorField", found _publishedID.`, `Author(groupBy: [published])` with `cannot group by array or object field. Field: published`, and selecting `_publishedID` with `Cannot query field "_publishedID" on type "Author". `.

# `go.query.one_to_one`: One-to-one relation queries

- **Parents:** ["go.query"]

Schema `Book { author: Author }` / `Author { published: Book @primary }` unless stated.

# `go.query.one_to_one.filter-relation-id`: Filtering on the secondary side's relation ID

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqNullFilterOnSecondaryRelationID_ShouldReturnOrphans", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithNeqNullFilterOnSecondaryRelationID_ShouldReturnRelated", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqFilterOnSecondaryRelationID_ShouldReturnRelated", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqNullFilterOnSecondaryRelationIDAndOtherField_ShouldReturnMatchingOrphan"]

With `Publisher { book: Book @primary }`, `Book(filter: {_publisherID: {_eq: null}})` returns the books no publisher points at, `_neq: null` and `_eq: <publisher docID>` return the linked book, and the null condition combines with other field conditions.

# `go.query.one_to_one.order`: Ordering by related fields

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildBooleanOrderDescending", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildBooleanOrderAscending", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildIntOrderDescendingWithNoSubTypeFieldsSelected", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildIntOrderAscendingWithNoSubTypeFieldsSelected", "query/one_to_one/with_order_test.go::TestQueryOneToOne_WithAliasedChildIntOrderAscending_ShouldOrder", "query/one_to_one/with_order_test.go::TestQueryOneToOne_WithChildAliasedIntOrderAscending_ShouldOrder", "query/one_to_one/with_filter_order_test.go::TestOnetoOneSubTypeDscOrderByQueryWithFilterHavinghNoSubTypeSelections", "query/one_to_one/with_filter_order_test.go::TestOnetoOneSubTypeAscOrderByQueryWithFilterHavinghNoSubTypeSelections"]

`Book(order: {author: {age|verified: ASC|DESC}})` orders books by the related author's field whether or not `author` is selected and combined with a relation filter; ordering works through an aliased relation (`order: {_alias: {writer: {age: ASC}}}`) and an aliased child field (`order: {author: {_alias: {authorAge: ASC}}}`).

# `go.query.many_to_many`: Many-to-many through a join collection

- **Parents:** ["go.query"]

# `go.query.one_to_many_multiple`: Several one-to-many relations on one type

- **Parents:** ["go.query"]

`Author { books: [Book], articles: [Article] }`.

# `go.query.one_to_many_to_many`: One-to-many followed by one-to-many

- **Parents:** ["go.query"]

# `go.query.one_to_many_to_one`: One-to-many followed by one-to-one

- **Parents:** ["go.query"]

Schema `Author { book: [Book] }`, `Book { publisher: Publisher }`, `Publisher { book: Book @primary }`.

# `go.query.one_to_one_multiple`: Several one-to-one relations on one type

- **Parents:** ["go.query"]

# `go.query.one_to_one_to_many`: One-to-one followed by one-to-many

- **Parents:** ["go.query"]

# `go.query.one_to_one_to_one`: Chains of one-to-one relations

- **Parents:** ["go.query"]

# `go.query.one_to_one_to_one.chain`: Resolving and ordering a one-to-one chain

- **Parents:** ["go.query.one_to_one_to_one"]
- **Go-Tests:** ["query/one_to_one_to_one/simple_test.go::TestQueryOneToOneToOne", "query/one_to_one_to_one/simple_test.go::TestQueryOneToOneToOneSecondaryThenPrimary", "query/one_to_one_to_one/simple_test.go::TestQueryOneToOneToOnePrimaryThenSecondary", "query/one_to_one_to_one/simple_test.go::TestQueryOneToOneToOneSecondary", "query/one_to_one_to_one/with_order_test.go::TestQueryOneToOneToOneWithNestedOrder"]

`Publisher { printed { author { name } } }` resolves for every placement of `@primary` along Publisher–Book–Author, and `Publisher(order: {printed: {author: {name: ASC}}})` orders by a field two relations away.

# `go.query.one_to_two_many`: Two one-to-many relations between the same types

- **Parents:** ["go.query"]
