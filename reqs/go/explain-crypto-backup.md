# `go.explain`: Query plan explanation via the `@explain` directive

- **Parents:** ["go"]

An operation annotated with `@explain` returns a description of its query plan (default and `simple`), the plan's node names only (`debug`), or per-node execution counters (`execute`) instead of its normal result.

# `go.explain.default`: Default (and `simple`) explain: plan graph with node attributes

- **Parents:** ["go.explain"]

`@explain` with no type, or `@explain(type: simple)`, returns `{explain: {operationNode: [...]}}` holding the nested plan nodes with their attributes (filters, docIDs, prefixes, collection name and ID, limits, orderings, sources) without running the operation.

# `go.explain.default.basic`: Plain select plan

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/basic_test.go::TestDefaultExplainRequestWithFullBasicGraph", "explain/default/basic_test.go::TestDefaultExplainWithAlias", "explain/simple/basic_test.go::TestSimpleExplainRequest"]

`query @explain { Author { name age } }` (and the same with `@explain(type: simple)` or with aliased fields) returns exactly `selectTopNode > selectNode{docID: nil, filter: nil} > scanNode{filter: nil, collectionName: "Author", collectionID: <CID>, prefixes: ["/3"]}`.

# `go.explain.default.mutation`: Mutation plans

- **Parents:** ["go.explain.default"]

Explained mutations return the mutation node with its arguments and a scan node beneath it; the mutation is not applied.

# `go.explain.default.mutation.add_upsert`: Add and upsert plan nodes

- **Parents:** ["go.explain.default.mutation"]
- **Go-Tests:** ["explain/default/add_test.go::TestDefaultExplainMutationRequestWithAdd", "explain/default/add_test.go::TestDefaultExplainMutationRequestDoesNotAddDocGivenDuplicate", "explain/default/upsert_test.go::TestDefaultExplainMutationRequest_WithUpsert_Succeeds"]

`add_Author(input: {...})` under `@explain` returns an `addNode` whose `input` attribute echoes the input object, and creates no document (a second identical explained add does not fail as a duplicate); `upsert_Author(filter, add, update)` returns an `upsertNode` with `filter` and `update` attributes over a `scanNode` carrying the filter.

# `go.explain.default.mutation.delete`: Delete plan nodes

- **Parents:** ["go.explain.default.mutation"]
- **Go-Tests:** ["explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingFilter", "explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingFilterToMatchEverything", "explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingId", "explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingIds", "explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingNoIds", "explain/default/delete_test.go::TestDefaultExplainMutationRequestWithDeleteUsingFilterAndIds"]
- **Go-Bug-Suspect:** "'delete_Author(docID: [])' is explained with 'docID: []' and a scan over the whole collection prefix '/3', so an empty docID list appears to mean 'all documents' rather than 'none'."

`delete_Author` under `@explain` returns a `deleteNode` whose `filter` and `docID` attributes echo the arguments (a single docID is shown as a one-element list) over a `scanNode` for the collection; the scan carries the filter and is limited to the requested docIDs. Two empty arguments differ: `filter: {}` is shown as `filter: null` on both nodes, and `docID: []` is shown as `docID: []` with a scan over the whole collection prefix.

# `go.explain.default.mutation.update`: Update plan nodes

- **Parents:** ["go.explain.default.mutation"]
- **Go-Tests:** ["explain/default/update_test.go::TestDefaultExplainMutationRequestWithUpdateUsingBooleanFilter", "explain/default/update_test.go::TestDefaultExplainMutationRequestWithUpdateUsingIds", "explain/default/update_test.go::TestDefaultExplainMutationRequestWithUpdateUsingId", "explain/default/update_test.go::TestDefaultExplainMutationRequestWithUpdateUsingIdsAndFilter"]

`update_Author(filter|docID, input)` under `@explain` returns an `updateNode` with `filter`, `docID` (list) and `input` attributes over a `scanNode` that carries the filter and is limited to the given docIDs.

# `go.explain.default.dagscan`: Commit query plan

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/dagscan_test.go::TestDefaultExplainCommitsDagScanQueryOp", "explain/default/dagscan_test.go::TestDefaultExplainCommitsDagScanQueryOpWithoutField"]

`_commits(docID: ..., filter: {fieldName: ...})` under `@explain` returns a `dagScanNode` whose `cid` is nil and whose `prefixes` name the document (and the field, when filtered by `fieldName`).

# `go.explain.default.filter`: Filter pushdown into the scan

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/with_filter_test.go::TestDefaultExplainRequestWithStringEqualFilter", "explain/default/with_filter_test.go::TestDefaultExplainRequestWithIntegerEqualFilter", "explain/default/with_filter_test.go::TestDefaultExplainRequestWithGreaterThanFilter", "explain/default/with_filter_test.go::TestDefaultExplainRequestWithLogicalCompoundAndFilter", "explain/default/with_filter_test.go::TestDefaultExplainRequestWithLogicalCompoundOrFilter", "explain/default/with_filter_test.go::TestDefaultExplainRequestWithMatchInsideList", "explain/default/with_filter_test.go::TestDefaultExplainRequest_WithJSONEqualFilter_Succeeds"]

A collection `filter` (string or integer `_eq`, `_gt`, `_and`, `_or`, `_in`, a JSON `_eq` on an object) appears verbatim as the `filter` attribute of the `scanNode`, with the collection prefix in `prefixes`.

# `go.explain.default.docid`: DocID selection

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/with_filter_doc_id_test.go::TestDefaultExplainRequestWithDocIDFilter", "explain/default/with_filter_doc_id_test.go::TestDefaultExplainRequestWithDocIDsFilterUsingOneID", "explain/default/with_filter_doc_id_test.go::TestDefaultExplainRequestWithDocIDsFilterUsingMultipleButDuplicateIDs", "explain/default/with_filter_doc_id_test.go::TestDefaultExplainRequestWithDocIDsFilterUsingMultipleUniqueIDs", "explain/default/with_filter_doc_id_test.go::TestDefaultExplainRequestWithMatchingIDFilter"]

`docID: "..."` or `docID: [...]` (duplicates kept as given) is shown on the `selectNode` as a docID list with an empty-prefix `scanNode`; filtering on `_docID: {_eq: ...}` instead stays a scan `filter`.

# `go.explain.default.index`: Indexed filter and order plans

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/with_index_test.go::TestDefaultExplainWithIndexOnFilter", "explain/default/with_index_test.go::TestDefaultExplainWithIndexOnOrder", "explain/default/with_index_test.go::TestDefaultExplainWithIndexOnSubqueryNestedRelationOrder", "explain/simple/with_index_test.go::TestSimpleExplainWithIndexOnFilter", "explain/simple/with_index_test.go::TestSimpleExplainWithIndexOnOrder", "explain/simple/with_index_test.go::TestSimpleExplainWithIndexOnSubqueryNestedRelationOrder"]

On a collection with `@index` on `age`, filtering on or ordering by `age` explains as `selectTopNode > selectNode > scanNode` with no `orderNode`; ordering a child by an indexed field of a related collection with `limit: 2` puts a `limitNode` but no `orderNode` in the child plan. Holds for default and `simple` types.

# `go.explain.default.group`: Group-by plans

- **Parents:** ["go.explain.default"]

Requests with `groupBy` explain with a `groupNode`.

# `go.explain.default.group.selection`: Group node attributes for filters and docIDs

- **Parents:** ["go.explain.default.group"]
- **Go-Tests:** ["explain/default/group_test.go::TestDefaultExplainRequestWithGroupByOnParent", "explain/default/group_test.go::TestDefaultExplainRequestWithGroupByTwoFieldsOnParent", "explain/default/group_with_doc_id_test.go::TestDefaultExplainRequestWithDocIDOnParentGroupBy", "explain/default/group_with_doc_id_test.go::TestDefaultExplainRequestWithDocIDsAndFilterOnParentGroupBy", "explain/default/group_with_doc_id_child_test.go::TestDefaultExplainRequestWithDocIDsOnInnerGroupSelection", "explain/default/group_with_filter_test.go::TestDefaultExplainRequestWithFilterOnGroupByParent", "explain/default/group_with_filter_child_test.go::TestDefaultExplainRequestWithFilterOnInnerGroupSelection", "explain/default/group_with_filter_child_test.go::TestDefaultExplainRequestWithFilterOnParentGroupByAndInnerGroupSelection"]

`groupBy: [age]` (or two fields) yields a `groupNode{groupByFields, childSelects}`; a parent `filter` or `docID` is pushed to the `scanNode` (`filter`, `prefixes`), while a `filter` or `docID` on the inner `GROUP` selection appears in the `childSelects` entry.

# `go.explain.default.group.limit`: Group plans with limit and offset

- **Parents:** ["go.explain.default.group"]
- **Go-Tests:** ["explain/default/group_with_limit_test.go::TestDefaultExplainRequestWithLimitAndOffsetOnParentGroupBy", "explain/default/group_with_limit_test.go::TestDefaultExplainRequestWithLimitOnParentGroupByAndInnerGroupSelection", "explain/default/group_with_limit_child_test.go::TestDefaultExplainRequestWithLimitAndOffsetOnInnerGroupSelection", "explain/default/group_with_limit_child_test.go::TestDefaultExplainRequestWithLimitAndOffsetOnMultipleInnerGroupSelections"]

`limit`/`offset` on a grouped parent add a `limitNode{limit, offset}` above the `groupNode`; `limit`/`offset` on inner `GROUP` selections (one or several aliased) appear in each `childSelects` entry.

# `go.explain.default.group.order`: Group plans with ordering

- **Parents:** ["go.explain.default.group"]
- **Go-Tests:** ["explain/default/group_with_order_test.go::TestDefaultExplainRequestWithDescendingOrderOnParentGroupBy", "explain/default/group_with_order_test.go::TestDefaultExplainRequestWithAscendingOrderOnParentGroupBy", "explain/default/group_with_order_test.go::TestDefaultExplainRequestWithOrderOnParentGroupByAndOnInnerGroupSelection", "explain/default/group_with_order_child_test.go::TestDefaultExplainRequestWithDescendingOrderOnInnerGroupSelection", "explain/default/group_with_order_child_test.go::TestDefaultExplainRequestWithAscendingOrderOnInnerGroupSelection", "explain/default/group_with_order_child_test.go::TestDefaultExplainRequestWithOrderOnNestedParentGroupByAndOnNestedParentsInnerGroupSelection"]

`order` on a grouped parent adds an `orderNode{orderings: [{direction, fields}]}`; `order` on an inner `GROUP` (including nested group-bys) appears in the matching `childSelects` entry's `orderBy`.

# `go.explain.default.group.average`: Averages over groups

- **Parents:** ["go.explain.default.group"]
- **Go-Tests:** ["explain/default/group_with_average_test.go::TestDefaultExplainRequestWithGroupByWithAverageOnAnInnerField", "explain/default/group_with_average_test.go::TestDefaultExplainRequestWithAverageInsideTheInnerGroupOnAField", "explain/default/group_with_average_test.go::TestDefaultExplainRequestWithAverageInsideTheInnerGroupOnAFieldAndNestedGroupBy", "explain/default/group_with_average_test.go::TestDefaultExplainRequestWithAverageInsideTheInnerGroupAndNestedGroupByWithAverage"]

`AVG(GROUP: {field: ...})`, including averages inside nested `GROUP(groupBy: ...)` selections, explain as `averageNode > countNode > sumNode > groupNode`, with `sources` naming the `GROUP` field and child field.

# `go.explain.default.aggregate`: Aggregate plans

- **Parents:** ["go.explain.default"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX` explain as dedicated nodes whose `sources` list the aggregated field, child field and filter.

# `go.explain.default.aggregate.top_level`: Top-level aggregates

- **Parents:** ["go.explain.default.aggregate"]
- **Go-Tests:** ["explain/default/top_with_average_test.go::TestDefaultExplainTopLevelAverageRequest", "explain/default/top_with_average_test.go::TestDefaultExplainTopLevelAverageRequestWithFilter", "explain/default/top_with_count_test.go::TestDefaultExplainTopLevelCountRequest", "explain/default/top_with_count_test.go::TestDefaultExplainTopLevelCountRequestWithFilter", "explain/default/top_with_max_test.go::TestDefaultExplain_WithTopLevelMaxRequest_Succeeds", "explain/default/top_with_max_test.go::TestDefaultExplain_WithTopLevelMaxRequestWithFilter_Succeeds", "explain/default/top_with_min_test.go::TestDefaultExplain_WithTopLevelMinRequest_Succeeds", "explain/default/top_with_min_test.go::TestDefaultExplain_WithTopLevelMinRequestWithFilter_Succeeds", "explain/default/top_with_sum_test.go::TestDefaultExplainTopLevelSumRequest", "explain/default/top_with_sum_test.go::TestDefaultExplainTopLevelSumRequestWithFilter"]

`COUNT(Author: {...})`, `SUM`, `AVG`, `MIN` and `MAX(Author: {field: age, filter?})` explain as the aggregate node (`AVG` as `averageNode > countNode > sumNode`) over a `scanNode` of `Author`; the optional filter appears in both the aggregate `sources` and the scan `filter`.

# `go.explain.default.aggregate.inline_array`: Aggregates over inline arrays

- **Parents:** ["go.explain.default.aggregate"]
- **Go-Tests:** ["explain/default/with_average_test.go::TestDefaultExplainRequestWithAverageOnArrayField", "explain/default/with_count_test.go::TestDefaultExplainRequestWithCountOnInlineArrayField", "explain/default/with_max_test.go::TestDefaultExplainRequest_WithMaxOnInlineArrayField_ChildFieldWillBeEmpty", "explain/default/with_min_test.go::TestDefaultExplainRequest_WithMinOnInlineArrayField_ChildFieldWillBeEmpty", "explain/default/with_sum_test.go::TestDefaultExplainRequestWithSumOnInlineArrayField_ChildFieldWillBeEmpty"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX(chapterPages: {})` on a `[Int!]` field explain with `sources: [{fieldName: "chapterPages", childFieldName: nil, filter: nil}]` over a plain `scanNode` of `Book`.

# `go.explain.default.aggregate.join`: Aggregates over one-to-many relations

- **Parents:** ["go.explain.default.aggregate"]
- **Go-Tests:** ["explain/default/with_average_join_test.go::TestDefaultExplainRequestWithAverageOnJoinedField", "explain/default/with_average_join_test.go::TestDefaultExplainRequestWithAverageOnMultipleJoinedFieldsWithFilter", "explain/default/with_count_join_test.go::TestDefaultExplainRequestWithCountOnOneToManyJoinedField", "explain/default/with_count_join_test.go::TestDefaultExplainRequestWithCountOnOneToManyJoinedFieldWithManySources", "explain/default/with_max_join_test.go::TestDefaultExplainRequest_WithMaxOnOneToManyJoinedField_Succeeds", "explain/default/with_max_join_test.go::TestDefaultExplainRequest_WithMaxOnOneToManyJoinedFieldWithFilter_Succeeds", "explain/default/with_max_join_test.go::TestDefaultExplainRequest_WithMaxOnOneToManyJoinedFieldWithManySources_Succeeds", "explain/default/with_min_join_test.go::TestDefaultExplainRequest_WithMinOnOneToManyJoinedField_Succeeds", "explain/default/with_min_join_test.go::TestDefaultExplainRequest_WithMinOnOneToManyJoinedFieldWithFilter_Succeeds", "explain/default/with_min_join_test.go::TestDefaultExplainRequest_WithMinOnOneToManyJoinedFieldWithManySources_Succeeds", "explain/default/with_sum_join_test.go::TestDefaultExplainRequestWithSumOnOneToManyJoinedField", "explain/default/with_sum_join_test.go::TestDefaultExplainRequestWithSumOnOneToManyJoinedFieldWithFilter", "explain/default/with_sum_join_test.go::TestDefaultExplainRequestWithSumOnOneToManyJoinedFieldWithManySources"]

`COUNT(books: {})`, `SUM`/`AVG`/`MIN`/`MAX(books: {field: pages}, articles: {...})` explain as the aggregate node over a `typeIndexJoin` (`joinType: typeJoinMany`); a source filter appears in `sources`, and several sources produce a `parallelNode` of joins.

# `go.explain.default.aggregate.shared_join`: Aggregate and child selection sharing a join

- **Parents:** ["go.explain.default.aggregate"]
- **Go-Tests:** ["explain/default/with_average_join_test.go::TestDefaultExplainRequestOneToManyWithAverageAndChildNeNilFilterSharesJoinField", "explain/default/with_count_join_test.go::TestDefaultExplainRequestOneToManyWithCountWithFilterAndChildFilterSharesJoinField", "explain/default/with_count_join_test.go::TestDefaultExplainRequestOneToManyWithCountAndChildFilterDoesNotShareJoinField"]

When an aggregate over `books` and a `books(filter: ...)` child selection use the same filter (`AVG(books: {field: rating})` with child filter `rating: {_neq: null}`, or `COUNT(books: {filter: {rating: {_neq: null}}})`), the plan has one `typeIndexJoin` under the aggregate; when the filters differ (`COUNT(books: {})` vs a filtered child) the plan has a `parallelNode` with two joins.

# `go.explain.default.join`: Relation join plans

- **Parents:** ["go.explain.default"]

Selecting related objects explains as `typeIndexJoin` nodes.

# `go.explain.default.join.structure`: Join node shape

- **Parents:** ["go.explain.default.join"]
- **Go-Tests:** ["explain/default/type_join_test.go::TestDefaultExplainRequestWith2SingleJoinsAnd1ManyJoin", "explain/default/type_join_many_test.go::TestDefaultExplainRequestWithAOneToManyJoin", "explain/default/type_join_one_test.go::TestDefaultExplainRequestWithAOneToOneJoin", "explain/default/type_join_one_test.go::TestDefaultExplainRequestWithTwoLevelDeepNestedJoins"]

A one-to-many child (`articles`) gives `typeIndexJoin{joinType: "typeJoinMany", rootName, subTypeName}` with `root` and `subType` plans; a one-to-one child (`contact`) gives `joinType: "typeJoinOne"`, nested two levels for `contact { address }`; several children give a `parallelNode` of joins.

# `go.explain.default.join.related_filter`: Filters on related fields

- **Parents:** ["go.explain.default.join"]
- **Go-Tests:** ["explain/default/type_join_with_filter_test.go::TestDefaultExplainRequestWithRelatedAndRegularFilter", "explain/default/type_join_with_filter_test.go::TestDefaultExplainRequestWithManyRelatedFilters", "explain/default/type_join_with_filter_doc_id_test.go::TestDefaultExplainRequestWithRelatedAndRegularFilterAndDocIDs", "explain/default/type_join_with_filter_doc_id_test.go::TestDefaultExplainRequestWithManyRelatedFiltersAndDocID"]

A parent filter that mixes own fields with related fields (`books: {name: ...}`, `articles: {...}`), optionally with `docID`, keeps the whole filter on the parent `selectNode`, pushes only the own-field part to the parent `scanNode`, and adds one `typeIndexJoin` per related field (a `parallelNode` for several).

# `go.explain.default.limit`: Limit and offset plans

- **Parents:** ["go.explain.default"]

`limit`/`offset` explain as a `limitNode{limit, offset}`.

# `go.explain.default.limit.parent`: Limit on the root collection

- **Parents:** ["go.explain.default.limit"]
- **Go-Tests:** ["explain/default/with_limit_test.go::TestDefaultExplainRequestWithOnlyLimit", "explain/default/with_limit_test.go::TestDefaultExplainRequestWithOnlyOffset", "explain/default/with_limit_test.go::TestDefaultExplainRequestWithLimitAndOffset"]

`Author(limit: 2)`, `Author(offset: 2)` and `Author(limit: 3, offset: 1)` explain with `limitNode{limit, offset}` (`limit: nil` when only offset is given) above the `selectNode`.

# `go.explain.default.limit.child`: Limit on related children

- **Parents:** ["go.explain.default.limit"]
- **Go-Tests:** ["explain/default/with_limit_join_test.go::TestDefaultExplainRequestWithOnlyLimitOnRelatedChild", "explain/default/with_limit_join_test.go::TestDefaultExplainRequestWithOnlyOffsetOnRelatedChild", "explain/default/with_limit_join_test.go::TestDefaultExplainRequestWithBothLimitAndOffsetOnRelatedChild", "explain/default/with_limit_join_test.go::TestDefaultExplainRequestWithLimitOnRelatedChildAndBothLimitAndOffsetOnParent", "explain/default/with_limit_count_test.go::TestDefaultExplainRequestWithOnlyLimitOnRelatedChildWithCount", "explain/default/with_limit_count_test.go::TestDefaultExplainRequestWithLimitArgsOnParentAndRelatedChildWithCount"]

`articles(limit: ..., offset: ...)` puts a `limitNode` in the join's `subType` plan, independently of a parent `limitNode`; combined with `COUNT(articles: {})` the plan has a `parallelNode` with the count's join and the limited child join.

# `go.explain.default.order`: Order plans

- **Parents:** ["go.explain.default"]
- **Go-Tests:** ["explain/default/with_order_test.go::TestDefaultExplainRequestWithAscendingOrderOnParent", "explain/default/with_order_test.go::TestDefaultExplainRequestWithMultiOrderFieldsOnParent", "explain/default/with_order_join_test.go::TestDefaultExplainRequestWithOrderFieldOnRelatedChild", "explain/default/with_order_join_test.go::TestDefaultExplainRequestWithOrderFieldOnParentAndRelatedChild"]

`order: {age: ASC}` or a list of orderings on the parent, and `order` on a related child, explain as `orderNode{orderings: [{direction, fields}]}` at the corresponding level.

# `go.explain.debug`: Debug explain: plan node names without attributes

- **Parents:** ["go.explain"]

`@explain(type: debug)` returns the same plan tree as the default type but with node names only and no attributes, for queries and mutations alike.

# `go.explain.debug.scan`: Scan, filter and docID plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/basic_test.go::TestDebugExplainRequest", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithStringEqualFilter", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithIntegerEqualFilter", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithGreaterThanFilter", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithLogicalCompoundAndFilter", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithLogicalCompoundOrFilter", "explain/debug/with_filter_test.go::TestDebugExplainRequestWithMatchInsideList", "explain/debug/with_filter_doc_id_test.go::TestDebugExplainRequestWithDocIDFilter", "explain/debug/with_filter_doc_id_test.go::TestDebugExplainRequestWithDocIDsFilterUsingOneID", "explain/debug/with_filter_doc_id_test.go::TestDebugExplainRequestWithDocIDsFilterUsingMultipleButDuplicateIDs", "explain/debug/with_filter_doc_id_test.go::TestDebugExplainRequestWithDocIDsFilterUsingMultipleUniqueIDs", "explain/debug/with_filter_doc_id_test.go::TestDebugExplainRequestWithMatchingIDFilter"]

Plain selects, collection filters and docID selections explain as `selectTopNode > selectNode > scanNode` with empty node bodies.

# `go.explain.debug.mutation`: Mutation plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/add_test.go::TestDebugExplainMutationRequestWithAdd", "explain/debug/add_test.go::TestDebugExplainMutationRequestDoesNotAddDocGivenDuplicate", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingFilter", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingFilterToMatchEverything", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingId", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingIds", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingNoIds", "explain/debug/delete_test.go::TestDebugExplainMutationRequestWithDeleteUsingFilterAndIds", "explain/debug/update_test.go::TestDebugExplainMutationRequestWithUpdateUsingBooleanFilter", "explain/debug/update_test.go::TestDebugExplainMutationRequestWithUpdateUsingIds", "explain/debug/update_test.go::TestDebugExplainMutationRequestWithUpdateUsingId", "explain/debug/update_test.go::TestDebugExplainMutationRequestWithUpdateUsingIdsAndFilter", "explain/debug/upsert_test.go::TestDebugExplainMutationRequest_WithUpsert_Succeeds"]

Explained `add_`, `delete_`, `update_` and `upsert_` mutations return `addNode`, `deleteNode > selectTopNode > selectNode > scanNode`, `updateNode > ...` and `upsertNode > ...` respectively, and no document is written.

# `go.explain.debug.dagscan`: Commit query plan

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/dagscan_test.go::TestDebugExplainCommitsDagScanQueryOp", "explain/debug/dagscan_test.go::TestDebugExplainCommitsDagScanQueryOpWithoutField"]

`_commits(docID: ...)` with or without a `fieldName` filter explains as `selectTopNode > selectNode > dagScanNode`.

# `go.explain.debug.group`: Group-by plans

- **Parents:** ["go.explain.debug"]

Grouped requests explain with a `groupNode` in debug mode.

# `go.explain.debug.group.selection`: Group with filters and docIDs

- **Parents:** ["go.explain.debug.group"]
- **Go-Tests:** ["explain/debug/group_test.go::TestDebugExplainRequestWithGroupByOnParent", "explain/debug/group_test.go::TestDebugExplainRequestWithGroupByTwoFieldsOnParent", "explain/debug/group_with_doc_id_test.go::TestDebugExplainRequestWithDocIDOnParentGroupBy", "explain/debug/group_with_doc_id_test.go::TestDebugExplainRequestWithDocIDsAndFilterOnParentGroupBy", "explain/debug/group_with_doc_id_child_test.go::TestDebugExplainRequestWithDocIDsOnInnerGroupSelection", "explain/debug/group_with_filter_test.go::TestDebugExplainRequestWithFilterOnGroupByParent", "explain/debug/group_with_filter_child_test.go::TestDebugExplainRequestWithFilterOnInnerGroupSelection", "explain/debug/group_with_filter_child_test.go::TestDebugExplainRequestWithFilterOnParentGroupByAndInnerGroupSelection"]

`groupBy` with parent or inner `GROUP` filters and docIDs explains as `selectTopNode > groupNode > selectNode > scanNode`.

# `go.explain.debug.group.limit_order`: Group with limit and order

- **Parents:** ["go.explain.debug.group"]
- **Go-Tests:** ["explain/debug/group_with_limit_test.go::TestDebugExplainRequestWithLimitAndOffsetOnParentGroupBy", "explain/debug/group_with_limit_test.go::TestDebugExplainRequestWithLimitOnParentGroupByAndInnerGroupSelection", "explain/debug/group_with_limit_child_test.go::TestDebugExplainRequestWithLimitAndOffsetOnInnerGroupSelection", "explain/debug/group_with_limit_child_test.go::TestDebugExplainRequestWithLimitAndOffsetOnMultipleInnerGroupSelections", "explain/debug/group_with_order_test.go::TestDebugExplainRequestWithDescendingOrderOnParentGroupBy", "explain/debug/group_with_order_test.go::TestDebugExplainRequestWithAscendingOrderOnParentGroupBy", "explain/debug/group_with_order_test.go::TestDebugExplainRequestWithOrderOnParentGroupByAndOnInnerGroupSelection", "explain/debug/group_with_order_child_test.go::TestDebugExplainRequestWithDescendingOrderOnInnerGroupSelection", "explain/debug/group_with_order_child_test.go::TestDebugExplainRequestWithAscendingOrderOnInnerGroupSelection", "explain/debug/group_with_order_child_test.go::TestDebugExplainRequestWithOrderOnNestedParentGroupByAndOnNestedParentsInnerGroupSelection"]

Parent `limit`/`order` on a grouped request add `limitNode`/`orderNode` above `groupNode`; the same arguments on inner `GROUP` selections add no extra nodes.

# `go.explain.debug.group.average`: Averages over groups

- **Parents:** ["go.explain.debug.group"]
- **Go-Tests:** ["explain/debug/group_with_average_test.go::TestDebugExplainRequestWithGroupByWithAverageOnAnInnerField", "explain/debug/group_with_average_test.go::TestDebugExplainRequestWithAverageInsideTheInnerGroupOnAField", "explain/debug/group_with_average_test.go::TestDebugExplainRequestWithAverageInsideTheInnerGroupOnAFieldAndNestedGroupBy", "explain/debug/group_with_average_test.go::TestDebugExplainRequestWithAverageInsideTheInnerGroupAndNestedGroupByWithAverage"]

`AVG(GROUP: {field: ...})`, including nested groups, explains as `averageNode > countNode > sumNode > groupNode > selectNode > scanNode`.

# `go.explain.debug.aggregate`: Aggregate plans

- **Parents:** ["go.explain.debug"]

Aggregates explain as their node names in debug mode.

# `go.explain.debug.aggregate.top_level`: Top-level aggregates

- **Parents:** ["go.explain.debug.aggregate"]
- **Go-Tests:** ["explain/debug/top_with_average_test.go::TestDebugExplainTopLevelAverageRequest", "explain/debug/top_with_average_test.go::TestDebugExplainTopLevelAverageRequestWithFilter", "explain/debug/top_with_count_test.go::TestDebugExplainTopLevelCountRequest", "explain/debug/top_with_count_test.go::TestDebugExplainTopLevelCountRequestWithFilter", "explain/debug/top_with_max_test.go::TestDebugExplain_TopLevelMaxRequest_Succeeds", "explain/debug/top_with_max_test.go::TestDebugExplain_TopLevelMaxRequestWithFilter_Succeeds", "explain/debug/top_with_min_test.go::TestDebugExplain_TopLevelMinRequest_Succeeds", "explain/debug/top_with_min_test.go::TestDebugExplain_TopLevelMinRequestWithFilter_Succeeds", "explain/debug/top_with_sum_test.go::TestDebugExplainTopLevelSumRequest", "explain/debug/top_with_sum_test.go::TestDebugExplainTopLevelSumRequestWithFilter"]

Top-level `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` over `Author`, with or without filter, explain as the aggregate node(s) over `selectNode > scanNode`.

# `go.explain.debug.aggregate.inline_array`: Aggregates over inline arrays

- **Parents:** ["go.explain.debug.aggregate"]
- **Go-Tests:** ["explain/debug/with_average_test.go::TestDebugExplainRequestWithAverageOnArrayField", "explain/debug/with_count_test.go::TestDebugExplainRequestWithCountOnInlineArrayField", "explain/debug/with_max_test.go::TestDebugExplainRequestWithMaxOnInlineArrayField_ChildFieldWillBeEmpty", "explain/debug/with_min_test.go::TestDebugExplainRequestWithMinOnInlineArrayField_ChildFieldWillBeEmpty", "explain/debug/with_sum_test.go::TestDebugExplainRequestWithSumOnInlineArrayField_ChildFieldWillBeEmpty"]

`COUNT`/`SUM`/`AVG`/`MIN`/`MAX(chapterPages: {})` explain as the aggregate node(s) over `selectNode > scanNode` with no join.

# `go.explain.debug.aggregate.join`: Aggregates over relations

- **Parents:** ["go.explain.debug.aggregate"]
- **Go-Tests:** ["explain/debug/with_average_join_test.go::TestDebugExplainRequestWithAverageOnJoinedField", "explain/debug/with_average_join_test.go::TestDebugExplainRequestWithAverageOnMultipleJoinedFieldsWithFilter", "explain/debug/with_count_join_test.go::TestDebugExplainRequestWithCountOnOneToManyJoinedField", "explain/debug/with_count_join_test.go::TestDebugExplainRequestWithCountOnOneToManyJoinedFieldWithManySources", "explain/debug/with_max_join_test.go::TestDebugExplainRequest_WithMaxOnOneToManyJoinedField_Succeeds", "explain/debug/with_max_join_test.go::TestDebugExplainRequest_WithMaxOnOneToManyJoinedFieldWithFilter_Succeeds", "explain/debug/with_max_join_test.go::TestDebugExplainRequest_WithMaxOnOneToManyJoinedFieldWithManySources_Succeeds", "explain/debug/with_min_join_test.go::TestDebugExplainRequest_WithMinOnOneToManyJoinedField_Succeeds", "explain/debug/with_min_join_test.go::TestDebugExplainRequest_WithMinOnOneToManyJoinedFieldWithFilter_Succeeds", "explain/debug/with_min_join_test.go::TestDebugExplainRequest_WithMinOnOneToManyJoinedFieldWithManySources_Succeeds", "explain/debug/with_sum_join_test.go::TestDebugExplainRequestWithSumOnOneToManyJoinedField", "explain/debug/with_sum_join_test.go::TestDebugExplainRequestWithSumOnOneToManyJoinedFieldWithFilter", "explain/debug/with_sum_join_test.go::TestDebugExplainRequestWithSumOnOneToManyJoinedFieldWithManySources"]

Aggregates over `books`/`articles` explain as the aggregate node(s) over `selectNode > typeIndexJoin > typeJoinMany`, with a `parallelNode` when there are several sources.

# `go.explain.debug.join`: Join plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/type_join_test.go::TestDebugExplainRequestWith2SingleJoinsAnd1ManyJoin", "explain/debug/type_join_many_test.go::TestDebugExplainRequestWithAOneToManyJoin", "explain/debug/type_join_one_test.go::TestDebugExplainRequestWithAOneToOneJoin", "explain/debug/type_join_one_test.go::TestDebugExplainRequestWithTwoLevelDeepNestedJoins", "explain/debug/type_join_with_filter_test.go::TestDebugExplainRequestWithRelatedAndRegularFilter", "explain/debug/type_join_with_filter_test.go::TestDebugExplainRequestWithManyRelatedFilters", "explain/debug/type_join_with_filter_doc_id_test.go::TestDebugExplainRequestWithRelatedAndRegularFilterAndDocIDs", "explain/debug/type_join_with_filter_doc_id_test.go::TestDebugExplainRequestWithManyRelatedFiltersAndDocID"]

Related selections and related-field filters explain as `typeIndexJoin > typeJoinOne|typeJoinMany` with `root` and `subType` subplans, and a `parallelNode` for several joins.

# `go.explain.debug.limit`: Limit plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/with_limit_test.go::TestDebugExplainRequestWithOnlyLimit", "explain/debug/with_limit_test.go::TestDebugExplainRequestWithOnlyOffset", "explain/debug/with_limit_test.go::TestDebugExplainRequestWithLimitAndOffset", "explain/debug/with_limit_join_test.go::TestDebugExplainRequestWithOnlyLimitOnRelatedChild", "explain/debug/with_limit_join_test.go::TestDebugExplainRequestWithOnlyOffsetOnRelatedChild", "explain/debug/with_limit_join_test.go::TestDebugExplainRequestWithBothLimitAndOffsetOnRelatedChild", "explain/debug/with_limit_join_test.go::TestDebugExplainRequestWithLimitOnRelatedChildAndBothLimitAndOffsetOnParent", "explain/debug/with_limit_count_test.go::TestDebugExplainRequestWithOnlyLimitOnRelatedChildWithCount", "explain/debug/with_limit_count_test.go::TestDebugExplainRequestWithLimitArgsOnParentAndRelatedChildWithCount"]

Parent `limit`/`offset` add a `limitNode` under `selectTopNode`; child `limit`/`offset` add a `limitNode` inside the join's `subType`; combined with `COUNT` the plan has a `parallelNode`.

# `go.explain.debug.order`: Order plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/with_order_test.go::TestDebugExplainRequestWithAscendingOrderOnParent", "explain/debug/with_order_test.go::TestDebugExplainRequestWithMultiOrderFieldsOnParent", "explain/debug/with_order_join_test.go::TestDebugExplainRequestWithOrderFieldOnRelatedChild", "explain/debug/with_order_join_test.go::TestDebugExplainRequestWithOrderFieldOnParentAndRelatedChild", "explain/debug/with_order_join_test.go::TestDebugExplainRequestWithSubqueryOrderByNestedRelationField", "explain/debug/with_order_join_test.go::TestDebugExplainRequestWithSubqueryOrderByNestedRelationFieldASC"]

Parent and child `order` add `orderNode`s; ordering a child by a nested relation field (`published(order: {publisher: {establishedYear: DESC|ASC}}, limit: 2)`) explains with an `orderNode` over a nested one-to-one join inside the child plan.

# `go.explain.debug.index`: Indexed filter and order plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/with_index_test.go::TestDebugExplainWithIndexOnFilter", "explain/debug/with_index_test.go::TestDebugExplainWithIndexOnOrder", "explain/debug/with_index_test.go::TestDebugExplainWithIndexOnSubqueryNestedRelationOrder"]

With `@index` on the filtered or ordered field the debug plan has no `orderNode`; a child ordered by an indexed related field with `limit` has a `limitNode` but no `orderNode`.

# `go.explain.debug.orphan`: Orphan handling when ordering by a relation

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/with_orphan_test.go::TestDebugExplainRequestWithOrderByRelationFieldWithIndex", "explain/debug/with_orphan_test.go::TestDebugExplainRequestWithOrderByRelationFieldWithIndexDESC", "explain/debug/with_orphan_test.go::TestDebugExplainRequestWithOrderByRelationFieldSecondaryParent"]

With `@exhaustive`, ordering a primary-side parent by a related field (`Publisher(order: {book: {rating: ASC|DESC}})`) explains as `typeIndexJoin > sequenceNode[orphanNode, typeJoinOne]` (orphans first for ASC, last for DESC); ordering from the secondary side (`Book(order: {publisher: {establishedYear: ASC}})`) explains as `typeIndexJoin > orphanNode > typeJoinOne`.

# `go.explain.debug.special_nodes`: Similarity and view plans

- **Parents:** ["go.explain.debug"]
- **Go-Tests:** ["explain/debug/with_similarity_test.go::TestDebugExplainRequestWith_WithSimilarity", "explain/debug/with_view_test.go::TestDebugExplainRequestWithView", "explain/debug/with_view_transform_test.go::TestDebugExplainRequestWithViewWithTransform"]
- **Scope:** "view tests: cacheless view type only"

`SIMILARITY(pointsList: {vector: [...]})` explains as `selectTopNode > similarityNode > selectNode > scanNode`; querying a non-materialized view explains with a `viewNode` over the view's source plan, and a view with a Lens transform adds a `lensNode` between them.

# `go.explain.execute`: Execute explain: run the plan and report counters

- **Parents:** ["go.explain"]

`@explain(type: execute)` runs the operation and returns `{executionSuccess: true, sizeOfResult, planExecutions, operationNode: [...]}` where each node reports counters such as `iterations`, `filterMatches`, `docFetches`, `fieldFetches` and `indexFetches`.

# `go.encryption`: Document and field encryption

- **Parents:** ["go"]

Documents created with `encrypt: true` (whole document) or `encryptFields: [...]` (selected fields) store their commit deltas encrypted, read back decrypted locally, and share keys with authorised peers through the KMS.

# `go.encryption.doc`: Whole-document encryption on one node

- **Parents:** ["go.encryption"]

A document created with `IsDocEncrypted` (`encrypt: true`) is encrypted at rest and readable in plaintext through queries.

# `go.encryption.doc.commits`: Encrypted commit deltas

- **Parents:** ["go.encryption.doc"]
- **Go-Tests:** ["encryption/commit_test.go::TestDocEncryption_WithEncryptionOnLWWCRDT_ShouldStoreCommitsDeltaEncrypted", "encryption/commit_test.go::TestDocEncryption_UponUpdateOnLWWCRDT_ShouldEncryptCommitDelta", "encryption/commit_test.go::TestDocEncryption_WithMultipleDocsUponUpdate_ShouldEncryptOnlyRelevantDocs", "encryption/commit_test.go::TestDocEncryption_WithEncryptionOnCounterCRDT_ShouldStoreCommitsDeltaEncrypted", "encryption/commit_test.go::TestDocEncryption_UponUpdateOnCounterCRDT_ShouldEncryptedCommitDelta", "encryption/commit_test.go::TestDocEncryption_UponEncryptionSeveralDocs_ShouldStoreAllCommitsDeltaEncrypted", "encryption/commit_test.go::TestDocEncryption_IfTwoDocsHaveSameFieldValue_CipherTextShouldBeDifferent", "encryption/commit_relation_test.go::TestDocEncryption_WithEncryptionSecondaryRelations_ShouldStoreEncryptedCommit"]

`_commits { delta }` for an encrypted document returns each field delta encrypted with a key derived per document (LWW and `pcounter` fields, on create and on update); the composite `_C` delta is nil; relation ID deltas are not plaintext; unencrypted documents in the same collection keep plaintext deltas; batch-created docs each use their own key, so two docs with the same value have different ciphertext.

# `go.encryption.doc.query`: Decrypted reads

- **Parents:** ["go.encryption.doc"]
- **Go-Tests:** ["encryption/query_test.go::TestDocEncryption_WithEncryption_ShouldFetchDecrypted", "encryption/query_test.go::TestDocEncryption_WithEncryptionOnCounterCRDT_ShouldFetchDecrypted", "encryption/query_relation_test.go::TestDocEncryption_WithEncryptionOnBothRelations_ShouldFetchDecrypted", "encryption/query_relation_test.go::TestDocEncryption_WithEncryptionOnPrimaryRelations_ShouldFetchDecrypted", "encryption/query_relation_test.go::TestDocEncryption_WithEncryptionOnSecondaryRelations_ShouldFetchDecrypted"]

Querying an encrypted document returns its plaintext fields, including a `pcounter` field that sums correctly after an update (5 then +3 gives 8), and related documents resolve when either, both or only the primary or secondary side are encrypted.

# `go.encryption.field`: Field-level encryption

- **Parents:** ["go.encryption"]

`encryptFields: [...]` encrypts only the named fields.

# `go.encryption.field.storage`: Per-field keys and decrypted reads

- **Parents:** ["go.encryption.field"]
- **Go-Tests:** ["encryption/field_commit_test.go::TestDocEncryptionField_WithEncryptionOnField_ShouldStoreOnlyFieldsDeltaEncrypted", "encryption/field_commit_test.go::TestDocEncryptionField_WithDocAndFieldEncryption_ShouldUseDedicatedEncKeyForIndividualFields", "encryption/field_commit_test.go::TestDocEncryptionField_UponUpdateWithDocAndFieldEncryption_ShouldUseDedicatedEncKeyForIndividualFields", "encryption/field_query_test.go::TestDocEncryptionField_WithEncryption_ShouldFetchDecrypted"]

With `encryptFields: ["age"]` only the `age` delta is encrypted while `name` stays plaintext; combining doc and field encryption gives each listed field its own key distinct from the document key, on create and on update; querying returns all fields decrypted.

# `go.encryption.field.errors`: Invalid encrypted field names

- **Parents:** ["go.encryption.field"]
- **Go-Tests:** ["encryption/field_test.go::TestDocEncryptionField_IfFieldDoesNotExistInGQLSchema_ReturnError", "encryption/field_test.go::TestDocEncryptionField_IfAttemptToEncryptBuiltinFieldInGQLSchema_ReturnError", "encryption/field_test.go::TestDocEncryptionField_IfFieldDoesNotExist_ReturnError", "encryption/field_test.go::TestDocEncryptionField_IfAttemptToEncryptBuiltinField_ReturnError"]
- **Scope:** "GQL-error tests: GQL request mutation type only; API-error tests: collection save and named mutation types only"

Through GraphQL, `encryptFields` naming an unknown field or `_docID`/`_version` fails with `Argument "encryptFields" has invalid value [<field>].`; through the collection API, an unknown field fails with `the given field does not exist. Name: points`, `_docID` with `can not encrypt build-in field` and `_version` with the field-not-exist error.

# `go.encryption.peer`: Encrypted documents over P2P

- **Parents:** ["go.encryption"]

Peers subscribed to a collection receive encrypted DAGs and fetch the keys from the KMS to decrypt them.

# `go.encryption.peer.key_fetch`: Key fetch and decryption on the receiving peer

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_test.go::TestDocEncryptionPeer_UponSync_ShouldSyncEncryptedDAG", "encryption/peer_test.go::TestDocEncryptionPeer_IfPeerDidNotReceiveKey_ShouldNotFetch", "encryption/peer_multipeer_test.go::TestDocEncryptionPeer_MultiplePeersOnEncryptionTopic_ShouldFetchKeyFromHolder", "encryption/peer_subscription_test.go::TestDocEncryptionPeer_WithSubscription_ShouldDeliverDecryptedDoc", "encryption/peer_share_test.go::TestDocEncryptionPeer_IfDocIsPublic_ShouldFetchKeyAndDecrypt", "encryption/peer_share_test.go::TestDocEncryptionPeer_IfPublicDocHasEncryptedField_ShouldFetchKeyAndDecrypt", "encryption/peer_share_test.go::TestDocEncryptionPeer_IfEncryptedPublicDocHasEncryptedField_ShouldFetchKeysAndDecrypt", "encryption/peer_share_test.go::TestDocEncryptionPeer_IfAllFieldsOfEncryptedPublicDocAreIndividuallyEncrypted_ShouldFetchKeysAndDecrypt", "encryption/peer_share_test.go::TestDocEncryptionPeer_IfAllFieldsOfPublicDocAreIndividuallyEncrypted_ShouldFetchKeysAndDecrypt"]

After sync the receiving node stores the same encrypted deltas and, once the key arrives, returns the plaintext document for public docs that are doc-encrypted, field-encrypted or both; before the key arrives the query returns an empty set; the key is fetched from the holder even when other KMS peers on the topic answer empty; a GraphQL subscription on the receiver delivers the decrypted document.

# `go.encryption.peer.merge`: Merging updates to encrypted documents

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_share_test.go::TestDocEncryptionPeer_WithUpdatesOnEncryptedDeltaBasedCRDTField_ShouldDecryptAndCorrectlyMerge", "encryption/peer_share_test.go::TestDocEncryptionPeer_WithUpdatesOnDeltaBasedCRDTFieldOfEncryptedDoc_ShouldDecryptAndCorrectlyMerge", "encryption/peer_share_test.go::TestDocEncryptionPeer_WithUpdatesThatSetsEmptyString_ShouldDecryptAndCorrectlyMerge", "encryption/peer_share_test.go::TestDocEncryptionPeer_WithUpdatesThatSetsStringToNull_ShouldDecryptAndCorrectlyMerge"]

Updates made on the source to an encrypted `pcounter` field, or a `pcounter` field of an encrypted document, decrypt and merge on the receiver to the summed value; updates setting a string to `""` or `null` and back to `"John"` replicate in order.

# `go.encryption.peer.index`: Indexing replicated encrypted documents

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_sec_index_test.go::TestDocEncryptionPeer_IfEncryptedDocHasIndexedField_ShouldIndexAfterDecryption", "encryption/peer_sec_index_test.go::TestDocEncryptionPeer_IfDocDocHasEncryptedIndexedField_ShouldIndexAfterDecryption"]

On the receiving node, a secondary index on `age` covers replicated documents whose doc or `age` field is encrypted once they are decrypted: `User(filter: {age: {_eq: 21}})` uses 2 index fetches and returns both matching docs.

# `go.encryption.peer.acp`: Key sharing under document ACP

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_acp_test.go::TestDocEncryptionACP_IfUserAndNodeHaveAccess_ShouldFetch", "encryption/peer_acp_test.go::TestDocEncryptionACP_IfUserHasAccessButNotNode_ShouldNotFetch", "encryption/peer_acp_test.go::TestDocEncryptionACP_IfNodeHasAccessToSomeDocs_ShouldFetchOnlyThem", "encryption/peer_acp_test.go::TestDocEncryptionACP_IfClientNodeHasDocPermissionButServerNodeIsNotAvailable_ShouldNotFetch", "encryption/peer_acp_branchable_test.go::TestDocEncryptionACP_BranchableCollection_AuthorizedPeerCanFetch", "encryption/peer_acp_branchable_test.go::TestDocEncryptionACP_BranchableCollectionWithRemoteDAC_AuthorizedPeerCanFetch"]
- **Scope:** "document ACP: remote DAC only, except the branchable local-DAC test which runs only with local DAC"

Keys and blocks for a private encrypted document reach a peer only when that peer's node identity has access: user and node with access read the doc; a user with access through a node without it reads nothing and the node has no commits; a node granted `reader` on some docs sees exactly the shared private docs plus public docs (encrypted or not); if the key-holding node is closed, a newly authorised peer gets nothing; on a private `@branchable` collection the collection owner reads the decrypted doc on the peer.

# `go.encryption.peer.nac`: Branchable sync of encrypted docs under node access control

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_nac_test.go::TestDocEncryptionNAC_SyncBranchableCollection_AuthorizedIdentity_AllowAccess", "encryption/peer_nac_test.go::TestDocEncryptionNAC_SyncBranchableCollection_UnauthorizedIdentity_DenyAccess", "encryption/peer_nac_test.go::TestDocEncryptionNAC_SyncBranchableCollection_NoIdentity_DenyAccess", "encryption/peer_nac_test.go::TestDocEncryptionNAC_SyncBranchableCollection_GrantedRelation_AllowAccess", "encryption/peer_nac_test.go::TestDocEncryptionNAC_SyncBranchableCollection_RevokedRelation_DenyAccess", "encryption/peer_nac_test.go::TestDocEncryptionNAC_GossipSync_AuthorizedNodeIdentity_AllowAccess"]
- **Scope:** "client types: go, http"

With NAC enabled on the source, `SyncBranchableCollection` for an encrypted doc succeeds for an authorised identity, fails with `not authorized to perform operation` for an unauthorised or missing identity, succeeds after an admin relation is granted, fails again after it is revoked; gossip sync works when the subscriber's node identity is authorised.

# `go.encryption.peer.diverged_versions`: KMS authorisation across diverged branchable versions

- **Parents:** ["go.encryption.peer"]
- **Go-Tests:** ["encryption/peer_branchable_diverged_versions_test.go::TestDocEncryptionBranchableSync_DivergedVersions_FailsKMSAuth"]
- **Go-Bug-Suspect:** "test is skipped ('t.Skip') for Go issue #4789: KMS authorisation needs the doc's collection version locally, which only the sync being authorised delivers, so Go deadlocks."

Two peers whose `@branchable` collection versions have diverged each create an encrypted doc and sync bidirectionally; each must obtain the other's key and read both docs.

# `go.searchable_encryption`: Searchable encryption (encrypted equality indexes)

- **Parents:** ["go"]

Fields marked `@encryptedIndex` (or given an encrypted index later) get equality indexes that replicators maintain, queried through generated `encrypted_<Collection>` GraphQL fields that return matching docIDs.

# `go.searchable_encryption.index`: Encrypted index management

- **Parents:** ["go.searchable_encryption"]

Creating, listing, deleting and patching encrypted indexes.

# `go.searchable_encryption.index.create`: Creating encrypted indexes

- **Parents:** ["go.searchable_encryption.index"]
- **Go-Tests:** ["searchable_encryption/add_test.go::TestEncryptedIndexNew_SchemaWithEncryptedIndex_ShouldNotHinderQuerying", "searchable_encryption/add_test.go::TestEncryptedIndexNew_AfterAddRequest_ShouldNotHinderQuerying", "searchable_encryption/add_test.go::TestEncryptedIndexNew_IfNonExistentFieldIsGiven_ReturnError", "searchable_encryption/add_test.go::TestEncryptedIndexNew_IfIndexAlreadyExists_ShouldReturnError"]

`@encryptedIndex` in SDL, or adding an encrypted index on `age` after documents exist, leaves normal `User { name age }` queries unchanged; creating one on a non-existent field fails with the encrypted-index-on-non-existent-field error naming the field; creating a second one on the same field fails with `encrypted index already exists on this field`.

# `go.searchable_encryption.index.list_delete`: Listing and deleting encrypted indexes

- **Parents:** ["go.searchable_encryption.index"]
- **Go-Tests:** ["searchable_encryption/list_test.go::TestEncryptedIndexList_ShouldReturnListOfExistingIndexes", "searchable_encryption/list_test.go::TestEncryptedIndexList_IfIndexAddedLater_ShouldReturnListOfExistingIndexes", "searchable_encryption/list_test.go::TestEncryptedIndexList_WhenRequestingAllIndexes_ShouldReturn", "searchable_encryption/delete_test.go::TestEncryptedIndexDelete_WithExistingIndex_ShouldDeleteSuccessfully", "searchable_encryption/delete_test.go::TestEncryptedIndexDelete_IfIndexDoesNotExist_ReturnError", "searchable_encryption/delete_test.go::TestEncryptedIndexDelete_AfterDelete_CanMakeNewIndexAnew", "searchable_encryption/delete_test.go::TestEncryptedIndexDelete_MultipleIndexes_ShouldOnlyDeleteSpecified"]

Listing per collection returns `{FieldName, Type: equality}` for each encrypted index, including ones added later; listing all returns them keyed by collection; deleting removes only the named index, after which it can be created again; deleting a missing one fails with `encrypted index does not exist on this field`.

# `go.searchable_encryption.index.patch`: Encrypted indexes are immutable via collection patch

- **Parents:** ["go.searchable_encryption.index"]
- **Go-Tests:** ["searchable_encryption/patch_test.go::TestPatchCollection_NewEncryptedIndex_ShouldError", "searchable_encryption/patch_test.go::TestPatchCollection_RemoveEncryptedIndex_ShouldError", "searchable_encryption/patch_test.go::TestPatchCollection_ModifyEncryptedIndex_ShouldError"]

A collection patch that adds, removes or modifies `/Users/EncryptedIndexes` fails with `collection encrypted indexes cannot be mutated`.

# `go.searchable_encryption.query`: Encrypted queries

- **Parents:** ["go.searchable_encryption"]

Querying encrypted indexes through `encrypted_<Collection>`.

# `go.searchable_encryption.query.schema`: Generation of the encrypted query field

- **Parents:** ["go.searchable_encryption.query"]
- **Go-Tests:** ["searchable_encryption/query_test.go::TestEncryptedIndexNew_IfP2PIsDisabled_CanNotDoSEQuery", "searchable_encryption/peer_add_test.go::TestEncryptedIndexNewPeer_SchemaWithEncryptedIndex_ShouldGenerateGQL", "searchable_encryption/peer_add_test.go::TestEncryptedIndexNewPeer_AfterAddRequest_ShouldGenerateGQL", "searchable_encryption/peer_delete_test.go::TestDocEncryptionPeer_AfterDeletingIndex_SEQueryShouldReturnError", "searchable_encryption/peer_query_test.go::TestDocEncryption_IfThereIsNoIndex_EncryptedQueryShouldError", "searchable_encryption/peer_query_test.go::TestDocEncryption_IfThereIsIndexButOnAnotherField_EncryptedQueryShouldError"]

`encrypted_User(filter: ...) { docIDs }` exists only when P2P is enabled and the collection has an encrypted index (from SDL or added later); without P2P, without any index, or after the index is deleted, the query fails with `Cannot query field "encrypted_User" on type "Query".`; filtering on a field without an encrypted index fails with `Argument "filter" has invalid value`.

# `go.searchable_encryption.query.results`: Encrypted query results

- **Parents:** ["go.searchable_encryption.query"]
- **Go-Tests:** ["searchable_encryption/peer_query_test.go::TestDocEncryptionPeer_WithSimpleRequest_ShouldFetchSuccessfully", "searchable_encryption/peer_query_test.go::TestDocEncryptionPeer_WithMultipleEncryptedFields_QueryShouldSucceed", "searchable_encryption/peer_query_test.go::TestDocEncryptionPeer_WithMultipleDocs_ShouldFilterCorrectly", "searchable_encryption/peer_query_test.go::TestDocEncryptionPeer_WithQueryOnMultipleFields_ShouldReturnIntersection"]

After an encrypted doc is replicated and SE sync completes, `encrypted_User(filter: {age: {_eq: 21}}) { docIDs }` on the source returns the matching docIDs; each of several encrypted fields can be queried; with several docs only matching ones are returned; filtering on two fields returns the intersection.

# `go.searchable_encryption.replication`: Encrypted index replication lifecycle

- **Parents:** ["go.searchable_encryption"]
- **Go-Tests:** ["searchable_encryption/replicator_test.go::TestSEReplicator_IfDocAddedWhileReplicatorIsOffline_ShouldRetry", "searchable_encryption/truncate_test.go::TestTruncateRemovesSearchableEncryptionArtifacts", "searchable_encryption/peer_acp_test.go::TestDocEncryptionPeer_WithACP_ReplicatorShouldNotHaveAccess"]
- **Scope:** "replicator restart test: file-backed database only; ACP test: local DAC only"
- **Go-Bug-Suspect:** "peer_acp test asserts the replicator has no commits for the doc, with a comment saying this should change to encrypted-delta commits once a dedicated replication permission exists."

Docs added while the replicator is offline are indexed after it restarts and are then found by `encrypted_User`; truncating the doc on the replicator empties its `encrypted_User` result; a replicator without ACP access to a private doc still answers the source's encrypted query but holds neither the doc nor its commits.

# `go.signature`: Block signing

- **Parents:** ["go"]

With signing enabled, the node signs the blocks it writes; `_commits { signature { type identity value } }` exposes signatures, which can be verified against an identity.

# `go.signature.commits`: Which commits carry signatures

- **Parents:** ["go.signature"]

Signature data returned by `_commits` for created, updated and deleted docs.

# `go.signature.commits.create`: Signatures on created documents

- **Parents:** ["go.signature.commits"]
- **Go-Tests:** ["signature/commit_test.go::TestSignature_WithCommitQuery_ShouldIncludeSignatureData", "signature/commit_test.go::TestSignature_WithCommitQuery_ShouldBeHexEncoded", "signature/commit_test.go::TestSignature_WithEd25519KeyType_ShouldIncludeSignatureData", "signature/branchable_test.go::TestSignature_WithBranchableCollection_ShouldSignCollectionBlocks"]
- **Scope:** "branchable test: client types go, cli, http, js (not the C client)"

Every field block and the `_C` composite block of a new doc carry a signature whose `type` matches the node key (`ES256K` for secp256k1, `EdDSA` for ed25519), whose `identity` is the node's compressed public key as a hex string (e.g. `02ae92ce...`) and whose `value` verifies over the block; on a `@branchable` collection the collection block (`fieldName: nil`) is signed too.

# `go.signature.commits.update_delete`: Signatures on updates and deletes

- **Parents:** ["go.signature.commits"]
- **Go-Tests:** ["signature/commit_test.go::TestSignature_WithUpdatedDocsAndCommitQuery_ShouldSignOnlyFirstFieldBlocks", "signature/commit_test.go::TestSignature_WithDeletedDocAndCommitQuery_ShouldIncludeSignatureData"]

After two updates, field blocks at heights 2 and 3 have `signature: nil` while every `_C` block and the height-1 field block are signed with the same identity and distinct values; deleting a doc adds a signed `_C` block at height 2 with the node identity.

# `go.signature.signer`: Choice of signer

- **Parents:** ["go.signature"]
- **Go-Tests:** ["signature/commit_test.go::TestSignature_WithPerOpSigningDisabled_ShouldNotSignAnyCommit", "signature/commit_test.go::TestSignature_WithPerOpSigningOverrideOnUpdate", "signature/commit_test.go::TestSignature_WithClientIdentity_ShouldUseItForSigning", "signature/fallback_signer_test.go::TestSignature_IfIdentityHasNoPrivateKey_ShouldUseNodeIdentity"]
- **Scope:** "per-op override: collection save/named mutation types only (save only on update); fallback signer: http and cli clients only"
- **Go-Bug-Suspect:** "'TestSignature_WithClientIdentity_ShouldUseItForSigning' is skipped (TODO, Go issue #3573): Go signs with the node identity even when the client identity is supplied."

Which identity signs, or whether anything is signed: an add with signing disabled per operation leaves every commit unsigned; disabling it on one update leaves that `_C` block unsigned while re-enabling it on the next signs again; a client identity without a private key (HTTP/CLI) falls back to the node identity; a client identity with a key should sign with that key.

# `go.signature.verify`: Signature verification

- **Parents:** ["go.signature"]

Verifying a block's signature against an expected identity.

# `go.signature.verify.basic`: Verifying with identity and CID

- **Parents:** ["go.signature.verify"]
- **Go-Tests:** ["signature/verify_test.go::TestSignatureVerify_WithValidData_ShouldVerify", "signature/verify_test.go::TestSignatureVerify_WithDifferentKeyType_ShouldVerify", "signature/verify_test.go::TestSignatureVerify_WithWrongIdentity_ShouldError", "signature/verify_test.go::TestSignatureVerify_WithWrongCid_ShouldError"]

Verifying the blocks of a create, update and delete against the node identity succeeds (secp256k1 and ed25519 keys); verifying against another node's identity fails with `signature was created by a different key`; verifying an unknown CID fails with an error containing `could not find`.

# `go.signature.verify.acp`: Verification gated by access control

- **Parents:** ["go.signature.verify"]
- **Go-Tests:** ["signature/acp_test.go::TestSignatureACP_IfHasNoAccessToDoc_ShouldError", "signature/acp_test.go::TestSignatureACP_IfHasAccessToDoc_ValidateSignature", "signature/acp_branchable_test.go::TestSignatureACP_WithBranchableCollection_IfHasNoCollectionAccess_ShouldError", "signature/acp_branchable_test.go::TestSignatureACP_WithBranchableCollection_IfHasCollectionAccess_ValidateSignature"]
- **Scope:** "document tests: go client only; branchable tests: go and C clients only"

On a DAC-protected collection, verifying a doc's block as an identity without access fails with `missing permission`, while the owner verifies it; on a private `@branchable` collection the collection-level block (no docID) is verifiable only by an identity with read access to the collection object.

# `go.signature.transparency`: Signing does not change query results

- **Parents:** ["go.signature"]
- **Go-Tests:** ["signature/query_test.go::TestDocSignature_WithEnabledSigning_ShouldQuery", "signature/query_test.go::TestDocSignature_WithEnabledSigning_ShouldQueryCommitsWithoutSignature"]

With signing enabled, `Users { _docID name age }` returns the created document and its field values, and `_version { height ... }` reads normally. The docID is the one the signed create produced: the signature is part of the CID the docID derives from, so it differs from the unsigned docID (the tests match it symbolically with `NewDocIndex`).

# `go.signature.peer`: Signed blocks across peers

- **Parents:** ["go.signature"]
- **Go-Tests:** ["signature/peer_test.go::TestDocSignature_WithPeersAndSecp256k1KeyType_ShouldSync", "signature/peer_test.go::TestDocSignature_WithPeersAndEd25519KeyType_ShouldSync", "signature/peer_test.go::TestDocSignature_WithPeersAnDifferentKeyTypes_ShouldSync", "signature/peer_test.go::TestDocSignature_WithPeersAnDifferentKeyTypesUpdatingSameDoc_ShouldSync"]

Docs created on one node replicate to a peer when both use secp256k1 or both ed25519 keys, or when they use different key types; after updates from both peers on the same doc, each `_C` block carries the key type of the node that wrote it.

# `go.backup`: Backup export and import

- **Parents:** ["go"]

`ExportBackup` writes collections' documents to a JSON file and `ImportBackup` recreates them from such a file.

# `go.backup.export`: Export

- **Parents:** ["go.backup"]

Writing documents to a backup file.

# `go.backup.export.content`: Export file content

- **Parents:** ["go.backup.export"]
- **Go-Tests:** ["backup/simple/export_test.go::TestBackupExport_Simple_NoError", "backup/simple/export_test.go::TestBackupExport_Empty_NoError", "backup/one_to_many/export_test.go::TestBackupExport_AllCollectionsMultipleDocsAndDocUpdate_NoError", "backup/one_to_many/export_test.go::TestBackupExport_AllCollectionsMultipleDocsAndMultipleDocUpdate_NoError", "backup/one_to_one/export_test.go::TestBackupExport_AllCollectionsMultipleDocsAndDocUpdate_NoError", "backup/one_to_one/export_test.go::TestBackupExport_DoubleReletionship_NoError", "backup/one_to_one/export_test.go::TestBackupExport_DoubleReletionshipWithUpdate_NoError", "backup/self_reference/export_test.go::TestBackupExport_MultipleDocsAndDocUpdate_NoError"]

The export is a JSON object keyed by collection name (sorted), each holding documents with `_docID`, `_docIDNew` (equal to `_docID`, also after updates) and their current field values in key order, relation foreign keys as `_<field>ID`; an empty doc exports only its ID keys; self-references, double relations and updates are exported with the latest values.

# `go.backup.export.collections`: Exporting selected collections

- **Parents:** ["go.backup.export"]
- **Go-Tests:** ["backup/simple/export_test.go::TestBackupExport_JustUserCollection_NoError", "backup/one_to_many/export_test.go::TestBackupExport_JustUserCollection_NoError", "backup/one_to_one/export_test.go::TestBackupExport_JustUserCollection_NoError", "backup/self_reference/export_test.go::TestBackupExport_Simple_NoError"]

With `Collections: ["User"]` the export holds only the `User` key and its documents, even when related collections have documents.

# `go.backup.export.errors`: Export errors

- **Parents:** ["go.backup.export"]
- **Go-Tests:** ["backup/simple/export_test.go::TestBackupExport_WithInvalidFilePath_ReturnError", "backup/simple/export_test.go::TestBackupExport_WithInvalidCollection_ReturnError"]

Exporting to a path whose parent directory does not exist fails with `failed to create file`; naming an unknown collection fails with `failed to get collection: collection not found. Name: Invalid`.

# `go.backup.import`: Import

- **Parents:** ["go.backup"]

Recreating documents from a backup file.

# `go.backup.import.documents`: Importing documents

- **Parents:** ["go.backup.import"]
- **Go-Tests:** ["backup/simple/import_test.go::TestBackupImport_Simple_NoError", "backup/simple/import_test.go::TestBackupImport_WithNoKeys_NoError", "backup/simple/import_test.go::TestBackupImport_WithMultipleNoKeys_NoError", "backup/simple/import_test.go::TestBackupImport_EmptyObject_NoError", "backup/one_to_many/import_test.go::TestBackupImport_WithMultipleNoKeyAndMultipleCollections_NoError", "backup/one_to_one/import_test.go::TestBackupImport_WithMultipleNoKeyAndMultipleCollections_NoError"]

Importing documents, with or without `_docID` keys, into one or several collections makes them queryable with the given values; an empty object imports a doc whose fields are null.

# `go.backup.import.relations`: Importing related documents

- **Parents:** ["go.backup.import"]
- **Go-Tests:** ["backup/one_to_many/import_test.go::TestBackupImport_WithMultipleNoKeyAndMultipleCollectionsAndUpdatedDocs_NoError", "backup/one_to_one/import_test.go::TestBackupImport_WithMultipleNoKeyAndMultipleCollectionsAndUpdatedDocs_NoError", "backup/one_to_one/import_test.go::TestBackupImport_DoubleRelationshipWithUpdate_NoError", "backup/self_reference/import_test.go::TestBackupSelfRefImport_Simple_NoError", "backup/self_reference/import_test.go::TestBackupSelfRefImport_SelfRef_NoError", "backup/self_reference/import_test.go::TestBackupSelfRefImport_PrimaryRelationWithSecondCollection_NoError", "backup/self_reference/import_test.go::TestBackupSelfRefImport_PrimaryRelationWithSecondCollectionWrongOrder_NoError", "backup/self_reference/import_test.go::TestBackupSelfRefImport_SplitPrimaryRelationWithSecondCollection_NoError"]
- **Go-Bug-Suspect:** "split-primary self-reference test has its 'ExportBackup' step commented out because the export is wrong (Go issue #1704); only the import of a hand-written file is checked."

Relation foreign keys in the file that name an old `_docID` are remapped to its `_docIDNew`, so one-to-many, one-to-one, double, self-referencing and split primary relations resolve after import, whatever order collections appear in, including an export from one node imported into another.

# `go.backup.import.errors`: Import errors

- **Parents:** ["go.backup.import"]
- **Go-Tests:** ["backup/simple/import_test.go::TestBackupImport_WithInvalidFilePath_ReturnError", "backup/simple/import_test.go::TestBackupImport_WithInvalidCollection_ReturnError", "backup/simple/import_test.go::TestBackupImport_WithDocAlreadyExists_ReturnError", "backup/simple/import_test.go::TestBackupImport_WithAliasCollidingExistingDoc_ReturnError", "backup/simple/import_test.go::TestBackupImport_WithMultipleNoKeysAndInvalidField_Errors", "backup/one_to_one/import_test.go::TestBackupImport_WithMultipleNoKeyAndMultipleCollectionsAndMultipleUpdatedDocs_NoError"]

Importing from a missing path fails with `failed to open file`; an unknown collection with `failed to get collection: collection not found. Name: Invalid`; a doc whose ID, or whose alias `_docID`, matches an existing doc with `a document with the given ID already exists`; an unknown field with `the given field does not exist. Name: INVALID`, committing none of the file's docs; two one-to-one docs pointing at the same author with `can not index a doc's field(s) that violates unique index`.
