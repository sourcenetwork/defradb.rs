# `go.index.unique.array`: Unique indexes on array fields

- **Parents:** ["go.index.unique"]
- **Scenarios:** ["index.array-unique-composite-index-if-doc-is-added-that-violates-uniqueness-error", "index.array-unique-composite-index-if-doc-is-updated-that-violates-uniqueness-error", "index.array-unique-composite-index-if-docs-have-nil-values-succeed", "index.array-unique-composite-index-with-unique-combinations-succeed", "index.array-unique-index-upon-deleting-doc-succeed", "index.array-unique-index-upon-doc-creation-with-unique-element-succeed", "index.array-unique-index-upon-doc-creation-with-unique-elements-succeed", "index.array-unique-index-upon-doc-update-with-array-element-that-exists-error", "index.array-unique-index-upon-doc-update-with-unique-elements-succeed", "index.array-unique-index-with-nil-elements-and-all-op-succeed", "index.array-unique-index-with-nil-elements-and-any-op-succeed", "index.array-unique-index-with-nil-elements-and-none-op-succeed"]

A unique index on an array field is unique per element: creating or updating a document so that an element (or, for a composite, an element plus scalar combination) already exists in another document fails with `can not index a doc's field(s) that violates unique index.`. Unique combinations, null values and nil elements are allowed, deleting frees the values, and `_any`/`_all`/`_none` filters return correct results.

# `go.index.unique.json`: Unique indexes on JSON fields

- **Parents:** ["go.index.unique"]
- **Scenarios:** ["index.json-array-unique-index-should-allow-only-unique-values-and-use-them-for-fetching", "index.json-unique-index-upon-update-should-use-new-index-values", "index.json-unique-index-with-random-values-should-guarantee-uniqueness-and-be-abel-to-use-index-for-fetching"]

A unique index on a JSON field is unique per leaf value path (and per element of JSON arrays): a create or update that repeats a value at the same path fails with `can not index a doc's field(s) that violates unique index.`, and equality filters on the new values are served by the index after updates.

# `go.index.query.single-operators`: Single-field index with comparison operators

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-index-empty-filter-on-indexed-field-should-succeed", "index.query-with-index-if-several-docs-with-eq-filter-should-fetch-all", "index.query-with-index-if-several-docs-with-in-filter-should-fetch-all", "index.query-with-index-with-equal-filter-should-fetch", "index.query-with-index-with-greater-or-equal-filter-should-fetch", "index.query-with-index-with-greater-than-filter-should-fetch", "index.query-with-index-with-in-filter-on-float-should-fetch", "index.query-with-index-with-in-filter-should-fetch", "index.query-with-index-with-less-or-equal-filter-should-fetch", "index.query-with-index-with-less-than-filter-should-fetch", "index.query-with-index-with-like-filter-should-fetch", "index.query-with-index-with-non-indexed-fields-should-fetch-all-of-them", "index.query-with-index-with-not-equal-filter-should-fetch", "index.query-with-index-with-not-in-filter-should-fetch", "index.query-with-index-with-not-like-filter-should-fetch"]

With `@index` on one field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like` and `_nlike` (Int, Float, String) return the matching documents, several matches are all returned, an empty filter on the field returns all, and filters on non-indexed fields still work; explain reports the expected index fetch count.

# `go.index.query.single-null`: Null semantics of indexed filters

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-index-with-geq-null-filter-on-date-time-field-should-not-use-index", "index.query-with-index-with-geq-null-filter-on-float-field-should-not-use-index", "index.query-with-index-with-geq-null-filter-on-int-field-should-not-use-index", "index.query-with-index-with-leq-null-filter-on-date-time-field-should-use-index", "index.query-with-index-with-leq-null-filter-on-float-field-should-use-index", "index.query-with-index-with-leq-null-filter-on-int-field-should-use-index", "index.query-with-index-with-ne-filter-against-boolean-field-should-fetch-nil-values", "index.query-with-index-with-ne-filter-against-date-time-field-should-fetch-nil-values", "index.query-with-index-with-ne-filter-against-float-field-should-fetch-nil-values", "index.query-with-index-with-ne-filter-against-int-field-should-fetch-nil-values", "index.query-with-index-with-ne-filter-against-string-field-should-fetch-nil-values"]

On an indexed Int, Float, String, Boolean or DateTime field, `_ne: <value>` also returns documents whose value is null and `_ne: null` returns only non-null ones. `_le: null` returns only null documents and uses the index; `_ge: null` returns every document and does not use the index (0 index fetches).

# `go.index.query.datetime`: Indexed DateTime filters

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-index-with-eq-filter-on-date-time-field-should-index", "index.query-with-index-with-ge-filter-on-date-time-field-should-index", "index.query-with-index-with-gt-filter-on-date-time-field-should-index", "index.query-with-index-with-le-filter-on-date-time-field-should-index", "index.query-with-index-with-lt-filter-on-date-time-field-should-index", "index.query-with-index-with-ne-filter-on-date-time-field-should-index"]

On an indexed DateTime field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt` and `_le` with an RFC 3339 value return the matching documents through the index.

# `go.index.query.unique-single`: Unique single-field index filters

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-unique-index-if-no-match-return-empty-result", "index.query-with-unique-index-with-date-time-field-should-index", "index.query-with-unique-index-with-equal-filter-on-nil-value-should-fetch", "index.query-with-unique-index-with-equal-filter-on-zero-should-not-fetch-nil", "index.query-with-unique-index-with-equal-filter-should-fetch", "index.query-with-unique-index-with-greater-or-equal-filter-should-fetch", "index.query-with-unique-index-with-greater-than-filter-should-fetch", "index.query-with-unique-index-with-in-filter-should-fetch", "index.query-with-unique-index-with-less-or-equal-filter-should-fetch", "index.query-with-unique-index-with-less-than-filter-should-fetch", "index.query-with-unique-index-with-like-filter-should-fetch", "index.query-with-unique-index-with-multiple-nil-values-and-equal-filter-should-fetch", "index.query-with-unique-index-with-not-case-insensitive-like-filter-should-fetch", "index.query-with-unique-index-with-not-equal-filter-on-nil-value-should-fetch", "index.query-with-unique-index-with-not-equal-filter-should-fetch", "index.query-with-unique-index-with-not-in-filter-should-fetch", "index.query-with-unique-index-with-not-like-filter-should-fetch"]

With a unique index on one field (Int, String or DateTime), every comparison operator including `_ilike`/`_nilike` returns the matching documents; no match returns an empty list, `_eq: 0` does not return null documents, `_eq: null` returns every null document and `_ne: null` every non-null one.

# `go.index.query.composite-operators`: Composite index with comparison operators

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-composite-index-with-equal-filter-on-nil-value-on-first-should-fetch", "index.query-with-composite-index-with-equal-filter-on-nil-value-on-second-should-fetch", "index.query-with-composite-index-with-equal-filter-should-fetch", "index.query-with-composite-index-with-greater-or-equal-filter-on-first-field-should-fetch", "index.query-with-composite-index-with-greater-or-equal-filter-on-second-field-should-fetch", "index.query-with-composite-index-with-greater-than-filter-on-first-field-should-fetch", "index.query-with-composite-index-with-greater-than-filter-on-second-field-should-fetch", "index.query-with-composite-index-with-in-filter-should-fetch", "index.query-with-composite-index-with-less-or-equal-filter-on-first-field-should-fetch", "index.query-with-composite-index-with-less-or-equal-filter-on-second-field-should-fetch", "index.query-with-composite-index-with-less-than-filter-on-first-field-should-fetch", "index.query-with-composite-index-with-less-than-filter-on-second-field-should-fetch", "index.query-with-composite-index-with-like-filter-should-fetch", "index.query-with-composite-index-with-not-equal-filter-should-fetch", "index.query-with-composite-index-with-not-in-filter-should-fetch", "index.query-with-composite-index-with-not-like-filter-should-fetch"]

With a composite index on two fields, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like`, `_nlike` on the first or second field return the matching documents, including `_eq: null` on either field.

# `go.index.query.unique-composite-operators`: Unique composite index with comparison operators

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-unique-composite-index-with-equal-filter-should-fetch", "index.query-with-unique-composite-index-with-greater-or-equal-filter-on-first-field-should-fetch", "index.query-with-unique-composite-index-with-greater-or-equal-filter-on-second-field-should-fetch", "index.query-with-unique-composite-index-with-greater-than-filter-on-first-field-should-fetch", "index.query-with-unique-composite-index-with-greater-than-filter-on-second-field-should-fetch", "index.query-with-unique-composite-index-with-in-filter-should-fetch", "index.query-with-unique-composite-index-with-less-or-equal-filter-on-first-field-should-fetch", "index.query-with-unique-composite-index-with-less-or-equal-filter-on-second-field-should-fetch", "index.query-with-unique-composite-index-with-less-than-filter-on-first-field-should-fetch", "index.query-with-unique-composite-index-with-less-than-filter-on-second-field-should-fetch", "index.query-with-unique-composite-index-with-like-filter-should-fetch", "index.query-with-unique-composite-index-with-not-case-insensitive-like-filter-should-fetch", "index.query-with-unique-composite-index-with-not-equal-filter-should-fetch", "index.query-with-unique-composite-index-with-not-in-filter-should-fetch", "index.query-with-unique-composite-index-with-not-like-filter-should-fetch"]

With a unique composite index on two fields, every comparison operator on the first or second field, including `_nilike`, returns the matching documents.

# `go.index.query.unique-composite-nil`: Nulls in a unique composite index

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-unique-composite-index-after-update-on-nil-fields-should-fetch", "index.query-with-unique-composite-index-with-equal-filter-on-nil-value-on-first-should-fetch", "index.query-with-unique-composite-index-with-equal-filter-on-nil-value-on-second-should-fetch", "index.query-with-unique-composite-index-with-multiple-nil-on-both-fields-and-nil-filter-should-fetch-all", "index.query-with-unique-composite-index-with-multiple-nil-on-first-field-and-nil-filter-should-fetch-all", "index.query-with-unique-composite-index-with-multiple-nil-on-second-fields-and-nil-filter-should-fetch-all"]

A unique composite index allows several documents with null in either or both fields; `_eq: null` on either field returns all of them, and documents updated from null values are found by their new values.

# `go.index.query.combined`: Indexed and non-indexed conditions together

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-index-filter-on-non-indexed-field-should-ignore-index", "index.query-with-index-if-index-filter-with-regular-should-filter", "index.query-with-index-if-multiple-index-filters-with-regular-case-insensitive-should-filter", "index.query-with-index-if-multiple-index-filters-with-regular-should-filter"]

A filter combining one or several indexed fields with non-indexed fields (field names case-insensitive in the test schema) returns only documents matching all conditions; a filter on only non-indexed fields ignores the index.

# `go.index.query.or`: `_or` filters and indexes

- **Parents:** ["go.index.query"]
- **Scenarios:** ["index.query-with-index-with-or-filter-on-indexed-and-non-indexed-field-should-fallback-to-full-scan", "index.query-with-index-with-or-filter-on-only-non-indexed-fields-should-not-use-index", "index.query-with-index-with-or-filter-on-two-different-indexed-fields-should-not-use-index", "index.query-with-index-with-or-filter-on-two-different-indexed-fields-with-overlap-should-return-correct-results", "index.query-with-index-with-or-filter-on-two-different-indexed-fields-with-range-conditions-should-not-use-index", "index.query-with-index-with-or-filter-should-fetch-correct-docs", "index.query-with-index-with-or-filter-with-any-and-none-on-same-array-field-should-fallback-to-full-scan", "index.query-with-index-with-or-filter-with-overlapping-conditions-should-deduplicate-results", "index.query-with-index-with-or-filter-with-range-conditions-should-use-index", "index.query-with-index-with-or-filter-with-three-branches-should-use-index"]

`_or` branches on the same indexed field (equality or ranges, two or three branches) are served by the index and overlapping branches return each document once. `_or` mixing indexed and non-indexed fields, two different indexed fields, or `_any` and `_none` on one array field falls back to a full scan, with correct results.

# `go.index.order.single-field`: `order` on a single indexed field

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.order-query-with-index-with-ascending-order-should-use-index", "index.order-query-with-index-with-ascending-query-on-descending-indexed-field-should-return-in-reverse-order", "index.order-query-with-index-with-filter-on-indexed-field-ascending-should-use-index", "index.order-query-with-index-with-filter-on-indexed-field-descending-should-use-index", "index.order-query-with-index-with-filter-on-non-indexed-field-ascending-should-use-index-for-ordering", "index.order-query-with-index-with-filter-on-non-indexed-field-descending-should-use-index-for-ordering", "index.order-query-with-index-with-limit-ascending-should-use-index", "index.order-query-with-index-with-limit-descending-should-use-index"]

`order: {age: ASC|DESC}` on an indexed field returns documents in that order through the index, with or without `limit` and with filters on the same or another field; an ascending order on a `direction: DESC` index still returns ascending results.

# `go.index.order.composite`: `order` against a composite index

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.order-query-with-composite-index-order-mismatch-asc-and-asc-should-not-use-index", "index.order-query-with-composite-index-order-mismatch-asc-and-desc-should-not-use-index", "index.order-query-with-composite-index-order-mismatch-desc-and-asc-should-not-use-index", "index.order-query-with-composite-index-order-mismatch-desc-and-desc-should-not-use-index", "index.order-query-with-composite-index-with-order-on-non-index-in-end-should-not-use-index", "index.order-query-with-composite-index-with-order-on-non-index-in-middle-should-not-use-index"]

A multi-field `order` whose directions match neither the composite index nor its exact reverse, or that puts a non-indexed field in the middle or end, does not use the index (0 index fetches) and still returns correctly ordered results.

# `go.index.order.relation`: `order` on related or relation ID fields

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.order-query-with-index-on-relation-order-by-primary-doc-should-order-with-index", "index.order-query-with-index-on-relation-order-by-secondary-doc-should-order-with-index", "index.order-query-with-index-with-order-on-nested-field-should-use-index-for-ordering", "index.order-query-with-index-with-order-on-relation-id-field-should-use-index-for-ordering"]

Ordering by an indexed field of a related document (`order: {device: {model: ASC}}`), by an indexed relation ID (`order: {_ownerID: ASC}`), or from either side of a relation returns correctly ordered results using the index.

# `go.index.order.composite-direction`: Unordered results follow composite index directions

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.query-with-composite-index-if-exact-match-with-reverted-order-on-first-field-should-fetch", "index.query-with-composite-index-if-exact-match-with-reverted-order-on-second-field-should-fetch", "index.query-with-composite-index-with-default-order-case-insensitive-should-fetch-in-default-order", "index.query-with-composite-index-with-default-order-should-fetch-in-default-order", "index.query-with-composite-index-with-reverted-order-on-first-field-and-no-filter-should-fetch-in-reverted-order", "index.query-with-composite-index-with-reverted-order-on-first-field-case-insensitive-should-fetch-in-reverted-order", "index.query-with-composite-index-with-reverted-order-on-first-field-should-fetch-in-reverted-order", "index.query-with-composite-index-with-reverted-order-on-second-field-case-insensitive-should-fetch-in-reverted-order", "index.query-with-composite-index-with-reverted-order-on-second-field-should-fetch-in-reverted-order"]

A query without `order` filtered through a composite index returns documents in the index's per-field direction order (default ASC, or DESC on the first or second field), with direction keywords case-insensitive, also without a filter and for exact matches.

# `go.index.order.composite-in`: `_in` through a composite index with a reversed field

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.query-with-composite-index-with-in-filter-on-first-field-with-reverted-order-should-fetch", "index.query-with-composite-index-with-in-filter-on-second-field-with-reverted-order-should-fetch"]

`_in` on the first or second field of a composite index with one DESC field returns the matching documents.

# `go.index.order.composite-range`: Range filters on a composite index's first field

- **Parents:** ["go.index.order"]
- **Scenarios:** ["index.query-with-composite-index-with-descending-first-field-and-range-query-should-use-range-optimization", "index.query-with-composite-index-with-range-query-on-first-field-should-use-range-optimization", "index.query-with-composite-index-with-range-query-on-first-field-with-multiple-filters-should-use-range-optimization"]

A range filter (`_gt` etc.) on the first field of a composite index, ASC or DESC, alone or with further filters, is served by an index range scan and returns the matching documents.

# `go.index.relation.one-to-many-filter`: Filters through one-to-many and many-to-one indexes

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-index-on-many-to-one-if-filter-on-indexed-field-should-filter-with-explain", "index.query-with-index-on-many-to-one-if-filter-on-indexed-relation-should-filter-with-explain", "index.query-with-index-on-many-to-one-multiple-via-one-to-many", "index.query-with-index-on-one-to-many-if-filter-on-indexed-primary-doc-and-sub-filter-should-filter", "index.query-with-index-on-one-to-many-if-filter-on-indexed-primary-doc-should-filter", "index.query-with-index-on-one-to-many-if-filter-on-indexed-relation-should-filter-with-explain", "index.query-with-index-on-one-to-many-if-indexed-relation-is-nil-eq-nil-filter-should-use-index", "index.query-with-index-on-one-to-many-if-indexed-relation-is-nil-ne-nil-filter-should-use-index", "index.query-with-index-on-one-to-many-relation-if-filter-on-indexed-relation-should-filter"]

Filtering parents by an indexed field of their children (`User(filter: {devices: {model: {_eq: ...}}})`), children by an indexed parent field or relation ID, including `_eq: null`/`_ne: null` on an indexed relation, and many-to-one chains, returns the right documents and reports index fetches at the expected explain levels.

# `go.index.relation.one-to-one-filter`: Filters through one-to-one indexes

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-index-on-one-to-one-if-filter-on-indexed-relation-should-filter", "index.query-with-index-on-one-to-one-primary-relation-if-filter-on-indexed-field-of-relation-and-relation-should-filter", "index.query-with-index-on-one-to-one-primary-relation-if-filter-on-indexed-field-of-relation-should-filter", "index.query-with-index-on-one-to-one-primary-relation-if-filter-on-indexed-relation-while-indexed-foreign-field-should-filter", "index.query-with-index-on-one-to-ones-secondary-relation-if-filter-on-indexed-relation-should-filter", "index.query-with-unique-composite-index-with-filter-on-indexed-relation-should-filter", "index.query-with-unique-index-with-filter-on-child-indexed-field-should-fetch"]

Filtering on an indexed field of a one-to-one related document, from the primary or secondary side, on the relation and a related field together, or through a unique (composite) index on the related type, returns the matching documents.

# `go.index.relation.combined-filter`: Relation filters combined with other conditions

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.index-group-by-with-filter-on-indexed-relation-returns-group-by-field-value", "index.index-query-with-index-on-one-to-many-relation-and-filter-data", "index.index-query-with-index-on-one-to-many-relation-and-filter-no-data", "index.index-query-with-index-on-one-to-many-relation-not-filter-data", "index.index-query-with-index-on-one-to-many-relation-not-filter-no-data", "index.index-query-with-index-on-one-to-many-relation-or-filter-data", "index.index-query-with-index-on-one-to-many-relation-or-filter-no-data", "index.query-with-composite-index-on-many-to-one-with-multiple-indexed-child-nodes-should-succeed", "index.query-with-index-with-filter-on2-relations-should-filter", "index.query-with-index-with-multiple-scalars-and-relation-filter-should-apply-all-as-and", "index.query-with-index-with-scalar-and-relation-filter-at-top-level-should-apply-both-as-and"]

Scalar and relation conditions at the top level apply as an AND; `_and`, `_or` and `_not` over an indexed one-to-many relation return the right documents or none; filters on two relations, `groupBy` with a filter on an indexed relation, and a composite index on several indexed child fields all return correct results.

# `go.index.relation.sub-filter`: Child sub-filters through indexes

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-index-on-one-to-many-if-sub-filter-and-order-on-indexed-field-should-use-index-for-filter", "index.query-with-index-on-one-to-many-if-sub-filter-on-indexed-field-should-filter", "index.query-with-index-on-one-to-many-if-sub-filter-on-non-indexed-field-should-not-use-index", "index.query-with-index-on-one-to-many-with-order-on-parent-and-sub-filter-should-filter-both-with-indexes", "index.query-with-index-on-one-to-many-with-order-on-parent-and-sub-filter-should-filter-per-parent", "index.query-with-index-on-one-to-many-with-parent-filter-on-own-field-and-relation-and-sub-filter-should-combine-all-filters", "index.query-with-index-on-one-to-many-with-parent-filter-on-relation-and-sub-filter-on-different-indexed-field-should-use-both-indexes", "index.query-with-index-on-one-to-many-with-parent-filter-on-relation-and-sub-filter-on-non-indexed-field-should-use-parent-index", "index.query-with-index-on-one-to-many-with-same-filter-on-parent-and-sub-type-should-filter-both-with-indexes", "index.query-with-index-on-one-to-many-with-same-filter-value-on-parent-and-sub-type-should-return-matching-docs"]

A filter on a one-to-many child selection (`devices(filter: ...)`), alone or with a parent filter, parent ordering, or the same condition at both levels, returns each parent's matching children, using the child's index when the field is indexed and not otherwise.

# `go.index.relation.sub-order`: Child ordering through indexes

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-order-on-one-to-many-with-index-on-order-field-ascending-should-order", "index.query-with-order-on-one-to-many-with-index-on-order-field-ascending-with-limit-should-order-and-limit", "index.query-with-order-on-one-to-many-with-index-on-order-field-descending-should-order", "index.query-with-order-on-one-to-many-with-multiple-authors-and-index-on-relation-should-order-each-authors-books", "index.query-with-order-on-one-to-many-with-multiple-authors-should-order-each-authors-books", "index.query-with-order-on-one-to-many-with-parent-filter-on-relation-and-sub-order-should-order-children", "index.query-with-order-on-one-to-many-with-sub-filter-and-order-and-relation-index-should-filter-then-order", "index.query-with-order-on-one-to-many-with-sub-filter-and-order-on-same-indexed-field-should-filter-then-order-asc", "index.query-with-order-on-one-to-many-with-sub-filter-and-order-on-same-indexed-field-should-filter-then-order-desc"]

`published(order: {rating: ASC|DESC})` on a one-to-many child with an indexed order field orders each parent's children independently, with `limit`, with a sub-filter on the same or another field (filter then order), and with a parent filter on the relation.

# `go.index.relation.order-orphans-excluded`: Ordering by a related field drops orphans without `@exhaustive`

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-nested-order-by-relation-field-with-asc-and-limit-excludes-orphans", "index.query-with-nested-order-by-relation-field-with-asc-and-limit-recursive-explain", "index.query-with-nested-order-by-relation-field-with-desc-and-limit-excludes-orphans", "index.query-with-nested-order-by-relation-field-with-desc-and-limit-recursive-explain", "index.query-with-order-by-relation-field-with-parent-primary-asc-excludes-orphans", "index.query-with-order-by-relation-field-with-parent-secondary-asc-excludes-orphans"]

Without `@exhaustive`, ordering by an indexed field of a related document (`Book(order: {publisher: {establishedYear: ASC}})`, at the root or in a nested selection, with or without `limit`) returns only documents that have the related document; explain shows the index-based inverted join at each level.

# `go.index.relation.order-orphans-exhaustive`: `@exhaustive` keeps orphans when ordering by a related field

- **Parents:** ["go.index.relation"]
- **Scenarios:** ["index.query-with-filter-on-null-relation-secondary-doc-without-relation-should-return-orphans", "index.query-with-index-exhaustive-order-by-relation-with-scalar-filter-descending-should-filter-orphans", "index.query-with-index-exhaustive-order-by-relation-with-scalar-filter-should-filter-orphans", "index.query-with-nested-order-by-relation-field-exhaustive-with-asc-and-limit-should-include-orphans-first", "index.query-with-nested-order-by-relation-field-exhaustive-with-desc-and-limit-should-append-orphans-last", "index.query-with-nested-order-by-relation-field-exhaustive-with-primary-parent-asc-should-include-orphans", "index.query-with-order-by-relation-field-exhaustive-asc-many-books-shows-full-pipeline", "index.query-with-order-by-relation-field-exhaustive-asc-with-limit-many-orphans-early-termination", "index.query-with-order-by-relation-field-exhaustive-desc-with-limit-many-orphans-skips-orphan-phase", "index.query-with-order-by-relation-field-exhaustive-with-parent-primary-asc-should-include-orphans", "index.query-with-order-by-relation-field-exhaustive-with-parent-primary-desc-should-include-orphans", "index.query-with-order-by-relation-field-exhaustive-with-parent-secondary-asc-should-include-orphans", "index.query-with-order-by-relation-field-exhaustive-with-parent-secondary-desc-should-include-orphans", "index.query-with-order-by-relation-field-with-some-docs-without-relation-should-include-all"]

With `query @exhaustive`, ordering by a related document's field returns every document: orphans (null related value) come first for ASC and last for DESC, at the root or nested, from the primary or secondary side, with `limit` stopping early, and with same-object scalar filters still applied to orphans.

# `go.index.array.types`: Array element types

- **Parents:** ["go.index.array"]
- **Scenarios:** ["index.array-index-bool-should-use-index", "index.array-index-float-should-use-index", "index.array-index-optional-bool-should-use-index", "index.array-index-optional-float-should-use-index", "index.array-index-optional-int-should-use-index", "index.array-index-optional-string-should-use-index"]

An index on `[Int]`, `[Float]`, `[Boolean]`, `[String]` and their nillable-element variants serves `_any: {_eq: ...}` filters and returns the matching documents.

# `go.index.array.operators`: `_any`, `_all`, `_none` on indexed arrays

- **Parents:** ["go.index.array"]
- **Scenarios:** ["index.array-index-with-all-and-in-operator-succeed", "index.array-index-with-all-and-nin-operator-succeed", "index.array-index-with-any-and-in-operator-succeed", "index.array-index-with-any-and-nin-operator-succeed", "index.array-index-with-filter-on-indexed-array-using-all-should-use-index", "index.array-index-with-filter-on-indexed-array-using-any-should-use-index", "index.array-index-with-filter-on-indexed-array-using-none-should-not-use-index", "index.array-index-with-none-and-in-operator-succeed", "index.array-index-with-none-and-nin-operator-succeed"]

`_any` and `_all` filters on an indexed array (with `_eq`, `_in`, `_nin`) use the index and return the matching documents; `_none` returns correct results without using the index.

# `go.index.array.nil-elements`: Null elements in indexed arrays

- **Parents:** ["go.index.array"]
- **Scenarios:** ["index.array-index-with-nil-elements-and-all-op-succeed", "index.array-index-with-nil-elements-and-any-op-succeed", "index.array-index-with-nil-elements-and-none-op-succeed"]

Arrays containing null elements are indexed; `_any`, `_all` and `_none` filters with `_eq: null` or a value return the matching documents.

# `go.index.array.update`: Array index maintenance on update

- **Parents:** ["go.index.array"]
- **Scenarios:** ["index.array-index-delete-if-update-removes-sought-element-should-not-fetch", "index.array-index-update-if-update-adds-sought-element-should-fetch", "index.array-index-update-if-update-rearranges-array-elements-should-fetch", "index.array-index-update-if-update-removes-sought-element-should-not-fetch"]

After an update adds, removes or reorders array elements, `_any` filters return the document only if the sought element is present in the new array.

# `go.index.array.composite`: Composite indexes with array fields

- **Parents:** ["go.index.array"]
- **Scenarios:** ["index.array-composite-index-delete-with2-consecutive-array-fields-succeed", "index.array-composite-index-update-with2-array-fields-succeed", "index.array-composite-index-with-any-none-all-succeed", "index.array-composite-index-with-filter-on-indexed-array-using-all-should-use-index", "index.array-composite-index-with-filter-on-indexed-array-using-any-should-use-index", "index.array-composite-index-with-filter-on-indexed-array-using-none-should-use-index", "index.array-composite-index-with2-consecutive-array-fields-succeed", "index.array-composite-index-with2-separate-array-fields-succeed"]

Composite indexes with two array fields (adjacent or separated by a scalar) serve `_any`, `_all` and `_none` filters and stay correct after updates and deletes.

# `go.index.json.scalar-ops`: Comparison filters on JSON paths

- **Parents:** ["go.index.json"]
- **Scenarios:** ["index.json-index-upon-update-should-use-new-index-values", "index.json-index-with-compound-filter-condition-should-use-index", "index.json-index-with-eq-filter-on-bool-field-should-use-index", "index.json-index-with-eq-filter-on-string-field-should-use-index", "index.json-index-with-filter-on-number-field-should-use-index", "index.json-index-with-ge-filter-on-number-field-should-use-index", "index.json-index-with-greater-than-filter-on-top-level-json-field-should-use-index", "index.json-index-with-gt-filter-on-number-field-should-use-index", "index.json-index-with-in-filter-of-different-types-should-use-index", "index.json-index-with-in-filter-should-use-index", "index.json-index-with-le-filter-on-number-field-should-use-index", "index.json-index-with-like-filter-on-string-field-should-use-index", "index.json-index-with-lt-filter-on-number-field-should-use-index", "index.json-index-with-n-like-filter-on-string-field-should-use-index", "index.json-index-with-ne-filter-on-bool-field-should-use-index", "index.json-index-with-ne-filter-on-number-field-should-use-index", "index.json-index-with-nin-filter-should-use-index", "index.json-index-with-non-nillable-field-with-filter-on-number-field-should-use-index", "index.json-index-with-not-and-in-filter-should-not-use-index"]

Filters on a path inside an indexed JSON field (`custom: {height: {_eq: 168}}`) for numbers, strings and booleans with `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in` (including mixed-type lists), `_nin`, `_like`, `_ilike`, `_nlike`, `_nilike`, `_and`, and top-level `_gt` return the matching documents through the index, also for `JSON!` and after updates; `_not` over `_in` does not use the index.

# `go.index.json.null`: Null semantics on indexed JSON

- **Parents:** ["go.index.json"]
- **Scenarios:** ["index.json-index-with-eq-filter-against-explicit-null-field-should-fetch-null-values", "index.json-index-with-eq-filter-on-null-field-should-use-index", "index.json-index-with-geq-null-filter-on-nested-json-path-should-not-use-index", "index.json-index-with-geq-null-filter-on-top-level-json-field-should-not-use-index", "index.json-index-with-leq-null-filter-on-nested-json-path-should-not-use-index", "index.json-index-with-leq-null-filter-on-top-level-json-field-should-use-index", "index.json-index-with-ne-filter-against-bool-field-should-fetch-null-values", "index.json-index-with-ne-filter-against-number-field-should-fetch-null-values", "index.json-index-with-ne-filter-against-string-field-should-fetch-null-values", "index.json-index-with-ne-filter-on-null-nested-field-should-use-index"]

`_eq: null` on a JSON path or on the whole field returns documents where it is null; `_ne: <value>` also returns documents where the path is null or missing; `_ne: null` returns non-null ones. `_le: null` on the whole field uses the index; `_ge: null` (top-level or nested) and `_le: null` on a nested path do not, with correct results.

# `go.index.json.object-values`: Object and array values in JSON filters

- **Parents:** ["go.index.json"]
- **Scenarios:** ["index.json-index-with-eq-filter-with-object-value-on-nested-path-should-filter", "index.json-index-with-in-filter-with-object-value-on-nested-path-should-filter", "index.json-index-with-neq-filter-with-object-value-on-nested-path-should-filter", "index.json-index-with-nin-filter-with-object-value-on-nested-path-should-filter", "index.query-with-index-with-eq-filter-on-json-field-with-nested-array-value-should-not-use-index", "index.query-with-index-with-eq-filter-on-json-field-with-object-value-should-not-use-index", "index.query-with-index-with-in-filter-on-json-field-with-object-value-should-not-use-index"]

`_eq`, `_ne`, `_in` and `_nin` whose value is a JSON object or nested array (`nested: {_eq: {foo: "bar"}}`) return the matching documents by full-scan comparison without using the index (0 index fetches).

# `go.index.json.array`: JSON arrays in an index

- **Parents:** ["go.index.json"]
- **Scenarios:** ["index.json-array-index-with-all-eq-and-comparison-filter-should-fetch-correctly-using-index", "index.json-array-index-with-all-eq-filter-should-fetch-correctly-using-index", "index.json-array-index-with-any-and-comparison-filter-should-not-consider-them", "index.json-array-index-with-any-eq-filter-should-not-consider-them", "index.json-array-index-with-different-element-values-and-types-should-fetch-correctly-using-index", "index.json-array-index-with-none-eq-and-comparison-filter-should-fetch-correctly-using-index", "index.json-array-index-with-none-eq-filter-should-fetch-correctly-using-index"]

Elements of JSON arrays are indexed: `_any: {_eq|_gt: ...}` matches top-level elements only (not elements of nested arrays or a scalar in place of the array); `_all` with `_eq`/`_gt` uses the index; `_none` returns correct results without the index; elements of mixed types are matched correctly.

# `go.index.maintenance.delete-with-filter`: Filter deletes clean up index entries

- **Parents:** ["go.index.maintenance"]
- **Scenarios:** ["index.doc-delete-with-filter-with-multiple-matching-docs-cleans-up-all-index-entries", "index.doc-delete-with-filter-with-unique-index-cleans-up-index-entries"]

`delete_<Type>(filter: ...)` removes the index entries of every deleted document, so re-adding documents with the same unique or non-unique indexed values succeeds.

# `go.txn.docs.uncommitted-invisible`: Uncommitted document writes are invisible

- **Parents:** ["go.txn.docs"]
- **Scenarios:** ["txn.txn-add-doc-without-commit-empty-results", "txn.txn-delete-doc-without-commit-does-not-delete", "txn.txn-request-without-commit-empty-results", "txn.txn-update-doc-with-filter-without-commit-does-not-update-document", "txn.txn-update-doc-without-commit-does-not-update-document"]

Before commit, documents added, updated (by ID or filter), deleted or created by a mutation inside a transaction are not visible to queries outside it, which still return the previous state.

# `go.txn.docs.read-own-writes`: Transactions see their own document writes

- **Parents:** ["go.txn.docs"]
- **Scenarios:** ["txn.txn-delete-doc-exhibits-transactional-isolation-succeeds", "txn.txn-update-doc-exhibits-transactional-isolation-succeeds", "txn.txn-update-with-filter-exhibits-transactional-isolation-succeeds"]

Inside a transaction, `UpdateDoc`, update-with-filter and `DeleteDoc` operate on documents created earlier in the same transaction, and the result is visible after commit.

