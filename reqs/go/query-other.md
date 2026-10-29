# `go.query.commits`: Commit history queries

- **Parents:** ["go.query"]

The `_commits` GraphQL query exposes the Merkle-CRDT block history (field, composite `_C` and collection-level blocks) of documents.

# `go.query.commits.branchable`: Branchable collection commits

- **Parents:** ["go.query.commits"]

Collections declared `@branchable` get collection-level commits in addition to document field and composite commits.

# `go.query.commits.branchable.collection-commits`: Collection-level commit graph

- **Parents:** ["go.query.commits.branchable"]
- **Go-Tests:** ["query/commits/branchables/simple_test.go::TestQueryCommitsBranchables", "query/commits/branchables/simple_test.go::TestQueryCommitsBranchables_WithAllFields", "query/commits/branchables/add_test.go::TestQueryCommitsBranchables_WithMultipleAdd", "query/commits/branchables/update_test.go::TestQueryCommitsBranchables_WithDocUpdate", "query/commits/branchables/delete_test.go::TestQueryCommitsBranchables_WithDelete", "query/commits/branchables/if_test.go::TestQueryCommitsBranchables_WithIfDirectiveTrue", "query/commits/branchables/if_test.go::TestQueryCommitsBranchables_WithIfDirectiveFalse"]
- **Scope:** "TestQueryCommitsBranchables_WithAllFields excludes the signed-docs and encrypted-docs multipliers (it asserts plaintext delta bytes)"

For a `type Users @branchable` (or `@branchable(if: true)`) collection, every create, update and delete adds one collection-level commit with `docID: null`, `fieldName: null`, height 1 for the first, whose `links` point to the document's composite (`_C`) commit and whose `heads` point to the previous collection commit; one doc create yields 4 `_commits` (name, age, `_C`, collection). With `@branchable(if: false)` no collection-level commit exists and a create yields 3 commits.

# `go.query.commits.branchable.query-args`: Looking up branchable collection commits

- **Parents:** ["go.query.commits.branchable"]
- **Go-Tests:** ["query/commits/branchables/cid_test.go::TestQueryCommitsBranchables_WithCidParam", "query/commits/branchables/cid_doc_id_test.go::TestQueryCommitsBranchables_WithCidAndDocIDParam", "query/commits/branchables/field_id_test.go::TestQueryCommitsBranchables_WithFieldNameFilter"]
- **Scope:** "TestQueryCommitsBranchables_WithCidParam excludes the signed-docs and encrypted-docs multipliers"
- **Go-Bug-Suspect:** "TestQueryCommitsBranchables_WithCidAndDocIDParam asserts that '_commits(docID: {doc}, cid: {collection cid})' fails with 'cid either does not exist or belong to document'; the test comment says it would be nice if this worked and links Go issue #3213"

`_commits(cid: <collection commit cid>)` returns that single commit with `docID` and `fieldName` null; `_commits(filter: {fieldName: {_eq: null}})` returns only the collection-level commit (with its `collectionVersionId`). Combining a document's `docID` with a collection-level `cid` errors with `cid either does not exist or belong to document`.

# `go.query.commits.branchable.sync`: Branchable commits replicate over P2P

- **Parents:** ["go.query.commits.branchable"]
- **Go-Tests:** ["query/commits/branchables/peer_test.go::TestQueryCommitsBranchables_SyncsAcrossPeerConnection", "query/commits/branchables/peer_test.go::TestQueryCommitsBranchables_SyncsMultipleAcrossPeerConnection", "query/commits/branchables/peer_update_test.go::TestQueryCommitsBranchables_HandlesConcurrentUpdatesAcrossPeerConnection", "query/commits/branchables/peer_index_test.go::TestQueryCommitsBranchables_SyncsIndexAcrossPeerConnection"]
- **Scope:** "two-node P2P tests (connected peers with collection subscription); TestQueryCommitsBranchables_HandlesConcurrentUpdatesAcrossPeerConnection excludes the signed-docs and encrypted-docs multipliers"

When a node subscribes to a branchable collection, docs created on the peer arrive with their full commit graph, including collection-level commits, so `_commits` returns an identical graph on both nodes (two creates: 8 commits, 4 leaves, 2 composites, a collection root and one collection head). Concurrent offline updates on both nodes converge after sync to the same `_commits` graph and the same latest value (`Users { name }` returns "Addo"), and a synced doc is found through its secondary index (`Users(filter: {name: {_eq: "John"}})`).

# `go.query.commits.listing`: Listing commits and their fields

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/simple_test.go::TestQueryCommits", "query/commits/simple_test.go::TestQueryCommitsMultipleDocs", "query/commits/simple_test.go::TestQueryCommitsWithCollectionVersionIDField", "query/commits/simple_test.go::TestQueryCommitsWithFieldNameField", "query/commits/simple_test.go::TestQueryCommitsWithFieldNameFieldAndUpdate", "query/commits/simple_test.go::TestQuery_CommitsWithAllFieldsWithUpdate_NoError", "query/commits/simple_test.go::TestQueryCommits_WithAlias_Succeeds", "query/commits/with_doc_id_prop_test.go::TestQueryCommitsWithDocIDProperty", "query/commits/with_doc_id_typename_test.go::TestQueryCommitsWithDocIDWithTypeName", "query/commits/with_delete_test.go::TestQueryCommits_AfterDocDeletion_ShouldStillFetch"]
- **Scope:** "TestQuery_CommitsWithAllFieldsWithUpdate_NoError excludes the signed-docs and encrypted-docs multipliers"

Unfiltered `_commits` on a non-branchable collection returns one commit per field plus one composite (`fieldName: "_C"`) per create (3 for one doc, 6 for two) and one more field and composite commit per update. Each commit exposes `cid`, `collectionVersionId`, `delta`, `docID`, `fieldName`, `height`, `links`, `heads`, `signature` (null when unsigned) and `__typename: "Commit"`; the query can be aliased (`history: _commits`). After the document is deleted, its commits, including the delete composite at height 2 heading to the create composite, are still returned.

# `go.query.commits.links`: Commit links and heads

- **Parents:** ["go.query.commits"]

Commits expose `links` (child blocks) and `heads` (previous versions of the same block).

# `go.query.commits.links.structure`: Link and head structure

- **Parents:** ["go.query.commits.links"]
- **Go-Tests:** ["query/commits/with_doc_id_test.go::TestQueryCommitsWithDocIDAndLinks", "query/commits/with_doc_id_test.go::TestQueryCommitsWithDocIDAndUpdateAndLinks", "query/commits/with_nested_links_test.go::TestQueryCommits_WithSingleAddNestedLinks_Succeed", "query/commits/with_nested_links_test.go::TestQueryCommits_WithSingleAddNestedLinksCompositeFilter_Succeed", "query/commits/with_nested_links_test.go::TestQueryCommits_WithSingleAddNestedLinksNestedFilter_Succeed", "query/commits/with_nested_links_test.go::TestQueryCommits_WithSingleUpdateDoubleNestedLinks_Succeeds"]
- **Scope:** "TestQueryCommits_WithSingleUpdateDoubleNestedLinks_Succeeds excludes the signed-docs and encrypted-docs multipliers"
- **Go-Bug-Suspect:** "TestQueryCommitsWithDocIDAndUpdateAndLinks is marked in Go as 'for documentation reasons only. This is not desired behaviour' (the first result includes the head link, the second the field links)"

A composite `_C` commit links to the field commits it changed (create: `age` and `name` at height 1; update: only `age`) and heads to the previous composite; field commits have empty `links` and head to their previous version. Nested selections (`links { heads { links } }`) resolve recursively with `cid`, `height`, `fieldName` and `docID`, and `links(filter: {fieldName: {_eq: "age"}})` filters the nested list.

# `go.query.commits.links.count`: Counting links and heads

- **Parents:** ["go.query.commits.links"]
- **Go-Tests:** ["query/commits/with_doc_id_count_test.go::TestQueryCommitsWithDocIDAndLinkCount", "query/commits/with_doc_id_count_test.go::TestQueryCommits_WithDocUpdatesAndLinkHeadCount"]

`COUNT(field: links)` and `COUNT(field: heads)` inside `_commits(docID: ...)` return per-commit counts: after one create the composite has 2 links and field commits 0; after an update the new composite has 1 link and 1 head, the new `age` commit 0 links and 1 head.

# `go.query.commits.docid`: Commits by document ID

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_doc_id_test.go::TestQueryCommitsWithUnknownDocID", "query/commits/with_doc_id_test.go::TestQueryCommitsWithDocID", "query/commits/with_doc_id_test.go::TestQueryCommitsWithDocIDAndUpdate", "query/commits/with_doc_id_test.go::TestQueryCommits_DocIDEmptyList", "query/commits/with_doc_id_test.go::TestQueryCommits_DocIDListOfOne", "query/commits/with_doc_id_test.go::TestQueryCommits_DocIDListOfMany", "query/commits/with_doc_id_shared_field_test.go::TestQueryCommitsWithDocID_SharedFieldBlockAcrossDocuments"]
- **Go-Bug-Suspect:** "TestQueryCommits_DocIDListOfMany asserts '_commits(docID: [a, b])' errors with 'querying by multiple docIDs is not yet supported' although the argument accepts a list; a gap, not a design choice"

`_commits(docID: X)` returns only that document's commits (3 after create, 5 after one update with heights 2,1,1,2,1); an unknown docID or `docID: []` returns an empty list and `docID: [X]` behaves like `docID: X`. Two docs with an identical field value share a byte-identical field block, and each doc's `_commits(docID:)` still returns its own `name`, `age` and `_C` commits attributed to that docID. `docID` with two or more entries errors with `querying by multiple docIDs is not yet supported`.

# `go.query.commits.cid`: Commits by CID

- **Parents:** ["go.query.commits"]

`_commits(cid: ...)` returns specific blocks.

# `go.query.commits.cid.lookup`: Fetching commits by CID

- **Parents:** ["go.query.commits.cid"]
- **Go-Tests:** ["query/commits/with_cid_test.go::TestQueryCommits_WithFirstCommitCid_ShouldSucceed", "query/commits/with_cid_test.go::TestQueryCommits_WithFirstCommitCidForFieldCommit_ShouldSucceed", "query/commits/with_cid_test.go::TestQueryCommits_ListOfOne", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsSameDoc", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsDifferentDocs", "query/commits/with_cid_test.go::TestQueryCommits_MultipleCidsDifferentDocs_NoAccessToSecondCid", "query/commits/with_cid_shared_field_test.go::TestQueryCommits_WithSharedFieldCidAndDocID", "query/commits/with_cid_shared_field_test.go::TestQueryCommits_WithSharedFieldCidWithoutDocID_ReturnsAllOwners", "query/commits/with_doc_id_cid_test.go::TestQueryCommits_WithDocIDAndCidWithUpdate", "query/commits/with_doc_id_cid_test.go::TestQueryCommitsWithDocIDAndCidWithUpdateAndDepth"]
- **Scope:** "all tests exclude the signed-docs and encrypted-docs multipliers (CIDs are hardcoded); TestQueryCommits_MultipleCidsDifferentDocs_NoAccessToSecondCid uses a DAC policy on the collection"

`_commits(cid: C)` (or `cid: [C]`) returns exactly that commit, for a field or composite block, even after later updates; `cid: [C1, C2]` returns both, across one or several docs, but omits a CID belonging to a doc the requesting identity cannot read under document ACP. A field CID shared by two docs returns one commit per owning doc when queried alone and only the named doc's commit when combined with `docID`. `docID` + `cid` + `depth: 5` returns the commit and its ancestors.

# `go.query.commits.cid.errors`: Invalid or unknown CIDs

- **Parents:** ["go.query.commits.cid"]
- **Go-Tests:** ["query/commits/with_cid_test.go::TestQueryCommitsWithInvalidCid", "query/commits/with_cid_test.go::TestQueryCommitsWithInvalidShortCid", "query/commits/with_cid_test.go::TestQueryCommitsWithUnknownCid", "query/commits/with_doc_id_cid_test.go::TestQueryCommitsWithDocIDAndCidForDifferentDoc", "query/commits/with_doc_id_cid_test.go::TestQueryCommitsWithDocIDAndCidForDifferentDocWithUpdate"]

A malformed CID string (`"fhbnjfahfhfhanfhga"`, `"bafybeidfhbnjfahfhfhanfhga"`) errors with `invalid cid`; a well-formed CID that is not in the store errors with `cid either does not exist or belong to document`.

# `go.query.commits.depth`: Depth-limited commit traversal

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_depth_test.go::TestQueryCommitsWithDepth1", "query/commits/with_depth_test.go::TestQueryCommitsWithDepth1WithUpdate", "query/commits/with_depth_test.go::TestQueryCommitsWithDepth2WithUpdate", "query/commits/with_depth_test.go::TestQueryCommitsWithDepth1AndMultipleDocs", "query/commits/with_depth_test.go::TestQueryCommits_WithFilterFieldNameAndDepth_ReturnsCommitsAtAllHeights"]

`_commits(depth: N)` returns the current heads of every block plus up to N-1 prior versions: depth 1 after a create returns 3 commits (6 for two docs), after an update the heads only (age at 2, name at 1, composite at 2); depth 2 after two updates adds the head-1 versions. Combined with `filter: {fieldName: {_eq: "age"}}` and depth 2 it returns `age` at heights 3 and 2.

# `go.query.commits.filter`: Filtering commits

- **Parents:** ["go.query.commits"]

`_commits(filter: ...)` filters on commit fields, mainly `fieldName`.

# `go.query.commits.filter.fieldname-cid`: fieldName filter combined with a CID

- **Parents:** ["go.query.commits.filter"]
- **Go-Tests:** ["query/commits/with_field_cid_test.go::TestQueryCommitsWithFieldAndCID", "query/commits/with_field_cid_test.go::TestQueryCommits_WithWrongFieldAndCID_ReturnEmptyList", "query/commits/with_field_cid_test.go::TestQueryCommits_WithInvalidFieldAndCID_ReturnEmptyList"]
- **Scope:** "TestQueryCommitsWithFieldAndCID excludes the signed-docs and encrypted-docs multipliers; the other two exclude the signed-docs multiplier"

`_commits(filter: {fieldName: {_eq: "age"}}, cid: <age field cid>)` returns that commit; the same CID with `fieldName` `"name"` or `"NOT_A_FIELD"` returns an empty list.

# `go.query.commits.filter.compound`: Compound and set operators on fieldName

- **Parents:** ["go.query.commits.filter"]
- **Go-Tests:** ["query/commits/with_filter_compound_test.go::TestQueryCommits_WithFilterFieldNameOrCondition_ReturnsMatchingCommits", "query/commits/with_filter_compound_test.go::TestQueryCommits_WithFilterFieldNameAndCondition_ReturnsOnlyNameCommit", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameIn_ReturnsMatchingCommits", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameInComposite_ReturnsCompositeCommit", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameInEmpty_ReturnsNoCommits", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameNotIn_ExcludesMatchingCommits", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameNotInComposite_ExcludesCompositeCommit", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameNotInEmpty_ReturnsAllCommits", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameInAndCondition_ReturnsFilteredCommits", "query/commits/with_filter_in_test.go::TestQueryCommits_WithFilterFieldNameNotInOrCondition_ReturnsFilteredCommits"]

`_or`, `_and`, `_in` and `_nin` work on `fieldName`: `_in: ["age", "name"]` returns those two field commits, `_in: ["_C"]` the composite, `_in: []` nothing, `_nin: ["_C", "age"]` only `name`, `_nin: []` all three, and nested combinations (e.g. `_and: [{_in: [age, name]}, {_neq: age}]` gives `name`) evaluate as boolean logic.

# `go.query.commits.order`: Ordering commits

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_doc_id_order_test.go::TestQueryCommitsWithDocIDAndOrderHeightDesc", "query/commits/with_doc_id_order_test.go::TestQueryCommitsWithDocIDAndOrderHeightAsc", "query/commits/with_doc_id_order_test.go::TestQueryCommitsWithDocIDAndOrderCidDesc", "query/commits/with_doc_id_order_test.go::TestQueryCommitsWithDocIDAndOrderCidAsc", "query/commits/with_doc_id_order_test.go::TestQueryCommitsWithDocIDAndOrderAndMultiUpdatesCidAsc"]
- **Scope:** "the two order-by-cid tests exclude the signed-docs and encrypted-docs multipliers"

`_commits(docID: X, order: {height: ASC|DESC})` sorts commits by height (after three updates: 1,1,1,2,2,3,3,4,4 ascending); `order: {cid: ASC|DESC}` sorts by CID bytes.

# `go.query.commits.paging`: Limit and offset on commits

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_doc_id_limit_test.go::TestQueryCommitsWithDocIDAndLimit", "query/commits/with_doc_id_limit_offset_test.go::TestQueryCommitsWithDocIDAndLimitAndOffset", "query/commits/with_doc_id_order_limit_offset_test.go::TestQueryCommitsWithDocIDAndOrderAndLimitAndOffset"]

`_commits(docID: X, limit: 2)` returns 2 commits and `limit: 2, offset: 1` skips one; with `order: {height: ASC}, limit: 2, offset: 4` after three updates it returns the commits at heights 2 and 3.

# `go.query.commits.group`: Grouping commits

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_group_test.go::TestQueryCommitsWithGroupBy", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByHeightWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByCidWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByDocID", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByFieldName", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByFieldNameWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByEmptyWithChild", "query/commits/with_group_test.go::TestQueryCommitsWithGroupByRejectsNonGroupFields", "query/commits/with_doc_id_group_order_test.go::TestQueryCommitsOrderedAndGroupedByDocID"]

`_commits(groupBy: [height|cid|docID|fieldName])` returns one row per distinct value with a `GROUP` sub-list of the member commits, and combines with `order` (`groupBy: [docID], order: {docID: DESC}`); `groupBy: []` returns a single row whose `GROUP` holds every commit. Selecting a non-grouped field at group level (e.g. `groupBy: [cid] { cid height }`, `groupBy: [] { cid }`, `links`/`heads` at group level, or grouped nested `links`/`heads`) errors with `cannot select a non-group-by field at group-level`.

# `go.query.commits.null-args`: Null arguments are ignored

- **Parents:** ["go.query.commits"]
- **Go-Tests:** ["query/commits/with_null_input_test.go::TestQueryCommitsWithNullDepth", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullCID", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullField", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullOrder", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullOrderField", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullLimit", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullOffset", "query/commits/with_null_input_test.go::TestQueryCommitsWithNullGroupBy"]

Passing `null` for `depth`, `cid`, `docID`, `order`, `limit`, `offset` or `groupBy` behaves as if the argument were absent (all 3 commits returned); `filter: {fieldName: {_eq: null}}` on a non-branchable collection returns an empty list.

# `go.query.json`: Filtering JSON fields

- **Parents:** ["go.query"]

Filters on a `JSON` scalar field compare whole values or descend into object paths (`custom: {path: {_op: v}}`).

# `go.query.json.eq`: JSON equality

- **Parents:** ["go.query.json"]
- **Go-Tests:** ["query/json/with_eq_test.go::TestQueryJSON_WithEqualFilterWithObject_ShouldFilter", "query/json/with_eq_test.go::TestQueryJSON_WithCompoundFilterCondition_ShouldFilter", "query/json/with_eq_test.go::TestQueryJSON_WithEqualFilterWithNestedObjects_ShouldFilter", "query/json/with_eq_test.go::TestQueryJSON_WithEqualFilterWithNullValue_ShouldFilter", "query/json/with_eq_test.go::TestQueryJSON_WithEqualFilterWithAllTypes_ShouldFilter", "query/json/with_eq_test.go::TestQueryJSON_WithEqualFilterWithObjectValueOnNestedPath_ShouldFilter", "query/json/with_aggregate_test.go::TestQueryJSON_WithAggregateFilter_Succeeds"]

`{custom: {_eq: {tree: "oak", age: 450}}}` matches documents whose JSON equals that object (deep, including nested arrays in order); `_eq: null` matches a null value but not `{}`; an object literal never matches strings, numbers, bools or arrays. A path filter (`{custom: {tree: {_eq: "oak"}}}`, `{custom: {nested: {_eq: {foo: "bar"}}}}`) compares the value at that path, combines under `_and`, and works inside `COUNT(Users: {filter: ...})` (returns 1).

# `go.query.json.neq`: JSON inequality

- **Parents:** ["go.query.json"]
- **Go-Tests:** ["query/json/with_neq_test.go::TestQueryJSON_WithNotEqualFilterWithObject_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNotEqualFilterWithNestedObjects_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNotEqualFilterWithNullValue_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNeFilterAgainstNumberField_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNeFilterAgainstStringField_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNeFilterAgainstBooleanField_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNeFilterAgainstNullField_ShouldFilter", "query/json/with_neq_test.go::TestQueryJSON_WithNotEqualFilterWithNestedObject_ShouldFilter"]
- **Go-Bug-Suspect:** "'{custom: {age: {_neq: 48}}}' includes a doc whose 'age' is explicitly null, but '{custom: {nested: {_neq: {foo: 'bar'}}}}' excludes a doc that lacks the 'nested' key entirely; a missing path and a null path are treated differently"

`_neq` is the negation of JSON `_eq`: a whole-value `_neq` object returns every other doc including one whose JSON is null; `_neq: null` returns non-null docs. On a path (`{custom: {age: {_neq: 48}}}`, string and bool variants) docs where the path holds null are included and `{age: {_neq: null}}` returns docs with a non-null age; on an object-valued path, docs whose path holds a different object or a scalar match, while docs missing the path do not.

# `go.query.json.in`: JSON membership

- **Parents:** ["go.query.json"]
- **Go-Tests:** ["query/json/with_in_test.go::TestQueryJSON_WithInFilter_ShouldFilter", "query/json/with_nin_test.go::TestQueryJSON_WithNotInFilter_ShouldFilter"]

`{custom: {_in: [{tree: "oak", age: 450}]}}` matches docs whose JSON equals one of the listed objects and `_nin` returns the others.

# `go.query.json.ordering`: JSON range comparisons

- **Parents:** ["go.query.json"]

`_gt`, `_geq`, `_lt`, `_leq` on JSON values and paths.

# `go.query.json.ordering.greater`: Greater-than on JSON numbers

- **Parents:** ["go.query.json.ordering"]
- **Go-Tests:** ["query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithGreaterValue_ShouldFilter", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithLesserValue_ShouldFilter", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithNestedGreaterValue_ShouldFilter", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithNestedLesserValue_ShouldFilter", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterWithAllTypes_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithEqualValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithGreaterValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithNestedEqualValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithNestedGreaterValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithAllTypes_ShouldFilter"]

`{Custom: {_gt: 20}}` / `_geq` with a number match docs whose JSON number (top level or at a path such as `{Custom: {age: {_gt: 20}}}`) is greater (or equal); non-number JSON values (strings like `"32"`, arrays, objects, bools) never match. The returned top-level number renders as an integer (`21`) and a number nested in an object as a float (`21.0`).

# `go.query.json.ordering.lesser`: Less-than on JSON numbers

- **Parents:** ["go.query.json.ordering"]
- **Go-Tests:** ["query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithGreaterValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithLesserValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithNestedGreaterValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithNestedLesserValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterWithAllTypes_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithEqualValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithLesserValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithNestedEqualValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithNestedLesserValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithAllTypes_ShouldFilter"]

`_lt` / `_leq` with a number match docs whose JSON number (top level or at a path) is less (or equal); non-number JSON values never match (`{Custom: {_lt: 33}}` over `"32"`, `[1,2]`, `{one:1}`, `false`, `32` returns only the `32` doc).

# `go.query.json.ordering.null`: Range comparisons against null

- **Parents:** ["go.query.json.ordering"]
- **Go-Tests:** ["query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithNullFilterValue_ShouldFilter", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithNestedNullFilterValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithNullValue_ShouldFilter", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithNestedNullValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithNullFilterValue_ShouldFilter", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithNestedNullFilterValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithNullValue_ShouldFilter", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithNestedNullValue_ShouldFilter"]

Null sorts below every value: `_gt: null` matches only docs with a non-null value, `_geq: null` matches all docs (including unset and explicit-null path values), `_lt: null` matches none, and `_leq: null` matches only docs where the value or path is unset or null.

# `go.query.json.ordering.type-errors`: Non-numeric range operands

- **Parents:** ["go.query.json.ordering"]
- **Go-Tests:** ["query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithBoolValue_ReturnsError", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithStringValue_ReturnsError", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithObjectValue_ReturnsError", "query/json/with_gt_test.go::TestQueryJSON_WithGreaterThanFilterBlockWithArrayValue_ReturnsError", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithBoolValue_ReturnsError", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithStringValue_ReturnsError", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithObjectValue_ReturnsError", "query/json/with_geq_test.go::TestQueryJSON_WithGreaterEqualFilterWithArrayValue_ReturnsError", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithBoolValue_ReturnsError", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithStringValue_ReturnsError", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithObjectValue_ReturnsError", "query/json/with_lt_test.go::TestQueryJSON_WithLesserThanFilterBlockWithArrayValue_ReturnsError", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithBoolValue_ReturnsError", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithStringValue_ReturnsError", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithObjectValue_ReturnsError", "query/json/with_leq_test.go::TestQueryJSON_WithLesserEqualFilterWithArrayValue_ReturnsError"]
- **Go-Bug-Suspect:** "the asserted messages leak Go type names: 'unexpected type. Property: condition, Actual: map[string]interface {}' and '[]interface {}'; Rust cannot reproduce them literally"

`_gt`, `_geq`, `_lt` or `_leq` on a JSON field with a bool, string, object or array operand errors with `unexpected type. Property: condition, Actual: bool` (resp. `string`, `map[string]interface {}`, `[]interface {}`).

# `go.query.json.like`: Pattern matching on JSON strings

- **Parents:** ["go.query.json"]
- **Go-Tests:** ["query/json/with_like_test.go::TestQueryJSON_WithLikeFilter_ShouldFilter", "query/json/with_nlike_test.go::TestQueryJSON_WithNotLikeFilter_ShouldFilter"]

`{custom: {_like: "Daenerys%Name"}}` matches JSON string values by pattern and never matches arrays, objects, bools or numbers; `_nlike: "%Stormborn%"` returns every doc whose value does not match, including non-string values.

# `go.query.json.array-ops`: `_any`, `_all`, `_none` on JSON arrays

- **Parents:** ["go.query.json"]
- **Go-Tests:** ["query/json/with_all_test.go::TestQueryJSON_WithAllFilterWithAllTypes_ShouldFilter", "query/json/with_all_test.go::TestQueryJSON_WithAllFilterAndNestedArray_ShouldFilter", "query/json/with_any_test.go::TestQueryJSON_WithAnyFilterWithAllTypes_ShouldFilter", "query/json/with_any_test.go::TestQueryJSON_WithAnyFilterAndNestedArray_ShouldFilter", "query/json/with_none_test.go::TestQueryJSON_WithNoneFilter_ShouldFilter", "query/json/with_none_test.go::TestQueryJSON_WithNoneFilterAndNestedArray_ShouldFilter"]

`_all`, `_any` and `_none` apply the inner condition to the top-level elements of a JSON array and never match a non-array value (null, scalar 0/1/3, `""`, `true`). Nested arrays are not flattened: `_any: {_eq: 3}` does not match `[..., [1, [2, 3]]]` and `_all: {_eq: 1}` does not match `[1, [1, [1]]]`.

# `go.query.inline_array`: Inline scalar array fields

- **Parents:** ["go.query"]

Fields typed `[T]` / `[T!]` for Boolean, Int, Float, Float32, Float64, String and DateTime.

# `go.query.inline_array.read`: Reading inline arrays

- **Parents:** ["go.query.inline_array"]

Values round-trip through create and query.

# `go.query.inline_array.read.bool-int`: Boolean and Int arrays

- **Parents:** ["go.query.inline_array.read"]
- **Go-Tests:** ["query/inline_array/simple_test.go::TestQueryInlineArrayWithBooleans_Null", "query/inline_array/simple_test.go::TestQueryInlineArrayWithBooleans_EmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithBooleans_NotEmpty", "query/inline_array/simple_test.go::TestQueryInlineArrayWithNillableBooleans", "query/inline_array/simple_test.go::TestQueryInlineArrayWithIntegers_Missing", "query/inline_array/simple_test.go::TestQueryInlineArrayWithIntegers_Null", "query/inline_array/simple_test.go::TestQueryInlineArrayWithIntegers_EmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithIntegers_NotEmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithNegativeIntegers_NotEmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithMixIntegers_NotEmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithNillableInts"]

`[Boolean!]`, `[Boolean]`, `[Int!]` and `[Int]` fields return exactly what was stored: null when set to null or omitted, `[]` for an empty list, values in insertion order (including negatives), and null elements in nillable arrays (`[-1, null, -1, 2, 0]`).

# `go.query.inline_array.read.float-string`: Float and String arrays

- **Parents:** ["go.query.inline_array.read"]
- **Go-Tests:** ["query/inline_array/simple_test.go::TestQueryInlineArrayWithFloats_Null", "query/inline_array/simple_test.go::TestQueryInlineArrayWithFloats_EmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithFloats_NotEmpty", "query/inline_array/simple_test.go::TestQueryInlineArrayWithNillableFloats", "query/inline_array/simple_test.go::TestQueryInlineArrayWithStrings_Null", "query/inline_array/simple_test.go::TestQueryInlineArrayWithStrings_EmptyList", "query/inline_array/simple_test.go::TestQueryInlineArrayWithStrings_NotEmpty", "query/inline_array/simple_test.go::TestQueryInlineArrayWithNillableString"]

`[Float!]`, `[Float]`, `[String!]` and `[String]` fields return null, `[]`, or the stored elements in order, preserving precision (`0.00000000001`, `-0.00000000001`), empty strings and null elements in nillable arrays.

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

# `go.query.inline_array.non-nillable.scalar-required`: Non-nillable DateTime, Blob and JSON fields

- **Parents:** ["go.query.inline_array.non-nillable"]
- **Go-Tests:** ["query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableDateTime_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableDateTime_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableDateTime_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableBlob_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableBlob_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableBlob_Omitted", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableJSON_Null", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableJSON_NotEmpty", "query/inline_array/with_nonnillable_scalars_test.go::TestQueryNonNillableJSON_Omitted"]

For `DateTime!`, `Blob!` and `JSON!` fields a create with an explicit null fails with `null value provided for non-nillable field. Name: <field>` and a create omitting the field fails with `value not provided for non-nillable field. Name: <field>`; a provided value is stored and returned (`"ff0011"`, `{"role": "admin"}`).

# `go.query.inline_array.filter`: Filtering on inline arrays

- **Parents:** ["go.query.inline_array"]

Collection filters on array fields.

# `go.query.inline_array.filter.all`: `_all` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_filter_all_test.go::TestQueryInlineStringArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineNotNullStringArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineIntArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineNotNullIntArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineFloatArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineNotNullFloatArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineBooleanArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineNotNullBooleanArray_WithAllFilter_Succeeds", "query/inline_array/with_filter_all_test.go::TestQueryInlineStringArray_WithAllFilterAndNullValue_Succeeds"]

`Users(filter: {field: {_all: {...}}})` returns docs whose every element satisfies the condition (`_all: {_neq: null}` excludes arrays holding a null; `_all: {_lt: 70}` keeps `[0, 60]`) for String, Int, Float and Boolean arrays, nillable or not; a doc whose array is null does not match `_all: {_eq: null}`.

# `go.query.inline_array.filter.any`: `_any` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_filter_any_test.go::TestQueryInlineStringArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineNotNullStringArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineIntArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineNotNullIntArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineFloatArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineNotNullFloatArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineBooleanArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineNotNullBooleanArray_WithAnyFilter_Succeeds", "query/inline_array/with_filter_any_test.go::TestQueryInlineStringArray_WithAnyFilterAndNullValue_Succeeds"]

`_any` returns docs with at least one matching element (`_any: {_eq: null}` matches `[null, "second"]`, `_any: {_gt: 70}` matches `[50, 80]`) for String, Int, Float and Boolean arrays; a null array does not match `_any: {_eq: null}`.

# `go.query.inline_array.filter.none`: `_none` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_filter_none_test.go::TestQueryInlineStringArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineNonNullStringArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineIntArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineNonNullIntArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineFloatArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineNonNullFloatArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineBooleanArrayWithNoneFilter", "query/inline_array/with_filter_none_test.go::TestQueryInlineNonNullBooleanArrayWithNoneFilter"]

`_none` returns docs where no element matches (`_none: {_eq: null}` keeps arrays without nulls, `_none: {_gt: 70}` keeps `[0, 60]`, `_none: {_neq: true}` keeps `[true, true]`) for String, Int, Float and Boolean arrays.

# `go.query.inline_array.filter.eq`: Whole-array equality

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_filter_eq_test.go::TestQueryInlineBooleanArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableBooleanArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineIntegerArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableIntegerArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineFloatArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableFloatArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineStringArray_WithEqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableStringArray_WithEqFilter_ReturnsResults"]

`{field: {_eq: [..]}}` matches docs whose array equals the literal element by element, including null elements (`[90, null, 85]`), for Boolean, Int, Float and String arrays.

# `go.query.inline_array.filter.neq`: Whole-array inequality

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_filter_eq_test.go::TestQueryInlineBooleanArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableBooleanArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineIntegerArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableIntegerArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineFloatArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableFloatArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineStringArray_WithNeqFilter_ReturnsResults", "query/inline_array/with_filter_eq_test.go::TestQueryInlineNullableStringArray_WithNeqFilter_ReturnsResults"]

`{field: {_neq: [..]}}` returns the docs whose array differs from the literal, for Boolean, Int, Float and String arrays with or without null elements.

# `go.query.inline_array.filter.datetime`: Filtering DateTime arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Go-Tests:** ["query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_FilterAny", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithNillableDateTime_FilterAny", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_FilterAll", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_FilterNone", "query/inline_array/with_datetime_test.go::TestQueryInlineArrayWithDateTime_Index"]

`_any`, `_all` and `_none` on `[DateTime!]` / `[DateTime]` compare elements as timestamps against RFC 3339 strings (`_all: {_geq: "2024-01-15T09:00:00Z"}`, `_none: {_lt: "2024-01-01T00:00:00Z"}`), skip null elements, and return the same result when the array field has an `@index`.

# `go.query.inline_array.aggregate`: Aggregates over inline arrays

- **Parents:** ["go.query.inline_array"]

`COUNT`, `SUM`, `AVG`, `MIN`, `MAX` with an array field as the source (`AGG(field: {filter, limit, offset, order})`).

# `go.query.inline_array.aggregate.count`: COUNT over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element counts.

# `go.query.inline_array.aggregate.count.basic`: Plain COUNT

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Go-Tests:** ["query/inline_array/with_count_test.go::TestQueryInlineIntegerArrayWithCountAndNullArray", "query/inline_array/with_count_test.go::TestQueryInlineIntegerArrayWithCountAndEmptyArray", "query/inline_array/with_count_test.go::TestQueryInlineIntegerArrayWithCountAndPopulatedArray", "query/inline_array/with_count_test.go::TestQueryInlineNillableBoolArrayWithCountAndPopulatedArray"]

`COUNT(favouriteIntegers: {})` returns 0 for a null or empty array and the number of elements otherwise; null elements are counted (`[true, true, false, null]` gives 4).

# `go.query.inline_array.aggregate.count.filter`: COUNT with an element filter

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Go-Tests:** ["query/inline_array/with_count_filter_test.go::TestQueryInlineBoolArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineNillableBoolArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineIntegerArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineNillableIntegerArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineIntegerArrayWithsWithCountWithAndFilterAndPopulatedArray", "query/inline_array/with_count_filter_test.go::TestQueryInlineFloatArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineNillableFloatArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineFloat32Array_WithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineNillableFloat32Array_WithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineFloat64Array_WithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineNillableFloat64Array_WithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineStringArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQueryInlineNillableStringArrayWithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineDateTimeArray_WithCountWithFilter", "query/inline_array/with_count_filter_test.go::TestQuery_InlineNillableDateTimeArray_WithCountWithFilter"]

`COUNT(field: {filter: {...}})` counts only elements matching the condition (`_eq`, `_gt`, `_lt`, `_in`, `_and` of ranges), for Boolean, Int, Float, Float32, Float64, String and DateTime arrays; null elements never match a value condition.

# `go.query.inline_array.aggregate.count.paging`: COUNT with limit and offset

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Go-Tests:** ["query/inline_array/with_count_limit_test.go::TestQueryInlineIntegerArrayWithCountWithLimitGreaterThanLength", "query/inline_array/with_count_limit_test.go::TestQueryInlineIntegerArrayWithCountWithLimit", "query/inline_array/with_count_limit_offset_test.go::TestQueryInlineIntegerArrayWithCountWithOffsetWithLimitGreaterThanLength", "query/inline_array/with_count_limit_offset_test.go::TestQueryInlineIntegerArrayWithCountWithOffsetWithLimit"]

`COUNT(field: {limit: 3})` returns min(limit, length) and `{offset: 1, limit: 3}` counts at most `limit` elements after skipping `offset` (2 for a 3-element array).

# `go.query.inline_array.aggregate.sum`: SUM over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element sums.

# `go.query.inline_array.aggregate.sum.basic`: Plain SUM

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Go-Tests:** ["query/inline_array/with_sum_test.go::TestQueryInlineIntegerArrayWithSumAndNullArray", "query/inline_array/with_sum_test.go::TestQueryInlineIntegerArrayWithSumAndEmptyArray", "query/inline_array/with_sum_test.go::TestQueryInlineIntegerArrayWithSumAndPopulatedArray", "query/inline_array/with_sum_test.go::TestQueryInlineNillableIntegerArrayWithSumAndPopulatedArray", "query/inline_array/with_sum_test.go::TestQueryInlineFloatArrayWithSumAndNullArray", "query/inline_array/with_sum_test.go::TestQueryInlineFloatArrayWithSumAndEmptyArray", "query/inline_array/with_sum_test.go::TestQueryInlineFloatArrayWithSumAndPopulatedArray", "query/inline_array/with_sum_test.go::TestQueryInlineNillableFloatArrayWithSumAndPopulatedArray"]

`SUM(field: {})` returns an Int for Int arrays and a Float for Float arrays, 0 for a null or empty array, and ignores null elements (`[-1, 2, null, 1, 0]` gives 2; `[3.1425, 0.00000000001, 10]` gives 13.14250000001).

# `go.query.inline_array.aggregate.sum.filter`: SUM with an element filter

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Go-Tests:** ["query/inline_array/with_sum_filter_test.go::TestQueryInlineIntegerArrayWithSumWithFilter", "query/inline_array/with_sum_filter_test.go::TestQueryInlineNillableIntegerArrayWithSumWithFilter", "query/inline_array/with_sum_filter_test.go::TestQueryInlineFloatArrayWithSumWithFilter", "query/inline_array/with_sum_filter_test.go::TestQueryInlineNillableFloatArrayWithSumWithFilter"]

`SUM(field: {filter: {_gt: 0}})` sums only matching elements (3 for `[-1, 2, -1, 1, 0]`; 3.14250000001 for floats `_lt: 9`), in nillable and non-nillable Int and Float arrays.

# `go.query.inline_array.aggregate.sum.paging`: SUM with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Go-Tests:** ["query/inline_array/with_sum_limit_test.go::TestQueryInlineIntegerArrayWithSumWithLimit", "query/inline_array/with_sum_limit_offset_test.go::TestQueryInlineIntegerArrayWithSumWithOffsetWithLimit", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineIntegerArrayWithSumWithOffsetWithLimitWithOrderAsc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineIntegerArrayWithSumWithOffsetWithLimitWithOrderDesc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineNillableIntegerArrayWithSumWithOffsetWithLimitWithOrderAsc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineNillableIntegerArrayWithSumWithOffsetWithLimitWithOrderDesc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineFloatArrayWithSumWithOffsetWithLimitWithOrderAsc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineFloatArrayWithSumWithOffsetWithLimitWithOrderDesc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineNillableFloatArrayWithSumWithOffsetWithLimitWithOrderAsc", "query/inline_array/with_sum_limit_offset_order_test.go::TestQueryInlineNillableFloatArrayWithSumWithOffsetWithLimitWithOrderDesc"]

Without `order`, `limit`/`offset` take elements in stored order (`{limit: 2}` on `[-1, 2, ...]` gives 1; `{offset: 1, limit: 2}` on `[-1, 2, 5, ...]` gives 7). With `order: ASC|DESC` elements are sorted first, nulls dropped, then offset and limit applied (`[-1, 2, 5, 1, 0, 7]` with offset 1, limit 3: ASC 0+1+2 = 3, DESC 5+2+1 = 8).

# `go.query.inline_array.aggregate.avg`: AVG over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element averages.

# `go.query.inline_array.aggregate.avg.basic`: Plain AVG

- **Parents:** ["go.query.inline_array.aggregate.avg"]
- **Go-Tests:** ["query/inline_array/with_average_test.go::TestQueryInlineIntegerArrayWithAverageAndNullArray", "query/inline_array/with_average_test.go::TestQueryInlineIntegerArrayWithAverageAndEmptyArray", "query/inline_array/with_average_test.go::TestQueryInlineIntegerArrayWithAverageAndZeroArray", "query/inline_array/with_average_test.go::TestQueryInlineIntegerArrayWithAverageAndPopulatedArray", "query/inline_array/with_average_test.go::TestQueryInlineNillableIntegerArrayWithAverageAndPopulatedArray", "query/inline_array/with_average_test.go::TestQueryInlineFloatArrayWithAverageAndNullArray", "query/inline_array/with_average_test.go::TestQueryInlineFloatArrayWithAverageAndEmptyArray", "query/inline_array/with_average_test.go::TestQueryInlineFloatArrayWithAverageAndZeroArray", "query/inline_array/with_average_test.go::TestQueryInlineFloatArrayWithAverageAndPopulatedArray", "query/inline_array/with_average_test.go::TestQueryInlineNillableFloatArrayWithAverageAndPopulatedArray"]
- **Go-Bug-Suspect:** "AVG of a null or empty array is asserted as 0, while MIN and MAX of the same input return null; an average over no values is undefined"

`AVG(field: {})` returns a Float: 0 for a null, empty or all-zero array, and the mean of the non-null elements otherwise (`[-1, null, 13, 0]` gives 4; `[-0.1, 0, 0.9, 0]` gives 0.2).

# `go.query.inline_array.aggregate.avg.filter`: AVG with a filter, and AVG alongside SUM

- **Parents:** ["go.query.inline_array.aggregate.avg"]
- **Go-Tests:** ["query/inline_array/with_average_filter_test.go::TestQueryInlineIntegerArrayWithAverageWithFilter", "query/inline_array/with_average_filter_test.go::TestQueryInlineNillableIntegerArrayWithAverageWithFilter", "query/inline_array/with_average_filter_test.go::TestQueryInlineFloatArrayWithAverageWithFilter", "query/inline_array/with_average_filter_test.go::TestQueryInlineNillableFloatArrayWithAverageWithFilter", "query/inline_array/with_average_sum_test.go::TestQueryInlineIntegerArrayWithAverageAndSum"]

`AVG(field: {filter: ...})` averages only matching non-null elements (`_gt: 0` on `[-1, 2, -1, 1, 0]` gives 1.5; `_gt: -1` on `[-1, null, 13, 0]` gives 6.5). `AVG` and `SUM` selected together on the same array under `groupBy: [name]` return 2 and 8 for `[-1, 0, 9, 0]`.

# `go.query.inline_array.aggregate.min`: MIN over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element minima.

# `go.query.inline_array.aggregate.min.basic`: Plain MIN

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Go-Tests:** ["query/inline_array/with_min_test.go::TestQueryInlineIntegerArray_WithMinAndNullArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineIntegerArray_WithMinAndEmptyArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineIntegerArray_WithMinAndPopulatedArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineNillableIntegerArray_WithMinAndPopulatedArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineFloatArray_WithMinAndNullArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineFloatArray_WithMinAndEmptyArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineFloatArray_WithMinAndPopulatedArray_Succeeds", "query/inline_array/with_min_test.go::TestQueryInlineNillableFloatArray_WithMinAndPopulatedArray_Succeeds", "query/inline_array/with_min_doc_id_test.go::TestQueryInlineNillableFloatArray_WithDocIDAndMin_Succeeds"]

`MIN(field: {})` returns null for a null or empty array and the smallest non-null element otherwise (`-1`; `0.00000000001`), with the element type preserved, including when the parent is selected by `docID`.

# `go.query.inline_array.aggregate.min.filter`: MIN with a filter

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Go-Tests:** ["query/inline_array/with_min_filter_test.go::TestQueryInlineIntegerArray_WithMinWithFilter_Succeeds", "query/inline_array/with_min_filter_test.go::TestQueryInlineNillableIntegerArray_WithMinWithFilter_Succeeds", "query/inline_array/with_min_filter_test.go::TestQueryInlineFloatArray_WithMinWithFilter_Succeeds", "query/inline_array/with_min_filter_test.go::TestQueryInlineNillableFloatArray_WithMinWithFilter_Succeeds"]

`MIN(field: {filter: {_gt: 0}})` returns the smallest matching element (1 for `[-1, 2, -1, 1, 0]`; 3.1425 for floats `_gt: 1`).

# `go.query.inline_array.aggregate.min.paging`: MIN with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Go-Tests:** ["query/inline_array/with_min_limit_offset_test.go::TestQueryInlineIntegerArray_WithMinWithOffsetWithLimit_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineIntegerArray_WithMinWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineIntegerArray_WithMinWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineNillableIntegerArray_WithMinWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineNillableIntegerArray_WithMinWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineFloatArray_WithMinWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineFloatArray_WithMinWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineNillableFloatArray_WithMinWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_min_limit_offset_order_test.go::TestQueryInlineNillableFloatArray_WithMinWithOffsetWithLimitWithOrderDesc_Succeeds"]

MIN is taken over the window selected by offset/limit: in stored order without `order` (`{offset: 1, limit: 2}` on `[-1, 2, 5, 1, 0]` gives 2), or after sorting (nulls dropped) with `order` (`[-1, 2, 5, 1, 0, 7]`, offset 1, limit 3: ASC gives 0, DESC gives 1).

# `go.query.inline_array.aggregate.max`: MAX over arrays

- **Parents:** ["go.query.inline_array.aggregate"]

Element maxima.

# `go.query.inline_array.aggregate.max.basic`: Plain MAX

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Go-Tests:** ["query/inline_array/with_max_test.go::TestQueryInlineIntegerArray_WithMaxAndNullArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineIntegerArray_WithMaxAndEmptyArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineIntegerArray_WithMaxAndPopulatedArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineNillableIntegerArray_WithMaxAndPopulatedArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineFloatArray_WithMaxAndNullArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineFloatArray_WithMaxAndEmptyArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineFloatArray_WithMaxAndPopulatedArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineNillableFloatArray_WithMaxAndPopulatedArray_Succeeds", "query/inline_array/with_max_test.go::TestQueryInlineNillableFloatArray_WithDocIDMaxAndPopulatedArray_Succeeds", "query/inline_array/with_max_doc_id_test.go::TestQueryInlineNillableFloatArray_WithDocIDAndMax_Succeeds"]

`MAX(field: {})` returns null for a null or empty array and the largest non-null element otherwise (2; 10.0), with the element type preserved, including when the parent is selected by `docID`.

# `go.query.inline_array.aggregate.max.filter`: MAX with a filter

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Go-Tests:** ["query/inline_array/with_max_filter_test.go::TestQueryInlineIntegerArray_WithMaxWithFilter_Succeeds", "query/inline_array/with_max_filter_test.go::TestQueryInlineNillableIntegerArray_WithMaxWithFilter_Succeeds", "query/inline_array/with_max_filter_test.go::TestQueryInlineFloatArray_WithMaxWithFilter_Succeeds", "query/inline_array/with_max_filter_test.go::TestQueryInlineNillableFloatArray_WithMaxWithFilter_Succeeds"]

`MAX(field: {filter: {_lt: 2}})` returns the largest matching element (1 for `[-1, 2, -1, 1, 0]`; 3.1425 for floats `_lt: 9`).

# `go.query.inline_array.aggregate.max.paging`: MAX with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Go-Tests:** ["query/inline_array/with_max_limit_offset_test.go::TestQueryInlineIntegerArray_WithMaxWithOffsetWithLimit_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineIntegerArray_WithMaxWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineIntegerArray_WithMaxWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineNillableIntegerArray_WithMaxWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineNillableIntegerArray_WithMaxWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineFloatArray_WithMaxWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineFloatArray_WithMaxWithOffsetWithLimitWithOrderDesc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineNillableFloatArray_WithMaxWithOffsetWithLimitWithOrderAsc_Succeeds", "query/inline_array/with_max_limit_offset_order_test.go::TestQueryInlineNillableFloatArray_WithMaxWithOffsetWithLimitWithOrderDesc_Succeeds"]

MAX is taken over the offset/limit window: stored order without `order` (`{offset: 1, limit: 2}` on `[-1, 2, 5, 1, 0]` gives 5), sorted with nulls dropped when `order` is given (`[-1, 2, 5, 1, 0, 7]`, offset 1, limit 3: ASC gives 2, DESC gives 5).

# `go.query.inline_array.aggregate.alias-order`: Ordering parents by an aggregate alias

- **Parents:** ["go.query.inline_array.aggregate"]
- **Go-Tests:** ["query/inline_array/with_count_order_test.go::TestQueryInlineIntegerArray_WithCountAndOrder_Succeeds", "query/inline_array/with_count_order_test.go::TestQueryInlineIntegerArray_WithNullAndCountAndOrder_Succeeds", "query/inline_array/with_sum_order_test.go::TestQueryInlineIntegerArray_WithSumAndOrder_Succeeds", "query/inline_array/with_sum_order_test.go::TestQueryInlineIntegerArray_WithNullAndSumAndOrder_Succeeds", "query/inline_array/with_average_order_test.go::TestQueryInlineIntegerArrayWithAverageAndOrder_Succeeds", "query/inline_array/with_average_order_test.go::TestQueryInlineIntegerArrayWithNullWithAverageAndOrder_Succeeds", "query/inline_array/with_min_order_test.go::TestQueryInlineIntegerArray_WithMinAndOrder_Succeeds", "query/inline_array/with_min_order_test.go::TestQueryInlineIntegerArray_WithNullAndMinAndOrder_Succeeds", "query/inline_array/with_max_order_test.go::TestQueryInlineIntegerArray_WithMaxAndOrder_Succeeds", "query/inline_array/with_max_order_test.go::TestQueryInlineIntegerArray_WithNullAndMaxAndOrder_Succeeds"]

An aggregate over two array fields (`total: SUM(testScores: {}, pageRatings: {})`, likewise COUNT, AVG, MIN, MAX) combines elements of both arrays, and `Users(order: {_alias: {total: ASC|DESC}})` orders docs by it. Null elements are ignored by SUM, AVG, MIN and MAX but counted by COUNT (`[3,4,5,null]` + `[1,2,3,null]` counts 8).

# `go.query.inline_array.group`: Grouping with inline arrays

- **Parents:** ["go.query.inline_array"]
- **Go-Tests:** ["query/inline_array/with_group_test.go::TestQueryInlineArrayWithGroupByString", "query/inline_array/with_group_test.go::TestQueryInlineArrayWithGroupByArray"]

Array fields can be selected inside `GROUP` of a `groupBy: [name]` query, and `groupBy: [favouriteIntegers]` groups docs whose arrays are element-wise equal.

# `go.query.one_to_many`: One-to-many relation queries

- **Parents:** ["go.query"]

Schema `Book { author: Author }` / `Author { published: [Book] }` unless stated.

# `go.query.one_to_many.join`: Resolving the relation in both directions

- **Parents:** ["go.query.one_to_many"]
- **Go-Tests:** ["query/one_to_many/one_sided_test.go::TestQueryOneToMany_OneSided", "query/one_to_many/simple_test.go::TestQueryOneToMany_PrimaryDirection", "query/one_to_many/simple_test.go::TestQueryOneToMany_SecondaryDirection", "query/one_to_many/simple_test.go::TestQueryOneToManyWithNonExistantParent", "query/one_to_many/with_same_field_name_test.go::TestQueryOneToManyWithSameFieldName_SingleSide", "query/one_to_many/with_same_field_name_test.go::TestQueryOneToManyWithSameFieldName_MultiSide", "query/one_to_many/with_typename_test.go::TestQueryOneToManyWithTypeName"]

`Book { author { ... } }` returns the related author object, or null when the book has none; `Author { published { ... } }` returns the list of books whose `_authorID` points at the author. A relation declared only on the many side (`Book.author` without `Author.published`) still resolves, the relation field may share its name on both types (`relationship1`), and `__typename` returns `Book` / `Author` at each level.

# `go.query.one_to_many.relation-id`: The `_<field>ID` relation field

- **Parents:** ["go.query.one_to_many"]
- **Go-Tests:** ["query/one_to_many/with_related_id_test.go::TestQueryOneToManyWithRelatedTypeIDFromManySide", "query/one_to_many/with_related_id_test.go::TestQueryOneToManyWithRelatedTypeIDFromSingleSide", "query/one_to_many/with_id_field_test.go::TestQueryOneToManyWithIdFieldOnPrimary"]

`Book { _authorID }` returns the related author's docID. Selecting `_authorID` on `Author` errors with `Cannot query field "_authorID" on type "Author".`; declaring a field named `_authorID` alongside `author: Author` fails schema creation with `duplicate field. Name: _authorID`.

# `go.query.one_to_many.filter`: Filtering across the relation

- **Parents:** ["go.query.one_to_many"]

Filters on parent and child through the relation.

# `go.query.one_to_many.filter.related-id`: Filtering by related docID

- **Parents:** ["go.query.one_to_many.filter"]
- **Go-Tests:** ["query/one_to_many/with_filter_related_id_test.go::TestQueryFromManySideWithEqFilterOnRelatedType", "query/one_to_many/with_filter_related_id_test.go::TestQueryFromManySideWithFilterOnRelatedObjectID", "query/one_to_many/with_filter_related_id_test.go::TestQueryFromManySideWithSameFiltersInDifferentWayOnRelatedType", "query/one_to_many/with_filter_related_id_test.go::TestQueryFromSingleSideWithEqFilterOnRelatedType", "query/one_to_many/with_filter_related_id_test.go::TestQueryFromSingleSideWithFilterOnRelatedObjectID_Error"]

`Book(filter: {author: {_docID: {_eq: X}}})`, `Book(filter: {_authorID: {_eq: X}})` and both together return the books of author X; `Author(filter: {published: {_docID: {_eq: B}}})` returns the author of book B. `Author(filter: {_publishedID: ...})` errors with `Unknown field.`.

# `go.query.one_to_many.filter.parent-child`: Parent filters, child filters and compound relation filters

- **Parents:** ["go.query.one_to_many.filter"]
- **Go-Tests:** ["query/one_to_many/with_filter_test.go::TestQueryOneToManyWithNumericGreaterThanFilterOnParent", "query/one_to_many/with_filter_test.go::TestQueryOneToManyWithNumericGreaterThanChildFilterOnParentWithUnrenderedChild", "query/one_to_many/with_filter_test.go::TestQueryOneToManyWithNumericGreaterThanFilterOnParentAndChild", "query/one_to_many/with_filter_test.go::TestQueryOneToManyWithMultipleAliasedFilteredChildren", "query/one_to_many/with_filter_test.go::TestQueryOneToManyWithCompoundOperatorInFilterAndRelation", "query/one_to_many/with_filter_test.go::TestQueryOneToMany_WithCompoundOperatorInFilterAndRelationAndCaseInsensitiveLike_NoError", "query/one_to_many/with_filter_test.go::TestQueryOneToMany_WithAliasFilterOnRelated_Succeeds"]

A parent filter on a child field (`Author(filter: {published: {rating: {_gt: 4.8}}})`, also via an alias `{_alias: {books: {...}}}`) keeps authors with at least one matching book but still renders all their books; a filter on the child selection (`published(filter: ...)`) narrows only the rendered list, and two aliased children with different filters are independent. Relation conditions nest in `_and`, `_or` and `_not` (e.g. `_and: [{_not: {published: {rating: {_gt: 4.8}}}}, {_not: {published: {rating: {_lt: 4.8}}}}]` returns only the author whose only book is rated 4.8) and accept `_like` / `_ilike`.

# `go.query.one_to_many.child-docid`: Selecting children by docID

- **Parents:** ["go.query.one_to_many"]
- **Go-Tests:** ["query/one_to_many/with_doc_id_test.go::TestQueryOneToManyWithChildDocID", "query/one_to_many/with_doc_ids_test.go::TestQueryOneToManyWithChildDocIDs"]

`published(docID: X)` renders only that child and `published(docID: [X, Y])` only those children.

# `go.query.one_to_many.time-travel`: Querying a parent at a commit with its relation

- **Parents:** ["go.query.one_to_many"]
- **Go-Tests:** ["query/one_to_many/with_cid_doc_id_test.go::TestQueryOneToManyWithCidAndDocID", "query/one_to_many/with_cid_doc_id_test.go::TestQueryOneToManyWithChildUpdateAndFirstCidAndDocID", "query/one_to_many/with_cid_doc_id_test.go::TestQueryOneToManyWithParentUpdateAndFirstCidAndDocID", "query/one_to_many/with_cid_doc_id_test.go::TestQueryOneToManyWithParentUpdateAndLastCidAndDocID"]
- **Go-Bug-Suspect:** "TestQueryOneToManyWithChildUpdateAndFirstCidAndDocID is marked 'for documentation reasons only. This is not desired behaviour': the related author renders at its latest state (age 22), not as of the book's commit"

`Book(cid: C, docID: X) { author { ... } }` returns the book's fields as of commit C (first CID gives rating 4.9 after an update to 4.5; last CID gives 4.5) while the related author is rendered at its current state.

# `go.query.one_to_many.child-order-paging`: Ordering and limiting rendered children

- **Parents:** ["go.query.one_to_many"]
- **Go-Tests:** ["query/one_to_many/with_limit_test.go::TestQueryOneToManyWithSingleChildLimit", "query/one_to_many/with_limit_test.go::TestQueryOneToManyWithMultipleChildLimits", "query/one_to_many/with_order_filter_test.go::TestQueryOneToManyWithNumericGreaterThanFilterOnParentAndNumericSortAscendingOnChild", "query/one_to_many/with_order_filter_test.go::TestQueryOneToManyWithNumericGreaterThanFilterAndNumericSortDescendingOnChild", "query/one_to_many/with_order_filter_limit_test.go::TestQueryOneToManyWithNumericGreaterThanFilterOnParentAndNumericSortAscendingAndLimitOnChild", "query/one_to_many/with_order_filter_limit_test.go::TestQueryOneToManyWithNumericGreaterThanFilterOnParentAndNumericSortDescendingAndLimitOnChild"]

`published(order: {rating: ASC|DESC}, limit: N)` sorts and truncates each parent's child list independently of the parent filter and order; aliased children (`p1: published(limit: 1)`, `p2: published(limit: 2)`) apply their own limits.

# `go.query.one_to_many.count`: COUNT over a one-to-many relation

- **Parents:** ["go.query.one_to_many"]

`COUNT(published: {...})` on the one side.

# `go.query.one_to_many.count.basic`: Plain COUNT, alias filter and alias order

- **Parents:** ["go.query.one_to_many.count"]
- **Go-Tests:** ["query/one_to_many/with_count_test.go::TestQueryOneToMany_WithCount_NothingToCount", "query/one_to_many/with_count_test.go::TestQueryOneToMany_WithCount_ShouldMatchAll", "query/one_to_many/with_count_test.go::TestQueryOneToMany_WithCountAliasFilter_ShouldMatchAll", "query/one_to_many/with_count_test.go::TestQueryOneToMany_WithCountAliasFilter_ShouldMatchOne", "query/one_to_many/with_count_order_test.go::TestQueryOneToMany_WithCountAliasOrder_ShouldOrderResults"]

`COUNT(published: {})` returns the number of related books (0 when none). An aliased count can filter parents (`Author(filter: {_alias: {publishedCount: {_gt: 1}}})`) and order them (`order: {_alias: {publishedCount: DESC}}`).

# `go.query.one_to_many.count.filter`: COUNT with a child filter

- **Parents:** ["go.query.one_to_many.count"]
- **Go-Tests:** ["query/one_to_many/with_count_filter_test.go::TestQueryOneToManyWithCountWithFilter", "query/one_to_many/with_count_filter_test.go::TestQueryOneToManyWithCountWithFilterAndChildFilter", "query/one_to_many/with_count_filter_test.go::TestQueryOneToMany_WithCountWithJSONFilterAndChildFilter_Succeeds"]

`COUNT(published: {filter: {rating: {_gt: 4.8}}})` counts only matching children (`_neq: null` excludes books without a rating) and coexists with a separately filtered child selection; a top-level `COUNT(Author: {filter: {metadata: {yearOfBirth: {_eq: 1958}}, published: {name: {_ilike: "%lord%"}}}})` combines a JSON path filter with a relation filter (returns 1).

# `go.query.one_to_many.count.groupby`: COUNT of grouped children

- **Parents:** ["go.query.one_to_many.count"]
- **Go-Tests:** ["query/one_to_many/with_count_groupby_test.go::TestQueryOneToMany_WithCountWithGroupBy_CountingZero", "query/one_to_many/with_count_groupby_test.go::TestQueryOneToMany_WithCountWithGroupBy_CountingDistinctNames", "query/one_to_many/with_count_groupby_test.go::TestQueryOneToMany_WithCountWithGroupBy_AllSameName"]

`COUNT(published: {groupBy: [name]})` counts distinct child groups: 0 with no books, 2 for three books with two distinct names, 1 when all share a name.

# `go.query.one_to_many.count.paging`: COUNT with limit and offset

- **Parents:** ["go.query.one_to_many.count"]
- **Go-Tests:** ["query/one_to_many/with_count_limit_test.go::TestQueryOneToManyWithCountAndLimit", "query/one_to_many/with_count_limit_test.go::TestQueryOneToManyWithCountAndDifferentLimits", "query/one_to_many/with_count_limit_test.go::TestQueryOneToManyWithCountWithLimit", "query/one_to_many/with_count_limit_offset_test.go::TestQueryOneToManyWithCountAndLimitAndOffset", "query/one_to_many/with_count_limit_offset_test.go::TestQueryOneToManyWithCountAndDifferentOffsets", "query/one_to_many/with_count_limit_offset_test.go::TestQueryOneToManyWithCountWithLimitWithOffset"]

`COUNT(published: {limit: N, offset: M})` counts at most N children after skipping M, independently of the `limit`/`offset` on a rendered `published(...)` selection (an unbounded `COUNT(published: {})` still reports 4 while `published(limit: 2, offset: 1)` renders 2).

# `go.query.one_to_many.aggregate`: SUM, AVG, MIN, MAX over a one-to-many relation

- **Parents:** ["go.query.one_to_many"]

`AGG(published: {field: rating, ...})`.

# `go.query.one_to_many.aggregate.alias`: Aggregates used through aliases

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Go-Tests:** ["query/one_to_many/with_average_test.go::TestQueryOneToMany_WithAverageAliasFilter_ShouldMatchAll", "query/one_to_many/with_average_test.go::TestQueryOneToMany_WithAverageAliasFilter_ShouldMatchOne", "query/one_to_many/with_max_test.go::TestQueryOneToMany_WithMaxAliasFilter_ShouldMatchAll", "query/one_to_many/with_max_test.go::TestQueryOneToMany_WithMaxAliasFilter_ShouldMatchOne", "query/one_to_many/with_min_test.go::TestQueryOneToMany_WithMinAliasFilter_ShouldMatchAll", "query/one_to_many/with_min_test.go::TestQueryOneToMany_WithMinAliasFilter_ShouldMatchOne", "query/one_to_many/with_sum_test.go::TestQueryOneToMany_WithSumAliasFilter_ShouldMatchAll", "query/one_to_many/with_sum_test.go::TestQueryOneToMany_WithSumAliasFilter_ShouldMatchOne", "query/one_to_many/with_sum_test.go::TestQueryOneToMany_WithSumAliasFilterOnFloat32_ShouldMatchOne", "query/one_to_many/with_sum_order_test.go::TestQueryOneToMany_WithSumWithAliasOrder_ShouldOrderResults"]
- **Go-Bug-Suspect:** "SUM over a 'Float32' field is asserted as 9.400000095367432 (float32 values widened to float64 before summing), not 9.4"

`averageRating: AVG(published: {field: rating})` (and MIN, MAX, SUM) returns the per-author aggregate (AVG 4.7 for 4.9 and 4.5; MIN 4.5; MAX 4.9; SUM 9.4), and the alias can filter authors (`filter: {_alias: {totalRating: {_gt: 5}}}`) or order them (`order: {_alias: {totalRating: DESC}}`).

# `go.query.one_to_many.aggregate.sum-ordered-paging`: SUM with order, limit, offset and parent filters

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Go-Tests:** ["query/one_to_many/with_sum_filter_order_test.go::TestOneToManyAscOrderAndFilterOnParentWithAggSumOnSubTypeField", "query/one_to_many/with_sum_filter_order_test.go::TestOneToManyDescOrderAndFilterOnParentWithAggSumOnSubTypeField", "query/one_to_many/with_sum_filter_order_test.go::TestOnetoManySumBySubTypeFieldAndSumBySybTypeFieldWithDescOrderingOnFieldWithLimit", "query/one_to_many/with_sum_filter_order_test.go::TestOnetoManySumBySubTypeFieldAndSumBySybTypeFieldWithAscOrderingOnFieldWithLimit", "query/one_to_many/with_sum_filter_order_test.go::TestOneToManyLimitAscOrderSumOfSubTypeAndLimitAscOrderFieldsOfSubtype", "query/one_to_many/with_sum_filter_order_test.go::TestOneToManyLimitDescOrderSumOfSubTypeAndLimitAscOrderFieldsOfSubtype", "query/one_to_many/with_sum_limit_offset_order_test.go::TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderAsc", "query/one_to_many/with_sum_limit_offset_order_test.go::TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderDesc", "query/one_to_many/with_sum_limit_offset_order_test.go::TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderAscAndDesc", "query/one_to_many/with_sum_limit_offset_order_test.go::TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderOnDifferentFields", "query/one_to_many/with_sum_limit_offset_order_test.go::TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderDescAndRenderedChildrenOrderedAsc"]

`SUM(published: {field: rating, order: {name|rating: ASC|DESC}, offset: M, limit: N})` sums the children in that window (DESC by rating, limit 2 gives 4.9 + 4.5); several aliased SUMs with different windows, and a rendered `published(...)` with its own order and window, are evaluated independently. A SUM over no children is 0.0, and parent `filter` and `order` apply to the authors as usual.

# `go.query.one_to_many.aggregate.sum-unordered-paging`: SUM with limit and offset but no order

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Go-Tests:** ["query/one_to_many/with_sum_limit_test.go::TestQueryOneToManyWithSumWithLimit", "query/one_to_many/with_sum_limit_offset_test.go::TestQueryOneToManyWithSumWithLimitAndOffset"]
- **Go-Bug-Suspect:** "without 'order' the window depends on internal child ordering (the test comment says 'internal ordering selects Painted House + A Time for Mercy'); the expected sums 9.4 and 8.7 encode docID order, not any defined order"

`SUM(published: {field: rating, limit: 2})` and `{offset: 1, limit: 2}` sum the children in Go's internal (docID) order: 9.4 and 8.7 for John Grisham, and 4.8 / 0 for an author with one book.

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

# `go.query.one_to_one.join`: Resolving the relation in both directions

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/simple_test.go::TestQueryOneToOne_PrimaryDirection", "query/one_to_one/simple_test.go::TestQueryOneToOne_SecondaryDirection", "query/one_to_one/simple_test.go::TestQueryOneToOneWithMultipleRecords", "query/one_to_one/simple_test.go::TestQueryOneToOneWithMultipleRecordsSecondaryDirection", "query/one_to_one/simple_test.go::TestQueryOneToOneWithNilChild", "query/one_to_one/simple_test.go::TestQueryOneToOneWithNilParent", "query/one_to_one/with_fragments_test.go::TestQueryOneToOne_WithFragment", "query/one_to_one/with_fragments_test.go::TestQueryOneToOne_WithFragmentWithObjectWithFragment"]

From either side the relation renders as a single object (not a list) for each of several linked pairs, and as null when no counterpart exists. Relation selections work inside named fragments, including a fragment nested in another fragment's object.

# `go.query.one_to_one.relation-id`: Relation ID fields

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/simple_test.go::TestQueryOneToOne_WithRelationIDFromPrimarySide", "query/one_to_one/simple_test.go::TestQueryOneToOne_WithRelationIDFromSecondarySide", "query/one_to_one/with_clashing_id_field_test.go::TestQueryOneToOneWithClashingIdFieldOnSecondary", "query/one_to_one/with_clashing_id_field_test.go::TestQueryOneToOneWithClashingIdFieldOnPrimary"]

Both sides of a one-to-one expose the counterpart's docID: `Author { _publishedID }` on the primary side and `Book { _authorID }` on the secondary side. Declaring a field `_authorID` next to `author: Author` fails with `duplicate field. Name: _authorID` whichever side is primary.

# `go.query.one_to_one.filter`: Filtering across the relation

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_filter_test.go::TestQueryOneToOneWithNumericFilterOnParent", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithStringFilterOnChild", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithBooleanFilterOnChild", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithFilterThroughChildBackToParent", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithBooleanFilterOnChildWithNoSubTypeSelection", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithCompoundAndFilterThatIncludesRelation", "query/one_to_one/with_filter_test.go::TestQueryOneToOneWithCompoundOrFilterThatIncludesRelation", "query/one_to_one/with_filter_test.go::TestQueryOneToOne_WithCompoundFiltersThatIncludesRelation_ShouldReturnResults", "query/one_to_one/with_count_filter_test.go::TestQueryOneToOneWithCountWithCompoundOrFilterThatIncludesRelation"]

`Book(filter: {author: {verified: {_eq: true}}})` filters books by their author's fields even when `author` is not selected, a filter can round-trip (`{author: {published: {rating: {_eq: 4.9}}}}`), and `author(filter: ...)` filters the rendered child. Relation conditions combine with scalar ones under `_and`, `_or` and `_not` (a `_not` over an object means not-all of its conditions), also inside `COUNT(Book: {filter: ...})` (returns 2).

# `go.query.one_to_one.filter-relation-id`: Filtering on the secondary side's relation ID

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqNullFilterOnSecondaryRelationID_ShouldReturnOrphans", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithNeqNullFilterOnSecondaryRelationID_ShouldReturnRelated", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqFilterOnSecondaryRelationID_ShouldReturnRelated", "query/one_to_one/with_filter_secondary_relation_id_test.go::TestQueryOneToOne_WithEqNullFilterOnSecondaryRelationIDAndOtherField_ShouldReturnMatchingOrphan"]

With `Publisher { book: Book @primary }`, `Book(filter: {_publisherID: {_eq: null}})` returns the books no publisher points at, `_neq: null` and `_eq: <publisher docID>` return the linked book, and the null condition combines with other field conditions.

# `go.query.one_to_one.order`: Ordering by related fields

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildBooleanOrderDescending", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildBooleanOrderAscending", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildIntOrderDescendingWithNoSubTypeFieldsSelected", "query/one_to_one/with_order_test.go::TestQueryOneToOneWithChildIntOrderAscendingWithNoSubTypeFieldsSelected", "query/one_to_one/with_order_test.go::TestQueryOneToOne_WithAliasedChildIntOrderAscending_ShouldOrder", "query/one_to_one/with_order_test.go::TestQueryOneToOne_WithChildAliasedIntOrderAscending_ShouldOrder", "query/one_to_one/with_filter_order_test.go::TestOnetoOneSubTypeDscOrderByQueryWithFilterHavinghNoSubTypeSelections", "query/one_to_one/with_filter_order_test.go::TestOnetoOneSubTypeAscOrderByQueryWithFilterHavinghNoSubTypeSelections"]

`Book(order: {author: {age|verified: ASC|DESC}})` orders books by the related author's field whether or not `author` is selected and combined with a relation filter; ordering works through an aliased relation (`order: {_alias: {writer: {age: ASC}}}`) and an aliased child field (`order: {author: {_alias: {authorAge: ASC}}}`).

# `go.query.one_to_one.group`: Grouping by the one-to-one relation

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_group_related_id_test.go::TestQueryOneToOneWithGroupRelatedID", "query/one_to_one/with_group_related_id_test.go::TestQueryOneToOneWithGroupRelatedIDFromSecondaryWithoutGroup", "query/one_to_one/with_group_related_id_test.go::TestQueryOneToOneWithGroupRelatedIDFromSecondaryWithoutGroupWithJoin", "query/one_to_one/with_group_related_id_test.go::TestQueryOneToOneWithGroupRelatedIDFromSecondaryWithGroup", "query/one_to_one/with_group_related_id_test.go::TestQueryOneToOneWithGroupRelatedIDFromSecondaryWithGroupWithJoin", "query/one_to_one/with_group_related_id_alias_test.go::TestQueryOneToOneWithGroupRelatedIDAlias", "query/one_to_one/with_group_related_id_alias_test.go::TestQueryOneToOneWithGroupRelatedIDAliasFromSecondaryWithoutInnerGroup", "query/one_to_one/with_group_related_id_alias_test.go::TestQueryOneToOneWithGroupRelatedIDAliasFromSecondaryWithoutInnerGroupWithJoin", "query/one_to_one/with_group_related_id_alias_test.go::TestQueryOneToOneWithGroupRelatedIDAliasFromSecondaryWithInnerGroup", "query/one_to_one/with_group_related_id_alias_test.go::TestQueryOneToOneWithGroupRelatedIDAliasFromSecondaryWithInnerGroupWithJoin"]

`Book(groupBy: [_authorID])` or `Book(groupBy: [author])` groups books per author whether `Book` or `Author` holds the primary side; the group row may render `_authorID`, the `author` object, and/or `GROUP`.

# `go.query.one_to_one.version`: `_version` alongside a relation

- **Parents:** ["go.query.one_to_one"]
- **Go-Tests:** ["query/one_to_one/with_version_test.go::TestQueryOneToOne_WithVersionOnOuterBeforeJoin", "query/one_to_one/with_version_test.go::TestQueryOneToOne_WithVersionOnOuterAfterJoin"]

`Book { name _version { docID } author { name } }` returns the book's version list with its docID and the related author regardless of whether `_version` is selected before or after the relation (with non-ASCII string values).

# `go.query.many_to_many`: Many-to-many through a join collection

- **Parents:** ["go.query"]

# `go.query.many_to_many.join-collection`: Querying through the join collection

- **Parents:** ["go.query.many_to_many"]
- **Go-Tests:** ["query/many_to_many/simple_test.go::TestManyToMany_QueryFromJoinCollection_ShouldSucceed", "query/many_to_many/with_nested_query_test.go::TestManyToMany_QueryFromSecondary_Succeeds"]

With `Enrollment { student: Student, course: Course }` (named relations), `Enrollment(filter: {course: {name: {_eq: "Math"}}}, order: {student: {name: ASC}})` returns enrolments in that order, and `Student { enrollments(order: {course: {name: ASC}}) { course { name } } }` traverses from the other side.

# `go.query.one_to_many_multiple`: Several one-to-many relations on one type

- **Parents:** ["go.query"]

`Author { books: [Book], articles: [Article] }`.

# `go.query.one_to_many_multiple.count`: COUNT across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Go-Tests:** ["query/one_to_many_multiple/with_count_test.go::TestQueryOneToManyMultipleWithCount", "query/one_to_many_multiple/with_count_test.go::TestQueryOneToManyMultipleWithCountOnMultipleJoins", "query/one_to_many_multiple/with_count_filter_test.go::TestQueryOneToManyMultipleWithCountOnMultipleJoinsWithAndWithoutFilter", "query/one_to_many_multiple/with_count_filter_test.go::TestQueryOneToManyMultipleWithCountOnMultipleJoinsWithFilters"]

Separate `COUNT(books: {})` and `COUNT(articles: {})` count each relation; a single `COUNT(books: {...}, articles: {...})` adds both counts, each source applying its own filter.

# `go.query.one_to_many_multiple.sum`: SUM across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Go-Tests:** ["query/one_to_many_multiple/with_sum_test.go::TestQueryOneToManyMultipleWithSumOnMultipleJoins", "query/one_to_many_multiple/with_sum_filter_test.go::TestQueryOneToManyMultipleWithSumOnMultipleJoinsWithAndWithoutFilter", "query/one_to_many_multiple/with_sum_filter_test.go::TestQueryOneToManyMultipleWithSumOnMultipleJoinsWithFilters"]

`SUM(books: {field: score}, articles: {field: rating})` returns one Int sum over both relations' fields (9 and 7), each source applying its own filter; with no matching children the sum is 0.

# `go.query.one_to_many_multiple.avg`: AVG across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Go-Tests:** ["query/one_to_many_multiple/with_average_test.go::TestQueryOneToManyMultipleWithAverageOnMultipleJoins", "query/one_to_many_multiple/with_average_filter_test.go::TestQueryOneToManyMultipleWithAverageOnMultipleJoinsWithAndWithoutFilter", "query/one_to_many_multiple/with_average_filter_test.go::TestQueryOneToManyMultipleWithAverageOnMultipleJoinsWithFilters"]

`AVG(books: {field: score}, articles: {field: rating})` averages over the union of selected values (2.25; 2.3333333333333335), each source applying its own filter; with no matching values it returns 0.

# `go.query.one_to_many_multiple.filter`: Filtering on several relations at once

- **Parents:** ["go.query.one_to_many_multiple"]
- **Go-Tests:** ["query/one_to_many_multiple/with_multiple_filter_test.go::TestQueryOneToManyMultipleWithMultipleManyFilters"]

`Author(filter: {name: {_eq: "John Grisham"}, books: {score: {_eq: 1}}, articles: {rating: {_eq: 3}}})` requires every condition, each relation condition satisfied by some child.

# `go.query.one_to_many_to_many`: One-to-many followed by one-to-many

- **Parents:** ["go.query"]

# `go.query.one_to_many_to_many.joins`: Two-hop many relations link correctly

- **Parents:** ["go.query.one_to_many_to_many"]
- **Go-Tests:** ["query/one_to_many_to_many/joins_test.go::TestOneToManyToManyJoinsAreLinkedProperly"]

`Author { book { publisher { ... } } }` with `Book.publisher: [Publisher]` renders each book's own publishers (zero, one or two) under the right author, and authors without books get an empty list.

# `go.query.one_to_many_to_one`: One-to-many followed by one-to-one

- **Parents:** ["go.query"]

Schema `Author { book: [Book] }`, `Book { publisher: Publisher }`, `Publisher { book: Book @primary }`.

# `go.query.one_to_many_to_one.joins`: Two-hop links

- **Parents:** ["go.query.one_to_many_to_one"]
- **Go-Tests:** ["query/one_to_many_to_one/joins_test.go::TestOneToManyToOneJoinsAreLinkedProperly", "query/one_to_many_to_one/simple_test.go::TestQueryOneToOneRelations"]

`Author { book { publisher { ... } } }` and `Book { author { ... } publisher { ... } }` render each book's own publisher, null for a book without one, and an empty list for an author without books.

# `go.query.one_to_many_to_one.deep-filter`: Filters two relations deep

- **Parents:** ["go.query.one_to_many_to_one"]
- **Go-Tests:** ["query/one_to_many_to_one/with_filter_test.go::TestQueryComplexWithDeepFilterOnRenderedChildren", "query/one_to_many_to_one/with_filter_test.go::TestOneToManyToOneWithTwoLevelDeepFilter", "query/one_to_many_to_one/with_filter_test.go::TestOneToManyToOneWithCompoundOperatorInFilterAndRelation", "query/one_to_many_to_one/with_filter_test.go::TestOneToManyToOneWithCompoundOperatorInSubFilterAndRelation"]

`Author(filter: {book: {publisher: {yearOpened: {_gt: 2021}}}})` keeps authors with at least one such book but renders all their books; deep conditions combine under `_and`, `_or`, `_not` both in the parent filter and in a `book(filter: ...)` child filter.

# `go.query.one_to_many_to_one.sum`: SUM with deep filters and deep ordering

- **Parents:** ["go.query.one_to_many_to_one"]
- **Go-Tests:** ["query/one_to_many_to_one/with_filter_test.go::TestOneToManyToOneWithSumOfDeepFilterSubTypeOfBothDescAndAsc", "query/one_to_many_to_one/with_filter_test.go::TestOneToManyToOneWithSumOfDeepFilterSubTypeAndDeepOrderBySubtypeOppositeDirections", "query/one_to_many_to_one/with_sum_order_limit_test.go::TestOneToManyToOneWithSumOfDeepOrderBySubTypeAndDeepOrderBySubtypeDescDirections", "query/one_to_many_to_one/with_sum_order_limit_test.go::TestOneToManyToOneWithSumOfDeepOrderBySubTypeAndDeepOrderBySubtypeAscDirections", "query/one_to_many_to_one/with_sum_order_limit_test.go::TestOneToManyToOneWithSumOfDeepOrderBySubTypeOfBothDescAndAsc", "query/one_to_many_to_one/with_sum_order_limit_test.go::TestOneToManyToOneWithSumOfDeepOrderBySubTypeAndDeepOrderBySubtypeOppositeDirections", "query/one_to_many_to_one/with_sum_test.go::TestQueryWithSumOnInlineAndSumOnOneToManyField"]

`SUM(book: {field: rating, filter: {publisher: {yearOpened: ...}}})` sums books whose publisher matches, and `SUM(book: {field: rating, order: {publisher: {yearOpened: ASC|DESC}}, limit: 2})` sums the first two books by publisher year, with books lacking a publisher sorting first ascending (John: DESC 4.8 + 4.5, ASC 4.2 + 4.9); the same window on a rendered `book(...)` selection agrees. Several such sums may be selected, under `query @exhaustive`, alongside a SUM over an inline array field.

# `go.query.one_to_many_to_one.order`: Ordering by a field two relations deep

- **Parents:** ["go.query.one_to_many_to_one"]
- **Go-Tests:** ["query/one_to_many_to_one/with_order_test.go::TestMultipleOrderByWithDepthGreaterThanOne", "query/one_to_many_to_one/with_order_test.go::TestMultipleOrderByWithDepthGreaterThanOneOrderSwitched", "query/one_to_many_to_one/with_order_limit_test.go::TestOneToManyToOneDeepOrderBySubTypeOfBothDescAndAsc"]
- **Scope:** "TestOneToManyToOneDeepOrderBySubTypeOfBothDescAndAsc excludes the secondary-index multiplier; TestMultipleOrderByWithDepthGreaterThanOneOrderSwitched uses 'query @exhaustive' so books without a publisher are kept under the secondary-index multiplier"

`Book(order: [{rating: ASC}, {publisher: {yearOpened: DESC}}])` and the switched order sort by multiple keys including a related field, with books lacking a publisher sorting last on `yearOpened: DESC`; `book(order: {publisher: {yearOpened: DESC|ASC}}, limit: 1)` under each author picks the newest or oldest (a book with no publisher counts as oldest).

# `go.query.one_to_one_multiple`: Several one-to-one relations on one type

- **Parents:** ["go.query"]

# `go.query.one_to_one_multiple.join`: Resolving two one-to-one relations

- **Parents:** ["go.query.one_to_one_multiple"]
- **Go-Tests:** ["query/one_to_one_multiple/simple_test.go::TestQueryOneToOneMultiple_FromPrimary", "query/one_to_one_multiple/simple_test.go::TestQueryOneToOneMultiple_FromMixedPrimaryAndSecondary", "query/one_to_one_multiple/simple_test.go::TestQueryOneToOneMultiple_FromSecondary"]

`Book { publisher { name } author { name } }` resolves both one-to-one relations whether `Book` is primary for both, one, or neither.

# `go.query.one_to_one_multiple.cyclic-schema`: Cyclic named relations are accepted

- **Parents:** ["go.query.one_to_one_multiple"]
- **Go-Tests:** ["query/one_to_one_multiple/simple_test.go::TestAddCollectionWithCyclicMutuallyReferentialRelations_DoesNotError"]

An SDL with mutually referential named relations (`Player`/`Deck`/`Game` with `deck_player`, `game_players`, `game_decks` and an unnamed `winner: Player`) is added without error.

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

# `go.query.one_to_two_many.named`: Named parallel relations

- **Parents:** ["go.query.one_to_two_many"]
- **Go-Tests:** ["query/one_to_two_many/simple_test.go::TestQueryOneToTwoManyWithNilUnnamedRelationship_FromOneSide", "query/one_to_two_many/simple_test.go::TestQueryOneToTwoManyWithNilUnnamedRelationship_FromManySide", "query/one_to_two_many/simple_test.go::TestQueryOneToTwoManyWithNamedAndUnnamedRelationships", "query/one_to_two_many/simple_test.go::TestQueryOneToTwoManyWithNamedAndUnnamedRelationships_FromManySide", "query/one_to_two_many/with_order_test.go::TestQueryOneToTwoManyWithOrder"]

With `Book.author` / `Author.written` (`written_books`) and `Book.reviewedBy` / `Author.reviewed` (`reviewed_books`), plus an unnamed `Book.price` / `Price.books`, each relation resolves to its own counterpart from both sides, and `written(order: {rating: ASC})` / `reviewed(order: {rating: DESC})` order each list independently.
