# `go.explain.errors`: Invalid explain requests are rejected by GraphQL validation

- **Parents:** ["go.explain"]
- **Scenarios:** ["explain.debug.debug-explain-mutation-request-with-delete-having-no-sub-selection", "explain.debug.debug-explain-request-where-parent-is-ordered-by-its-related-child", "explain.default.default-explain-mutation-request-with-delete-having-no-sub-selection", "explain.default.default-explain-on-wrong-field-directive-bad-usage", "explain.default.default-explain-request-where-parent-is-ordered-by-its-related-child", "explain.default.invalid-explain-request-type-returns-error", "explain.execute.execute-explain-request-where-parent-field-is-ordered-by-child-field"]

`@explain` on a field instead of the operation fails with `Directive "explain" may not be used on FIELD.`; `@explain(type: invalid)` fails with `Argument "type" has invalid value invalid.\nExpected type "ExplainType", found invalid.`; an explained `delete_Author(docID: [...])` with no sub-selection fails with `Field "delete_Author" of type "[Author]" must have a sub selection.`; ordering a parent by a one-to-many child (`order: {articles: {name: ASC}}`) fails with `Argument "order" has invalid value {articles: {name: ASC}}.\nIn field "articles": Unknown field.` in default, debug and execute modes.

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

# `go.explain.execute.orphan`: Orphan node counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-with-orphan-node-with-primary-parent-reports-metrics", "explain.execute.execute-explain-with-orphan-node-with-secondary-parent-reports-metrics"]

With `@exhaustive` and ordering by a relation field, the `orphanNode` reports `iterations`, `docFetches`, `fieldFetches` and `indexFetches` for both the primary-parent and secondary-parent paths.

# `go.explain.execute.similarity`: Similarity counters

- **Parents:** ["go.explain.execute"]
- **Scenarios:** ["explain.execute.execute-explain-request-with-similarity"]

`SIMILARITY(pointsList: {vector: [1, 2, 0]})` reports `similarityNode` iterations above the select and scan counters.

