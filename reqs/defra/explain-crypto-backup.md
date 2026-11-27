# `go.explain.errors`: Invalid explain requests are rejected by GraphQL validation

- **Parents:** ["go.explain"]
- **Scenarios:** ["explain.debug.debug-explain-mutation-request-with-delete-having-no-sub-selection", "explain.debug.debug-explain-request-where-parent-is-ordered-by-its-related-child", "explain.default.default-explain-mutation-request-with-delete-having-no-sub-selection", "explain.default.default-explain-on-wrong-field-directive-bad-usage", "explain.default.default-explain-request-where-parent-is-ordered-by-its-related-child", "explain.default.invalid-explain-request-type-returns-error", "explain.execute.execute-explain-request-where-parent-field-is-ordered-by-child-field", "explain::simple::with_invalid_request::execute_explain_reports_a_run_time_failure_in_execution_errors"]

`@explain` on a field instead of the operation fails with `Directive "explain" may not be used on FIELD.`; `@explain(type: invalid)` fails with `Argument "type" has invalid value invalid.\nExpected type "ExplainType", found invalid.`; an explained `delete_Author(docID: [...])` with no sub-selection fails with `Field "delete_Author" of type "[Author]" must have a sub selection.`; ordering a parent by a one-to-many child (`order: {articles: {name: ASC}}`) fails with `Argument "order" has invalid value {articles: {name: ASC}}.\nIn field "articles": Unknown field.` in default, debug and execute modes.

An explained request is validated exactly as the request would be when run, in every explain type, and fails with the same top-level error instead of returning a plan (or, for `execute`, an `executionErrors` entry): an unknown filter field such as `Book(filter: {author: {name: {_like: "%vid%"}}})` fails with `Argument "filter" has invalid value {author: {name: {_like: "%vid%"}}}.\nIn field "author": Unknown field.`, also as an explained `update_Book` filter; an unknown filter operator fails with `In field "genre": In field "_eqq": Unknown field.`; selecting a field the collection does not define fails with `Cannot query field "author" on type "Book".`; a sub-selection on a scalar fails with `Field "title" of type "String!" must not have a sub selection.`; and an aggregate argument naming no field of the type, such as `COUNT(missing: {})` on `Publisher`, fails with `Unknown argument "missing" on field "COUNT" of type "Publisher".`. An `execute` explain reports in `executionErrors` only a failure of a request that passed validation and then ran: `Users(filter: {Custom: {_gt: false}})` on a `JSON` field validates and then fails while its plan scans, returning `executionSuccess: false`, an empty `operationNode`, and `unexpected type. Property: condition, Actual: bool` as the only `executionErrors` entry.

# `go.explain.execute.scan`: Scan counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-query-deleted-docs", "explain.execute.execute-explain-request-with-all-documents-matching", "explain.execute.execute-explain-request-with-documents-but-no-matches", "explain.execute.execute-explain-request-with-no-documents", "explain.execute.execute-explain-request-with-some-documents-matching"]

With no docs, all docs matching, some matching, or none matching a filter, `selectNode` reports `iterations` (rows + 1) and `filterMatches`, and `scanNode` reports `iterations`, `docFetches`, `fieldFetches` and `indexFetches: 0`; `showDeleted: true` counts deleted documents too.

# `go.explain.execute.mutation`: Mutation counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-mutation-request-with-add", "explain.execute.execute-explain-mutation-request-with-delete-using-filter", "explain.execute.execute-explain-mutation-request-with-delete-using-id", "explain.execute.execute-explain-mutation-request-with-update-using-filter", "explain.execute.execute-explain-mutation-request-with-update-using-i-ds", "explain.execute.execute-explain-mutation-request-with-upsert-and-matching-filter-succeeds", "explain.execute.execute-explain-mutation-request-with-upsert-and-no-matching-filter-succeeds"]
- **Go-Bug-Suspect:** "'upsert_ContactAddress' whose filter matches one of two documents reports 'selectNode{iterations: 4, filterMatches: 2}', counting the matched document twice."

Explained `add_`, `delete_` (by docID or filter), `update_` (by docIDs or filter) and `upsert_` (matching or not) run the mutation and report the counts of the mutation node's inner select and scan.

# `go.explain.execute.dagscan`: Commit scan counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-commits-dag-scan"]

`_commits(docID: ...)` under execute explain reports `dagScanNode` iteration counts and the select node's `filterMatches` for the document's commits.

# `go.explain.execute.shaping`: Group, limit and order counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-request-with-both-limit-and-offset-on-parent", "explain.execute.execute-explain-request-with-both-limit-and-offset-on-parent-and-limit-on-child", "explain.execute.execute-explain-request-with-group", "explain.execute.execute-explain-request-with-multi-order-fields-on-parent", "explain.execute.execute-explain-request-with-order-field-on-both-parent-and-child", "explain.execute.execute-explain-request-with-order-field-on-child", "explain.execute.execute-explain-request-with-order-field-on-parent", "explain.execute.execute-explain-request-with-subquery-order-by-nested-relation-field", "explain.execute.execute-explain-request-with-subquery-order-by-nested-relation-field-asc"]

`groupBy`, parent and child `limit`/`offset`, and parent, child and nested-relation `order` report counters on the `groupNode`, `limitNode` and `orderNode` and the scans below them.

# `go.explain.execute.aggregate`: Aggregate counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-average-request-on-array-field", "explain.execute.execute-explain-request-max-of-related-one-to-many-field-succeeds", "explain.execute.execute-explain-request-min-of-related-one-to-many-field-succeeds", "explain.execute.execute-explain-request-sum-of-related-one-to-many-field", "explain.execute.execute-explain-request-with-count-on-one-to-many-relation", "explain.execute.execute-explain-request-with-max-of-inline-array-field-succeeds", "explain.execute.execute-explain-request-with-min-of-inline-array-field-succeeds", "explain.execute.execute-explain-request-with-sum-of-inline-array-field", "explain.execute.execute-explain-top-level-average-request", "explain.execute.execute-explain-top-level-count-request", "explain.execute.execute-explain-top-level-sum-request", "explain.execute.explain-explain-average-request-on-joined-field"]

Top-level `COUNT`/`SUM`/`AVG`, and `COUNT`/`SUM`/`AVG`/`MIN`/`MAX` over inline arrays and one-to-many relations, report iteration counts on the aggregate nodes and the fetch counters of the scans and joins below.

# `go.explain.execute.join`: Join counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-request-with-a-one-to-one-join", "explain.execute.execute-explain-with-multiple-one-to-one-joins", "explain.execute.execute-explain-with-one-to-one-join-from-secondary-side-should-include-index", "explain.execute.execute-explain-with-two-level-deep-nested-joins"]

One-to-one joins (single, two in parallel, two levels deep, and from the secondary side) report `typeIndexJoin` iterations and the fetch counters of both sides; joining from the secondary side reports `indexFetches` from the relation index.

# `go.explain.execute.index`: Index fetch counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-with-index-on-filter", "explain.execute.execute-explain-with-index-on-order", "explain.execute.execute-explain-with-index-on-relation-order", "explain.execute.execute-explain-with-index-on-subquery-nested-relation-order"]

With `@index` on `age`, filtering `age: {_eq: 30}` over three docs reports `scanNode{iterations: 3, docFetches: 2, fieldFetches: 4, indexFetches: 2}`; ordering by the indexed field, by an indexed relation field with `@exhaustive`, and a child ordered by an indexed related field report index fetches instead of a sort.

# `defra.explain.index_name`: Index scans name their index in Rust plans

- **Parents:** ["defra"]
- **Scenarios:** ["explain.execute.index_name.a_rust_index_scan_names_its_index", "explain.execute.index_name.a_rust_direct_relation_lookup_names_its_index"]

The Rust execution explain includes `indexName` on an index-backed `scanNode`. A direct foreign-key lookup names the index it uses; otherwise the child scan retains its index name. This is a deliberate Rust-only addition to the Go execute-explain shape.

# `go.explain.execute.orphan`: Orphan node counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-with-orphan-node-with-primary-parent-reports-metrics", "explain.execute.execute-explain-with-orphan-node-with-secondary-parent-reports-metrics"]

With `@exhaustive` and ordering by a relation field, the `orphanNode` reports `iterations`, `docFetches`, `fieldFetches` and `indexFetches` for both the primary-parent and secondary-parent paths.

# `go.explain.execute.similarity`: Similarity counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-request-with-similarity"]

`SIMILARITY(pointsList: {vector: [1, 2, 0]})` reports `similarityNode` iterations above the select and scan counters.

# `go.explain.default.basic`: Plain select plan

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-request-with-full-basic-graph", "explain.default.default-explain-with-alias", "explain.simple.simple-explain-request"]

`query @explain { Author { name age } }` (and the same with `@explain(type: simple)` or with aliased fields) returns exactly `selectTopNode > selectNode{docID: nil, filter: nil} > scanNode{filter: nil, collectionName: "Author", collectionID: <CID>, prefixes: ["/3"]}`.

# `go.explain.default.mutation.delete`: Delete plan nodes

- **Parents:** ["go.explain.default.mutation"]
- **Scenarios:** ["explain.default.default-explain-mutation-request-with-delete-using-filter", "explain.default.default-explain-mutation-request-with-delete-using-filter-and-ids", "explain.default.default-explain-mutation-request-with-delete-using-filter-to-match-everything", "explain.default.default-explain-mutation-request-with-delete-using-id", "explain.default.default-explain-mutation-request-with-delete-using-ids", "explain.default.default-explain-mutation-request-with-delete-using-no-ids"]
- **Go-Bug-Suspect:** "'delete_Author(docID: [])' is explained with 'docID: []' and a scan over the whole collection prefix '/3', so an empty docID list appears to mean 'all documents' rather than 'none'."

`delete_Author` under `@explain` returns a `deleteNode` whose `filter` and `docID` attributes echo the arguments (a single docID is shown as a one-element list) over a `scanNode` for the collection; the scan carries the filter and is limited to the requested docIDs. Two empty arguments differ: `filter: {}` is shown as `filter: null` on both nodes, and `docID: []` is shown as `docID: []` with a scan over the whole collection prefix.

# `go.explain.default.mutation.update`: Update plan nodes

- **Parents:** ["go.explain.default.mutation"]
- **Scenarios:** ["explain.default.default-explain-mutation-request-with-update-using-boolean-filter", "explain.default.default-explain-mutation-request-with-update-using-id", "explain.default.default-explain-mutation-request-with-update-using-ids", "explain.default.default-explain-mutation-request-with-update-using-ids-and-filter"]

`update_Author(filter|docID, input)` under `@explain` returns an `updateNode` with `filter`, `docID` (list) and `input` attributes over a `scanNode` that carries the filter and is limited to the given docIDs.

# `go.explain.default.dagscan`: Commit query plan

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-commits-dag-scan-query-op", "explain.default.default-explain-commits-dag-scan-query-op-without-field"]

`_commits(docID: ..., filter: {fieldName: ...})` under `@explain` returns a `dagScanNode` whose `cid` is nil and whose `prefixes` name the document (and the field, when filtered by `fieldName`).

# `go.explain.default.filter`: Filter pushdown into the scan

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-request-with-greater-than-filter", "explain.default.default-explain-request-with-integer-equal-filter", "explain.default.default-explain-request-with-json-equal-filter-succeeds", "explain.default.default-explain-request-with-logical-compound-and-filter", "explain.default.default-explain-request-with-logical-compound-or-filter", "explain.default.default-explain-request-with-match-inside-list", "explain.default.default-explain-request-with-string-equal-filter", "explain::default::filter_applied_once::explain_filter_with_order_and_limit_is_applied_only_at_the_scan"]

A collection `filter` (string or integer `_eq`, `_gt`, `_and`, `_or`, `_in`, a JSON `_eq` on an object) appears verbatim as the `filter` attribute of the `scanNode`, with the collection prefix in `prefixes`. The filter is applied once, at the scan: the `selectNode` above it shows `filter: null`, also when the request adds an `order` and a `limit`.

# `go.explain.default.docid`: DocID selection

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-request-with-doc-i-ds-filter-using-multiple-but-duplicate-i-ds", "explain.default.default-explain-request-with-doc-i-ds-filter-using-multiple-unique-i-ds", "explain.default.default-explain-request-with-doc-i-ds-filter-using-one-id", "explain.default.default-explain-request-with-doc-id-filter", "explain.default.default-explain-request-with-matching-id-filter"]

`docID: "..."` or `docID: [...]` (duplicates kept as given) is shown on the `selectNode` as a docID list with an empty-prefix `scanNode`; filtering on `_docID: {_eq: ...}` instead stays a scan `filter`.

# `go.explain.default.index`: Indexed filter and order plans

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-with-index-on-filter", "explain.default.default-explain-with-index-on-order", "explain.default.default-explain-with-index-on-subquery-nested-relation-order", "explain.simple.simple-explain-with-index-on-filter", "explain.simple.simple-explain-with-index-on-order", "explain.simple.simple-explain-with-index-on-subquery-nested-relation-order", "explain::default::with_index_shape::indexed_scan_explains_the_selected_index", "explain::default::with_index_shape::go_indexed_scan_explains_the_selected_index", "explain::default::with_index_shape::explain_relation_filter_uses_the_indexed_child_scan", "explain::default::with_index_shape::simple_indexed_scan_reports_no_index_fetches", "explain::default::with_index_shape::go_simple_indexed_scan_reports_no_index_fetches"]

On a collection with `@index` on `age`, filtering on or ordering by `age` explains as `selectTopNode > selectNode > scanNode` with no `orderNode`. The plan scan names the collection and shows the selecting filter and root prefix. Rust also includes the selected index (`indexName`, `indexLimit`, `indexOffset`); Go omits those fields. Neither reports `indexFetches`, a runtime count only `execute` explain shows. Ordering a child by an indexed field of a related collection with `limit: 2` puts a `limitNode` but no `orderNode` in the child plan. Holds for default and `simple` types.

# `go.explain.default.group.selection`: Group node attributes for filters and docIDs

- **Parents:** ["go.explain.default.group"]
- **Scenarios:** ["explain.default.default-explain-request-with-doc-i-ds-and-filter-on-parent-group-by", "explain.default.default-explain-request-with-doc-i-ds-on-inner-group-selection", "explain.default.default-explain-request-with-doc-id-on-parent-group-by", "explain.default.default-explain-request-with-filter-on-group-by-parent", "explain.default.default-explain-request-with-filter-on-inner-group-selection", "explain.default.default-explain-request-with-filter-on-parent-group-by-and-inner-group-selection", "explain.default.default-explain-request-with-group-by-on-parent", "explain.default.default-explain-request-with-group-by-two-fields-on-parent"]

`groupBy: [age]` (or two fields) yields a `groupNode{groupByFields, childSelects}`; a parent `filter` or `docID` is pushed to the `scanNode` (`filter`, `prefixes`), while a `filter` or `docID` on the inner `GROUP` selection appears in the `childSelects` entry.

# `go.explain.default.group.limit`: Group plans with limit and offset

- **Parents:** ["go.explain.default.group"]
- **Scenarios:** ["explain.default.default-explain-request-with-limit-and-offset-on-inner-group-selection", "explain.default.default-explain-request-with-limit-and-offset-on-multiple-inner-group-selections", "explain.default.default-explain-request-with-limit-and-offset-on-parent-group-by", "explain.default.default-explain-request-with-limit-on-parent-group-by-and-inner-group-selection"]

`limit`/`offset` on a grouped parent add a `limitNode{limit, offset}` above the `groupNode`; `limit`/`offset` on inner `GROUP` selections (one or several aliased) appear in each `childSelects` entry.

# `go.explain.default.group.order`: Group plans with ordering

- **Parents:** ["go.explain.default.group"]
- **Scenarios:** ["explain.default.default-explain-request-with-ascending-order-on-inner-group-selection", "explain.default.default-explain-request-with-ascending-order-on-parent-group-by", "explain.default.default-explain-request-with-descending-order-on-inner-group-selection", "explain.default.default-explain-request-with-descending-order-on-parent-group-by", "explain.default.default-explain-request-with-order-on-nested-parent-group-by-and-on-nested-parents-inner-group-selection", "explain.default.default-explain-request-with-order-on-parent-group-by-and-on-inner-group-selection"]

`order` on a grouped parent adds an `orderNode{orderings: [{direction, fields}]}`; `order` on an inner `GROUP` (including nested group-bys) appears in the matching `childSelects` entry's `orderBy`.

# `go.explain.default.group.average`: Averages over groups

- **Parents:** ["go.explain.default.group"]
- **Scenarios:** ["explain.default.default-explain-request-with-average-inside-the-inner-group-and-nested-group-by-with-average", "explain.default.default-explain-request-with-average-inside-the-inner-group-on-a-field", "explain.default.default-explain-request-with-average-inside-the-inner-group-on-a-field-and-nested-group-by", "explain.default.default-explain-request-with-group-by-with-average-on-an-inner-field"]

`AVG(GROUP: {field: ...})`, including averages inside nested `GROUP(groupBy: ...)` selections, explain as `averageNode > countNode > sumNode > groupNode`, with `sources` naming the `GROUP` field and child field.

# `go.explain.default.aggregate.top_level`: Top-level aggregates

- **Parents:** ["go.explain.default.aggregate"]
- **Scenarios:** ["explain.default.default-explain-top-level-average-request", "explain.default.default-explain-top-level-average-request-with-filter", "explain.default.default-explain-top-level-count-request", "explain.default.default-explain-top-level-count-request-with-filter", "explain.default.default-explain-top-level-sum-request", "explain.default.default-explain-top-level-sum-request-with-filter", "explain.default.default-explain-with-top-level-max-request-succeeds", "explain.default.default-explain-with-top-level-max-request-with-filter-succeeds", "explain.default.default-explain-with-top-level-min-request-succeeds", "explain.default.default-explain-with-top-level-min-request-with-filter-succeeds"]

`COUNT(Author: {...})`, `SUM`, `AVG`, `MIN` and `MAX(Author: {field: age, filter?})` explain as the aggregate node (`AVG` as `averageNode > countNode > sumNode`) over a `scanNode` of `Author`; the optional filter appears in both the aggregate `sources` and the scan `filter`.

# `go.explain.default.aggregate.inline_array`: Aggregates over inline arrays

- **Parents:** ["go.explain.default.aggregate"]
- **Scenarios:** ["explain.default.default-explain-request-with-average-on-array-field", "explain.default.default-explain-request-with-count-on-inline-array-field", "explain.default.default-explain-request-with-max-on-inline-array-field-child-field-will-be-empty", "explain.default.default-explain-request-with-min-on-inline-array-field-child-field-will-be-empty", "explain.default.default-explain-request-with-sum-on-inline-array-field-child-field-will-be-empty"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX(chapterPages: {})` on a `[Int!]` field explain with `sources: [{fieldName: "chapterPages", childFieldName: nil, filter: nil}]` over a plain `scanNode` of `Book`.

# `go.explain.default.aggregate.join`: Aggregates over one-to-many relations

- **Parents:** ["go.explain.default.aggregate"]
- **Scenarios:** ["explain.default.default-explain-request-with-average-on-joined-field", "explain.default.default-explain-request-with-average-on-multiple-joined-fields-with-filter", "explain.default.default-explain-request-with-count-on-one-to-many-joined-field", "explain.default.default-explain-request-with-count-on-one-to-many-joined-field-with-many-sources", "explain.default.default-explain-request-with-max-on-one-to-many-joined-field-succeeds", "explain.default.default-explain-request-with-max-on-one-to-many-joined-field-with-filter-succeeds", "explain.default.default-explain-request-with-max-on-one-to-many-joined-field-with-many-sources-succeeds", "explain.default.default-explain-request-with-min-on-one-to-many-joined-field-succeeds", "explain.default.default-explain-request-with-min-on-one-to-many-joined-field-with-filter-succeeds", "explain.default.default-explain-request-with-min-on-one-to-many-joined-field-with-many-sources-succeeds", "explain.default.default-explain-request-with-sum-on-one-to-many-joined-field", "explain.default.default-explain-request-with-sum-on-one-to-many-joined-field-with-filter", "explain.default.default-explain-request-with-sum-on-one-to-many-joined-field-with-many-sources"]

`COUNT(books: {})`, `SUM`/`AVG`/`MIN`/`MAX(books: {field: pages}, articles: {...})` explain as the aggregate node over a `typeIndexJoin` (`joinType: typeJoinMany`); a source filter appears in `sources`, and several sources produce a `parallelNode` of joins.

# `go.explain.default.aggregate.shared_join`: Aggregate and child selection sharing a join

- **Parents:** ["go.explain.default.aggregate"]
- **Scenarios:** ["explain.default.default-explain-request-one-to-many-with-average-and-child-ne-nil-filter-shares-join-field", "explain.default.default-explain-request-one-to-many-with-count-and-child-filter-does-not-share-join-field", "explain.default.default-explain-request-one-to-many-with-count-with-filter-and-child-filter-shares-join-field"]

When an aggregate over `books` and a `books(filter: ...)` child selection use the same filter (`AVG(books: {field: rating})` with child filter `rating: {_neq: null}`, or `COUNT(books: {filter: {rating: {_neq: null}}})`), the plan has one `typeIndexJoin` under the aggregate; when the filters differ (`COUNT(books: {})` vs a filtered child) the plan has a `parallelNode` with two joins.

# `go.explain.default.join.structure`: Join node shape

- **Parents:** ["go.explain.default.join"]
- **Scenarios:** ["explain.default.default-explain-request-with-a-one-to-many-join", "explain.default.default-explain-request-with-a-one-to-one-join", "explain.default.default-explain-request-with-two-level-deep-nested-joins", "explain.default.default-explain-request-with2-single-joins-and1-many-join"]

A one-to-many child (`articles`) gives `typeIndexJoin{joinType: "typeJoinMany", rootName, subTypeName}` with `root` and `subType` plans; a one-to-one child (`contact`) gives `joinType: "typeJoinOne"`, nested two levels for `contact { address }`; several children give a `parallelNode` of joins.

# `go.explain.default.join.related_filter`: Filters on related fields

- **Parents:** ["go.explain.default.join"]
- **Scenarios:** ["explain.default.default-explain-request-with-many-related-filters", "explain.default.default-explain-request-with-many-related-filters-and-doc-id", "explain.default.default-explain-request-with-related-and-regular-filter", "explain.default.default-explain-request-with-related-and-regular-filter-and-doc-i-ds"]

A parent filter that mixes own fields with related fields (`books: {name: ...}`, `articles: {...}`), optionally with `docID`, keeps the whole filter on the parent `selectNode`, pushes only the own-field part to the parent `scanNode`, and adds one `typeIndexJoin` per related field (a `parallelNode` for several).

# `go.explain.default.limit.parent`: Limit on the root collection

- **Parents:** ["go.explain.default.limit"]
- **Scenarios:** ["explain.default.default-explain-request-with-limit-and-offset", "explain.default.default-explain-request-with-only-limit", "explain.default.default-explain-request-with-only-offset"]

`Author(limit: 2)`, `Author(offset: 2)` and `Author(limit: 3, offset: 1)` explain with `limitNode{limit, offset}` (`limit: nil` when only offset is given) above the `selectNode`.

# `go.explain.default.limit.child`: Limit on related children

- **Parents:** ["go.explain.default.limit"]
- **Scenarios:** ["explain.default.default-explain-request-with-both-limit-and-offset-on-related-child", "explain.default.default-explain-request-with-limit-args-on-parent-and-related-child-with-count", "explain.default.default-explain-request-with-limit-on-related-child-and-both-limit-and-offset-on-parent", "explain.default.default-explain-request-with-only-limit-on-related-child", "explain.default.default-explain-request-with-only-limit-on-related-child-with-count", "explain.default.default-explain-request-with-only-offset-on-related-child"]

`articles(limit: ..., offset: ...)` puts a `limitNode` in the join's `subType` plan, independently of a parent `limitNode`; combined with `COUNT(articles: {})` the plan has a `parallelNode` with the count's join and the limited child join.

# `go.explain.default.order`: Order plans

- **Parents:** ["go.explain.default"]
- **Scenarios:** ["explain.default.default-explain-request-with-ascending-order-on-parent", "explain.default.default-explain-request-with-multi-order-fields-on-parent", "explain.default.default-explain-request-with-order-field-on-parent-and-related-child", "explain.default.default-explain-request-with-order-field-on-related-child"]

`order: {age: ASC}` or a list of orderings on the parent, and `order` on a related child, explain as `orderNode{orderings: [{direction, fields}]}` at the corresponding level.

# `go.explain.debug.scan`: Scan, filter and docID plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-request", "explain.debug.debug-explain-request-with-doc-i-ds-filter-using-multiple-but-duplicate-i-ds", "explain.debug.debug-explain-request-with-doc-i-ds-filter-using-multiple-unique-i-ds", "explain.debug.debug-explain-request-with-doc-i-ds-filter-using-one-id", "explain.debug.debug-explain-request-with-doc-id-filter", "explain.debug.debug-explain-request-with-greater-than-filter", "explain.debug.debug-explain-request-with-integer-equal-filter", "explain.debug.debug-explain-request-with-logical-compound-and-filter", "explain.debug.debug-explain-request-with-logical-compound-or-filter", "explain.debug.debug-explain-request-with-match-inside-list", "explain.debug.debug-explain-request-with-matching-id-filter", "explain.debug.debug-explain-request-with-string-equal-filter"]

Plain selects, collection filters and docID selections explain as `selectTopNode > selectNode > scanNode` with empty node bodies.

# `go.explain.debug.mutation`: Mutation plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-mutation-request-does-not-add-doc-given-duplicate", "explain.debug.debug-explain-mutation-request-with-add", "explain.debug.debug-explain-mutation-request-with-delete-using-filter", "explain.debug.debug-explain-mutation-request-with-delete-using-filter-and-ids", "explain.debug.debug-explain-mutation-request-with-delete-using-filter-to-match-everything", "explain.debug.debug-explain-mutation-request-with-delete-using-id", "explain.debug.debug-explain-mutation-request-with-delete-using-ids", "explain.debug.debug-explain-mutation-request-with-delete-using-no-ids", "explain.debug.debug-explain-mutation-request-with-update-using-boolean-filter", "explain.debug.debug-explain-mutation-request-with-update-using-id", "explain.debug.debug-explain-mutation-request-with-update-using-ids", "explain.debug.debug-explain-mutation-request-with-update-using-ids-and-filter", "explain.debug.debug-explain-mutation-request-with-upsert-succeeds"]

Explained `add_`, `delete_`, `update_` and `upsert_` mutations return `addNode`, `deleteNode > selectTopNode > selectNode > scanNode`, `updateNode > ...` and `upsertNode > ...` respectively, and no document is written.

# `go.explain.debug.dagscan`: Commit query plan

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-commits-dag-scan-query-op", "explain.debug.debug-explain-commits-dag-scan-query-op-without-field"]

`_commits(docID: ...)` with or without a `fieldName` filter explains as `selectTopNode > selectNode > dagScanNode`.

# `go.explain.debug.group.selection`: Group with filters and docIDs

- **Parents:** ["go.explain.debug.group"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-doc-i-ds-and-filter-on-parent-group-by", "explain.debug.debug-explain-request-with-doc-i-ds-on-inner-group-selection", "explain.debug.debug-explain-request-with-doc-id-on-parent-group-by", "explain.debug.debug-explain-request-with-filter-on-group-by-parent", "explain.debug.debug-explain-request-with-filter-on-inner-group-selection", "explain.debug.debug-explain-request-with-filter-on-parent-group-by-and-inner-group-selection", "explain.debug.debug-explain-request-with-group-by-on-parent", "explain.debug.debug-explain-request-with-group-by-two-fields-on-parent"]

`groupBy` with parent or inner `GROUP` filters and docIDs explains as `selectTopNode > groupNode > selectNode > scanNode`.

# `go.explain.debug.group.limit_order`: Group with limit and order

- **Parents:** ["go.explain.debug.group"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-ascending-order-on-inner-group-selection", "explain.debug.debug-explain-request-with-ascending-order-on-parent-group-by", "explain.debug.debug-explain-request-with-descending-order-on-inner-group-selection", "explain.debug.debug-explain-request-with-descending-order-on-parent-group-by", "explain.debug.debug-explain-request-with-limit-and-offset-on-inner-group-selection", "explain.debug.debug-explain-request-with-limit-and-offset-on-multiple-inner-group-selections", "explain.debug.debug-explain-request-with-limit-and-offset-on-parent-group-by", "explain.debug.debug-explain-request-with-limit-on-parent-group-by-and-inner-group-selection", "explain.debug.debug-explain-request-with-order-on-nested-parent-group-by-and-on-nested-parents-inner-group-selection", "explain.debug.debug-explain-request-with-order-on-parent-group-by-and-on-inner-group-selection"]

Parent `limit`/`order` on a grouped request add `limitNode`/`orderNode` above `groupNode`; the same arguments on inner `GROUP` selections add no extra nodes.

# `go.explain.debug.group.average`: Averages over groups

- **Parents:** ["go.explain.debug.group"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-average-inside-the-inner-group-and-nested-group-by-with-average", "explain.debug.debug-explain-request-with-average-inside-the-inner-group-on-a-field", "explain.debug.debug-explain-request-with-average-inside-the-inner-group-on-a-field-and-nested-group-by", "explain.debug.debug-explain-request-with-group-by-with-average-on-an-inner-field"]

`AVG(GROUP: {field: ...})`, including nested groups, explains as `averageNode > countNode > sumNode > groupNode > selectNode > scanNode`.

# `go.explain.debug.aggregate.top_level`: Top-level aggregates

- **Parents:** ["go.explain.debug.aggregate"]
- **Scenarios:** ["explain.debug.debug-explain-top-level-average-request", "explain.debug.debug-explain-top-level-average-request-with-filter", "explain.debug.debug-explain-top-level-count-request", "explain.debug.debug-explain-top-level-count-request-with-filter", "explain.debug.debug-explain-top-level-max-request-succeeds", "explain.debug.debug-explain-top-level-max-request-with-filter-succeeds", "explain.debug.debug-explain-top-level-min-request-succeeds", "explain.debug.debug-explain-top-level-min-request-with-filter-succeeds", "explain.debug.debug-explain-top-level-sum-request", "explain.debug.debug-explain-top-level-sum-request-with-filter"]

Top-level `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` over `Author`, with or without filter, explain as the aggregate node(s) over `selectNode > scanNode`.

# `go.explain.debug.aggregate.inline_array`: Aggregates over inline arrays

- **Parents:** ["go.explain.debug.aggregate"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-average-on-array-field", "explain.debug.debug-explain-request-with-count-on-inline-array-field", "explain.debug.debug-explain-request-with-max-on-inline-array-field-child-field-will-be-empty", "explain.debug.debug-explain-request-with-min-on-inline-array-field-child-field-will-be-empty", "explain.debug.debug-explain-request-with-sum-on-inline-array-field-child-field-will-be-empty"]

`COUNT`/`SUM`/`AVG`/`MIN`/`MAX(chapterPages: {})` explain as the aggregate node(s) over `selectNode > scanNode` with no join.

# `go.explain.debug.aggregate.join`: Aggregates over relations

- **Parents:** ["go.explain.debug.aggregate"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-average-on-joined-field", "explain.debug.debug-explain-request-with-average-on-multiple-joined-fields-with-filter", "explain.debug.debug-explain-request-with-count-on-one-to-many-joined-field", "explain.debug.debug-explain-request-with-count-on-one-to-many-joined-field-with-many-sources", "explain.debug.debug-explain-request-with-max-on-one-to-many-joined-field-succeeds", "explain.debug.debug-explain-request-with-max-on-one-to-many-joined-field-with-filter-succeeds", "explain.debug.debug-explain-request-with-max-on-one-to-many-joined-field-with-many-sources-succeeds", "explain.debug.debug-explain-request-with-min-on-one-to-many-joined-field-succeeds", "explain.debug.debug-explain-request-with-min-on-one-to-many-joined-field-with-filter-succeeds", "explain.debug.debug-explain-request-with-min-on-one-to-many-joined-field-with-many-sources-succeeds", "explain.debug.debug-explain-request-with-sum-on-one-to-many-joined-field", "explain.debug.debug-explain-request-with-sum-on-one-to-many-joined-field-with-filter", "explain.debug.debug-explain-request-with-sum-on-one-to-many-joined-field-with-many-sources"]

Aggregates over `books`/`articles` explain as the aggregate node(s) over `selectNode > typeIndexJoin > typeJoinMany`, with a `parallelNode` when there are several sources.

# `go.explain.debug.join`: Join plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-a-one-to-many-join", "explain.debug.debug-explain-request-with-a-one-to-one-join", "explain.debug.debug-explain-request-with-many-related-filters", "explain.debug.debug-explain-request-with-many-related-filters-and-doc-id", "explain.debug.debug-explain-request-with-related-and-regular-filter", "explain.debug.debug-explain-request-with-related-and-regular-filter-and-doc-i-ds", "explain.debug.debug-explain-request-with-two-level-deep-nested-joins", "explain.debug.debug-explain-request-with2-single-joins-and1-many-join"]

Related selections and related-field filters explain as `typeIndexJoin > typeJoinOne|typeJoinMany` with `root` and `subType` subplans, and a `parallelNode` for several joins.

# `go.explain.debug.limit`: Limit plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-both-limit-and-offset-on-related-child", "explain.debug.debug-explain-request-with-limit-and-offset", "explain.debug.debug-explain-request-with-limit-args-on-parent-and-related-child-with-count", "explain.debug.debug-explain-request-with-limit-on-related-child-and-both-limit-and-offset-on-parent", "explain.debug.debug-explain-request-with-only-limit", "explain.debug.debug-explain-request-with-only-limit-on-related-child", "explain.debug.debug-explain-request-with-only-limit-on-related-child-with-count", "explain.debug.debug-explain-request-with-only-offset", "explain.debug.debug-explain-request-with-only-offset-on-related-child"]

Parent `limit`/`offset` add a `limitNode` under `selectTopNode`; child `limit`/`offset` add a `limitNode` inside the join's `subType`; combined with `COUNT` the plan has a `parallelNode`.

# `go.explain.debug.order`: Order plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-ascending-order-on-parent", "explain.debug.debug-explain-request-with-multi-order-fields-on-parent", "explain.debug.debug-explain-request-with-order-field-on-parent-and-related-child", "explain.debug.debug-explain-request-with-order-field-on-related-child", "explain.debug.debug-explain-request-with-subquery-order-by-nested-relation-field", "explain.debug.debug-explain-request-with-subquery-order-by-nested-relation-field-asc"]

Parent and child `order` add `orderNode`s; ordering a child by a nested relation field (`published(order: {publisher: {establishedYear: DESC|ASC}}, limit: 2)`) explains with an `orderNode` over a nested one-to-one join inside the child plan.

# `go.explain.debug.index`: Indexed filter and order plans

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-with-index-on-filter", "explain.debug.debug-explain-with-index-on-order", "explain.debug.debug-explain-with-index-on-subquery-nested-relation-order"]

With `@index` on the filtered or ordered field the debug plan has no `orderNode`; a child ordered by an indexed related field with `limit` has a `limitNode` but no `orderNode`.

# `go.explain.debug.orphan`: Orphan handling when ordering by a relation

- **Parents:** ["go.explain.debug"]
- **Scenarios:** ["explain.debug.debug-explain-request-with-order-by-relation-field-secondary-parent", "explain.debug.debug-explain-request-with-order-by-relation-field-with-index", "explain.debug.debug-explain-request-with-order-by-relation-field-with-index-desc"]

With `@exhaustive`, ordering a primary-side parent by a related field (`Publisher(order: {book: {rating: ASC|DESC}})`) explains as `typeIndexJoin > sequenceNode[orphanNode, typeJoinOne]` (orphans first for ASC, last for DESC); ordering from the secondary side (`Book(order: {publisher: {establishedYear: ASC}})`) explains as `typeIndexJoin > orphanNode > typeJoinOne`.

# `go.encryption.doc.query`: Decrypted reads

- **Parents:** ["go.encryption.doc"]
- **Scenarios:** ["encryption.doc-encryption-with-encryption-on-both-relations-should-fetch-decrypted", "encryption.doc-encryption-with-encryption-on-counter-crdt-should-fetch-decrypted", "encryption.doc-encryption-with-encryption-on-primary-relations-should-fetch-decrypted", "encryption.doc-encryption-with-encryption-on-secondary-relations-should-fetch-decrypted", "encryption.doc-encryption-with-encryption-should-fetch-decrypted"]

Querying an encrypted document returns its plaintext fields, including a `pcounter` field that sums correctly after an update (5 then +3 gives 8), and related documents resolve when either, both or only the primary or secondary side are encrypted.

# `go.encryption.peer.diverged_versions`: KMS authorisation across diverged branchable versions

- **Parents:** ["go.encryption.peer"]
- **Scenarios:** ["encryption.doc-encryption-branchable-sync-diverged-versions-fails-kmsauth"]
- **Go-Bug-Suspect:** "test is skipped ('t.Skip') for Go issue #4789: KMS authorisation needs the doc's collection version locally, which only the sync being authorised delivers, so Go deadlocks."

Two peers whose `@branchable` collection versions have diverged each create an encrypted doc and sync bidirectionally; each must obtain the other's key and read both docs.

# `go.searchable_encryption.index.patch`: Encrypted indexes are immutable via collection patch

- **Parents:** ["go.searchable_encryption.index"]
- **Scenarios:** ["searchable_encryption.patch-collection-modify-encrypted-index-should-error", "searchable_encryption.patch-collection-new-encrypted-index-should-error", "searchable_encryption.patch-collection-remove-encrypted-index-should-error"]

A collection patch that adds, removes or modifies `/Users/EncryptedIndexes` fails with `collection encrypted indexes cannot be mutated`.

# `defra.searchable_encryption.index.one_answer`: Every client manages encrypted indexes through the same checks

- **Parents:** ["go.searchable_encryption.index"]
- **Scenarios:** ["searchable_encryption::missing_collection::encrypted_index_on_a_missing_collection_is_refused_in_and_outside_a_transaction", "searchable_encryption::missing_collection::a_transaction_cannot_list_encrypted_indexes_for_a_deleted_collection", "searchable_encryption::caller_identity::an_anonymous_caller_cannot_use_the_signing_nodes_identity_in_a_transaction"]

`NewEncryptedIndex`, `ListEncryptedIndexes`, `ListAllEncryptedIndexes` and `DeleteEncryptedIndex` answer the same through the embedded node, FFI, the CLI, the embedded node's HTTP server and the HTTP server `defradb start` runs. In a transaction each runs there, with node access checked in it for the request's caller identity, never the signing node's transaction-owner identity; with none, the caller must hold `get-collection` and then the operation's own permission (`new-encrypted-index`, `list-encrypted-index`, `delete-encrypted-index`), or `list-all-encrypted-index` alone for every collection's listing, and a change commits in a transaction of its own. An anonymous caller inside a transaction owned by the signing node is refused with `not authorized to perform operation. Permission: get-collection` for `NewEncryptedIndex`, `ListEncryptedIndexes` and `DeleteEncryptedIndex`, and with `not authorized to perform operation. Permission: list-all-encrypted-index` for `ListAllEncryptedIndexes`. A collection the node does not hold fails with `collection not found. Name: <name>` (Go's `collection not found`, naming the collection), inside a transaction and outside one alike, including a collection deleted after a transaction opened; and a transaction that is not open fails with `transaction not found`.

# `defra.signature`: Block signing

- **Parents:** ["defra"]

Signing behaviour beyond what Go's integration suite checks.

# `defra.signature.per_op_enable`: `enableSigning: true` signs on a node that does not sign

- **Parents:** ["defra.signature"]
- **Scenarios:** ["signature::per_op_signing::signature_with_per_op_signing_enabled_on_a_node_not_signing_signs_every_commit"]

A node that holds a node identity but was started without signing (`--no-signing`) writes unsigned blocks by default. An add or update sent with `enableSigning: true` (the HTTP `enableSigning` query parameter, the CLI `--enable-signing` flag, or the FFI `enableSigning` option) signs its blocks anyway: every field block and the `_C` block of a new document carry a signature from the request identity's key, or the node identity's when the request has none, as Go's `EnableSigning` option does. A node with no key at all writes unsigned blocks under either choice.

# `defra.signature.embedded_caller_key`: An in-process write signs with the acting identity's own key

- **Parents:** ["defra.signature"]
- **Scenarios:** ["signature::caller_key::signature_in_process_write_signs_with_the_callers_key", "signature::caller_key::signature_in_process_write_without_identity_signs_with_the_node_key", "signature::caller_key::signature_in_process_field_and_delete_blocks_sign_with_the_callers_key", "signature::caller_key::signature_in_process_graphql_update_signs_with_the_callers_key", "signature::caller_key::signature_in_process_branchable_collection_block_signs_with_the_callers_key", "signature::caller_key::signature_in_process_backup_import_signs_with_the_callers_key"]

An in-process (embedded) write whose request carries the acting client identity's own local secp256k1 key signs its blocks with that key whenever the write signs: the priority-1 field blocks, the `_C` block, and a `@branchable` collection's collection-level block, as Go's in-process client does with a `FullIdentity`. Verifying such a block against that identity's public key succeeds, and against the node's key fails with `signature was created by a different key`. The same holds for the `_C` block of a later update by that identity (through the document API and a GraphQL mutation), the `_C` block of a delete by that identity, and the priority-1 field blocks and `_C` block of a backup import by that identity, as Go's import of a document with a `FullIdentity` signs them. A write whose request carries no identity and no key signs with the node key: verifying its blocks against the node's key succeeds.

# `defra.signature.client_session`: The HTTP client-signing session signs each block once

- **Parents:** ["defra.signature"]

A client that keeps its own Ed25519 key creates, updates or deletes one document over the `/api/v0/graphql/client-sign` WebSocket: it sends the mutation, the server asks it to sign the blocks the mutation writes, and the server commits them with the client's signatures. The server, not only the client, holds the session to the blocks it asked for.

# `defra.signature.client_session.waves`: Each signing wave is numbered and answered once

- **Parents:** ["defra.signature.client_session"]
- **Scenarios:** ["signature::client_session::a_client_that_signs_each_wave_once_creates_the_document"]

The server asks for signatures in waves, `{"wave": <n>, "blocks": [<hex>, …]}`, numbered from 1 within the session; a create asks for its field blocks, then its `_C` block. The client answers each wave once with `{"wave": <n>, "signatures": [<hex>, …]}`, one signature per block in the wave's order. A client that answers every wave this way gets the mutation's response, and the document it created reads back. The server never asks twice for a block the client already signed in the session: a wave that would, or that names one block twice, fails the mutation with `block <i> of signing wave <n> was already signed in this session`.

# `defra.signature.client_session.resigned`: A wave answered twice is refused

- **Parents:** ["defra.signature.client_session"]
- **Scenarios:** ["signature::client_session::a_client_that_answers_a_wave_twice_is_refused"]

An answer naming a wave the client already answered fails the session with `signing wave <n> was already signed`: the server signs nothing more in that session, so the mutation's response carries the error and nothing is written. Whether the repeated answer arrives between waves or in place of the next wave's answer, the error is the same. A repeat of the last wave's answer (a create's `_C` wave, or the one wave of an update or a delete) arrives when the server asks for no further wave, so it is refused but cannot fail the mutation: the mutation commits with the blocks already signed and its response carries no error.

# `defra.signature.client_session.outside_graph`: An answer outside the approved graph is refused

- **Parents:** ["defra.signature.client_session"]
- **Scenarios:** ["signature::client_session::a_client_that_answers_an_unrequested_wave_is_refused", "signature::client_session::a_client_that_signs_a_block_outside_the_graph_is_refused"]

An answer naming a wave the server never asked for fails the session with `signing wave <n> is outside the approved graph`. A signature that does not verify over the block the server asked for in that position fails it with `signature <i> of signing wave <n> does not sign a block of the approved graph`; an answer with a different number of signatures than the wave has blocks fails it with `signing wave <n> has <k> blocks but the answer has <m> signatures`, and one that is not `{"wave", "signatures"}` with `signing answer is malformed: <reason>`. Once refused, the session signs nothing more, so the mutation's response carries the error and nothing is written. An answer that arrives after the last wave was signed is refused the same way but cannot change a block that is already signed.

# `go.signature.verify.acp`: Verification gated by access control

- **Parents:** ["go.signature.verify"]
- **Scenarios:** ["signature::acp::signature_acp_if_has_access_to_doc_validate_signature", "signature::acp::signature_acp_if_has_no_access_to_doc_should_error", "signature::acp_branchable::signature_acp_with_branchable_collection_if_has_collection_access_validate_signature", "signature::acp_branchable::signature_acp_with_branchable_collection_if_has_no_collection_access_should_error"]
- **Scope:** "document tests: go client only; branchable tests: go and C clients only"

On a DAC-protected collection, verifying a doc's block as an identity without access fails with `missing permission`, while the owner verifies it; on a private `@branchable` collection the collection-level block (no docID) is verifiable only by an identity with read access to the collection object.

# `defra.encryption.peer.registered_identity`: A peer whose node identity the host registered fetches keys from a node with no identity

- **Parents:** ["go.encryption.peer"]
- **Scenarios:** ["encryption::peer_registered_identity::a_registered_identity_peer_fetches_an_encrypted_doc_from_a_node_with_no_identity"]

A node can take its node identity from the process's signing registry, where the embedding host registered it before starting the node (the FFI's `identity_ptr`, defradb.rs's `NodeBuilder::with_node_identity_did`), instead of loading it from a key it is given. The key-holding node needs no node identity of its own (started with signing disabled and no key) to serve it, and its KMS authenticates that peer's key request against the registered DID exactly as against a loaded one: on a private `@branchable` collection whose owner grants `reader` on the collection to the peer's node identity, a document the key-holding node creates anonymously with `encrypt: true` reaches the subscribed peer, and the owner reads `{name: "Fred"}` there.

# `defra.encryption.peer.iroh_grant`: A granted iroh peer obtains a policy-bound document's key

- **Parents:** ["go.encryption.peer"]
- **Scenarios:** ["encryption::peer_iroh_acp_branchable::a_granted_iroh_peer_obtains_the_key"]

Over iroh, the key-holding node's KMS releases a document's key to the node identity the iroh handshake authenticated. With two iroh nodes, the key holder started with no node identity and the peer with one the host registered, a private `@branchable` collection whose owner grants `reader` on the collection to the peer's node identity, and both nodes subscribed to it: a document the key holder creates anonymously with `encrypt: true` reaches the peer, and the owner reads `{name: "Fred"}` there.

# `defra.encryption.peer.iroh_no_grant`: An ungranted iroh peer receives the blocks but not the key

- **Parents:** ["go.encryption.peer"]
- **Scenarios:** ["encryption::peer_iroh_acp_branchable::an_ungranted_iroh_peer_receives_the_blocks_but_not_the_key"]

The same two iroh nodes and collection with no `reader` grant to the peer: the encrypted document's blocks still replicate to the subscribed peer, but the key holder's KMS withholds the key, so the owner querying `Users { name }` on the peer gets `[]`, and still gets `[]` after the peer has had time to fetch the key.

# `defra.encryption.peer.pending_key_arrival`: A pending merge appears when its key arrives

- **Parents:** ["go.encryption.peer"]
- **Scenarios:** ["encryption::peer_pending_key::an_encrypted_doc_appears_after_its_key_arrives", "p2p_tests::remote_key_arrival_wakes_a_parked_encrypted_merge"]

After a subscribed peer receives an encrypted document's blocks but not yet its key, the document can wait as a pending merge. When the KMS delivers the key, that pending merge wakes, the document becomes queryable, and its plaintext fields are returned.

# `defra.backup.import.signed_and_replicated`: An imported document is signed and replicated as a created one is

- **Parents:** ["go.backup.import"]
- **Scenarios:** ["backup::signed_import::an_import_on_a_signing_node_signs_every_block", "backup::signed_import::an_import_reaches_a_replicator_set_before_it"]

An import writes each document exactly as a create of that document on the same node and client would: on a node that signs, every field block and the `_C` block of an imported document carry the signature a create would carry (`_commits { signature { type identity } }` names the node's key), and a replicator configured before the import receives the imported document, so the target node reads it once it has synced.

# `defra.backup.import.creator`: An import creates its documents as a create on the same edge does

- **Parents:** ["go.backup.import"]
- **Scenarios:** ["backup::import_creator::an_anonymous_import_is_owned_as_an_anonymous_create"]

An import that names no identity creates its documents as a create that names no identity does through the same client: under document ACP, another identity sees an anonymously imported document exactly when it sees an anonymously created one. On a node that signs with its identity an anonymous create acts as the node, so the node owns both. An import that names an identity creates its documents as that identity.

# `defra.backup.import.duplicate_alias`: Two documents of one file that share an ID

- **Parents:** ["go.backup.import"]
- **Scenarios:** ["backup::duplicate_alias::a_repeated_doc_id_fails_and_keeps_the_first_document"]

A backup file in which two documents carry the same `_docID` (or `_docIDNew`) fails with `a document with the given ID already exists` when it reaches the second one; the first is imported and the second is not created.

# `defra.backup.import.keeps_earlier_documents`: A failed import keeps the documents before the failure

- **Parents:** ["go.backup.import"]
- **Scenarios:** ["backup::failed_import::a_failed_import_keeps_every_document_before_the_one_refused", "backup::failed_import::an_import_refusing_a_value_keeps_every_document_before_it"]

An import that cannot create one of its documents fails with that document's error and stops there. Every document of the file before it is imported, and none after it is: a file of `{"name":"Alice"}`, `{"name":"John"}`, `{"name":"John"}`, `{"name":"Zed"}`, whose second John has the first one's content and so its ID, fails with `a document with the given ID already exists` and leaves Alice and the first John. A value the schema refuses stops it the same way: a file of `{"name":"Alice","points":1}`, `{"name":"Bob","points":"many"}` into a `pcounter` field `points` fails with `failed to create document in 'User': …` and leaves Alice.
