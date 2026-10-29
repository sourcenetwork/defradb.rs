# `go.explain.errors`: Invalid explain requests are rejected by GraphQL validation

- **Parents:** ["go.explain"]

`@explain` on a field instead of the operation fails with `Directive "explain" may not be used on FIELD.`; `@explain(type: invalid)` fails with `Argument "type" has invalid value invalid.\nExpected type "ExplainType", found invalid.`; an explained `delete_Author(docID: [...])` with no sub-selection fails with `Field "delete_Author" of type "[Author]" must have a sub selection.`; ordering a parent by a one-to-many child (`order: {articles: {name: ASC}}`) fails with `Argument "order" has invalid value {articles: {name: ASC}}.\nIn field "articles": Unknown field.` in default, debug and execute modes.

# `go.explain.execute.scan`: Scan counters

- **Parents:** ["go.explain.execute"]

With no docs, all docs matching, some matching, or none matching a filter, `selectNode` reports `iterations` (rows + 1) and `filterMatches`, and `scanNode` reports `iterations`, `docFetches`, `fieldFetches` and `indexFetches: 0`; `showDeleted: true` counts deleted documents too.

# `go.explain.execute.mutation`: Mutation counters

- **Parents:** ["go.explain.execute"]
- **Go-Bug-Suspect:** "'upsert_ContactAddress' whose filter matches one of two documents reports 'selectNode{iterations: 4, filterMatches: 2}', counting the matched document twice."

Explained `add_`, `delete_` (by docID or filter), `update_` (by docIDs or filter) and `upsert_` (matching or not) run the mutation and report the counts of the mutation node's inner select and scan.

# `go.explain.execute.dagscan`: Commit scan counters

- **Parents:** ["go.explain.execute"]

`_commits(docID: ...)` under execute explain reports `dagScanNode` iteration counts and the select node's `filterMatches` for the document's commits.

# `go.explain.execute.shaping`: Group, limit and order counters

- **Parents:** ["go.explain.execute"]

`groupBy`, parent and child `limit`/`offset`, and parent, child and nested-relation `order` report counters on the `groupNode`, `limitNode` and `orderNode` and the scans below them.

# `go.explain.execute.aggregate`: Aggregate counters

- **Parents:** ["go.explain.execute"]

Top-level `COUNT`/`SUM`/`AVG`, and `COUNT`/`SUM`/`AVG`/`MIN`/`MAX` over inline arrays and one-to-many relations, report iteration counts on the aggregate nodes and the fetch counters of the scans and joins below.

# `go.explain.execute.join`: Join counters

- **Parents:** ["go.explain.execute"]

One-to-one joins (single, two in parallel, two levels deep, and from the secondary side) report `typeIndexJoin` iterations and the fetch counters of both sides; joining from the secondary side reports `indexFetches` from the relation index.

# `go.explain.execute.index`: Index fetch counters

- **Parents:** ["go.explain.execute"]

With `@index` on `age`, filtering `age: {_eq: 30}` over three docs reports `scanNode{iterations: 3, docFetches: 2, fieldFetches: 4, indexFetches: 2}`; ordering by the indexed field, by an indexed relation field with `@exhaustive`, and a child ordered by an indexed related field report index fetches instead of a sort.

# `go.explain.execute.orphan`: Orphan node counters

- **Parents:** ["go.explain.execute"]

With `@exhaustive` and ordering by a relation field, the `orphanNode` reports `iterations`, `docFetches`, `fieldFetches` and `indexFetches` for both the primary-parent and secondary-parent paths.

# `go.explain.execute.similarity`: Similarity counters

- **Parents:** ["go.explain.execute"]

`SIMILARITY(pointsList: {vector: [1, 2, 0]})` reports `similarityNode` iterations above the select and scan counters.

