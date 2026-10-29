# `go.index.unique.array`: Unique indexes on array fields

- **Parents:** ["go.index.unique"]

A unique index on an array field is unique per element: creating or updating a document so that an element (or, for a composite, an element plus scalar combination) already exists in another document fails with `can not index a doc's field(s) that violates unique index.`. Unique combinations, null values and nil elements are allowed, deleting frees the values, and `_any`/`_all`/`_none` filters return correct results.

# `go.index.unique.json`: Unique indexes on JSON fields

- **Parents:** ["go.index.unique"]

A unique index on a JSON field is unique per leaf value path (and per element of JSON arrays): a create or update that repeats a value at the same path fails with `can not index a doc's field(s) that violates unique index.`, and equality filters on the new values are served by the index after updates.

# `go.index.query.single-operators`: Single-field index with comparison operators

- **Parents:** ["go.index.query"]

With `@index` on one field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like` and `_nlike` (Int, Float, String) return the matching documents, several matches are all returned, an empty filter on the field returns all, and filters on non-indexed fields still work; explain reports the expected index fetch count.

# `go.index.query.single-null`: Null semantics of indexed filters

- **Parents:** ["go.index.query"]

On an indexed Int, Float, String, Boolean or DateTime field, `_ne: <value>` also returns documents whose value is null and `_ne: null` returns only non-null ones. `_le: null` returns only null documents and uses the index; `_ge: null` returns every document and does not use the index (0 index fetches).

# `go.index.query.datetime`: Indexed DateTime filters

- **Parents:** ["go.index.query"]

On an indexed DateTime field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt` and `_le` with an RFC 3339 value return the matching documents through the index.

# `go.index.query.unique-single`: Unique single-field index filters

- **Parents:** ["go.index.query"]

With a unique index on one field (Int, String or DateTime), every comparison operator including `_ilike`/`_nilike` returns the matching documents; no match returns an empty list, `_eq: 0` does not return null documents, `_eq: null` returns every null document and `_ne: null` every non-null one.

# `go.index.query.composite-operators`: Composite index with comparison operators

- **Parents:** ["go.index.query"]

With a composite index on two fields, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like`, `_nlike` on the first or second field return the matching documents, including `_eq: null` on either field.

# `go.index.query.unique-composite-operators`: Unique composite index with comparison operators

- **Parents:** ["go.index.query"]

With a unique composite index on two fields, every comparison operator on the first or second field, including `_nilike`, returns the matching documents.

# `go.index.query.unique-composite-nil`: Nulls in a unique composite index

- **Parents:** ["go.index.query"]

A unique composite index allows several documents with null in either or both fields; `_eq: null` on either field returns all of them, and documents updated from null values are found by their new values.

# `go.index.query.combined`: Indexed and non-indexed conditions together

- **Parents:** ["go.index.query"]

A filter combining one or several indexed fields with non-indexed fields (field names case-insensitive in the test schema) returns only documents matching all conditions; a filter on only non-indexed fields ignores the index.

# `go.index.query.or`: `_or` filters and indexes

- **Parents:** ["go.index.query"]

`_or` branches on the same indexed field (equality or ranges, two or three branches) are served by the index and overlapping branches return each document once. `_or` mixing indexed and non-indexed fields, two different indexed fields, or `_any` and `_none` on one array field falls back to a full scan, with correct results.

# `go.index.order.single-field`: `order` on a single indexed field

- **Parents:** ["go.index.order"]

`order: {age: ASC|DESC}` on an indexed field returns documents in that order through the index, with or without `limit` and with filters on the same or another field; an ascending order on a `direction: DESC` index still returns ascending results.

# `go.index.order.composite`: `order` against a composite index

- **Parents:** ["go.index.order"]

A multi-field `order` whose directions match neither the composite index nor its exact reverse, or that puts a non-indexed field in the middle or end, does not use the index (0 index fetches) and still returns correctly ordered results.

# `go.index.order.relation`: `order` on related or relation ID fields

- **Parents:** ["go.index.order"]

Ordering by an indexed field of a related document (`order: {device: {model: ASC}}`), by an indexed relation ID (`order: {_ownerID: ASC}`), or from either side of a relation returns correctly ordered results using the index.

# `go.index.order.composite-direction`: Unordered results follow composite index directions

- **Parents:** ["go.index.order"]

A query without `order` filtered through a composite index returns documents in the index's per-field direction order (default ASC, or DESC on the first or second field), with direction keywords case-insensitive, also without a filter and for exact matches.

# `go.index.order.composite-in`: `_in` through a composite index with a reversed field

- **Parents:** ["go.index.order"]

`_in` on the first or second field of a composite index with one DESC field returns the matching documents.

# `go.index.order.composite-range`: Range filters on a composite index's first field

- **Parents:** ["go.index.order"]

A range filter (`_gt` etc.) on the first field of a composite index, ASC or DESC, alone or with further filters, is served by an index range scan and returns the matching documents.

# `go.index.relation.one-to-many-filter`: Filters through one-to-many and many-to-one indexes

- **Parents:** ["go.index.relation"]

Filtering parents by an indexed field of their children (`User(filter: {devices: {model: {_eq: ...}}})`), children by an indexed parent field or relation ID, including `_eq: null`/`_ne: null` on an indexed relation, and many-to-one chains, returns the right documents and reports index fetches at the expected explain levels.

# `go.index.relation.one-to-one-filter`: Filters through one-to-one indexes

- **Parents:** ["go.index.relation"]

Filtering on an indexed field of a one-to-one related document, from the primary or secondary side, on the relation and a related field together, or through a unique (composite) index on the related type, returns the matching documents.

# `go.index.relation.combined-filter`: Relation filters combined with other conditions

- **Parents:** ["go.index.relation"]

Scalar and relation conditions at the top level apply as an AND; `_and`, `_or` and `_not` over an indexed one-to-many relation return the right documents or none; filters on two relations, `groupBy` with a filter on an indexed relation, and a composite index on several indexed child fields all return correct results.

# `go.index.relation.sub-filter`: Child sub-filters through indexes

- **Parents:** ["go.index.relation"]

A filter on a one-to-many child selection (`devices(filter: ...)`), alone or with a parent filter, parent ordering, or the same condition at both levels, returns each parent's matching children, using the child's index when the field is indexed and not otherwise.

# `go.index.relation.sub-order`: Child ordering through indexes

- **Parents:** ["go.index.relation"]

`published(order: {rating: ASC|DESC})` on a one-to-many child with an indexed order field orders each parent's children independently, with `limit`, with a sub-filter on the same or another field (filter then order), and with a parent filter on the relation.

# `go.index.relation.order-orphans-excluded`: Ordering by a related field drops orphans without `@exhaustive`

- **Parents:** ["go.index.relation"]

Without `@exhaustive`, ordering by an indexed field of a related document (`Book(order: {publisher: {establishedYear: ASC}})`, at the root or in a nested selection, with or without `limit`) returns only documents that have the related document; explain shows the index-based inverted join at each level.

# `go.index.relation.order-orphans-exhaustive`: `@exhaustive` keeps orphans when ordering by a related field

- **Parents:** ["go.index.relation"]

With `query @exhaustive`, ordering by a related document's field returns every document: orphans (null related value) come first for ASC and last for DESC, at the root or nested, from the primary or secondary side, with `limit` stopping early, and with same-object scalar filters still applied to orphans.

# `go.index.array.types`: Array element types

- **Parents:** ["go.index.array"]

An index on `[Int]`, `[Float]`, `[Boolean]`, `[String]` and their nillable-element variants serves `_any: {_eq: ...}` filters and returns the matching documents.

# `go.index.array.operators`: `_any`, `_all`, `_none` on indexed arrays

- **Parents:** ["go.index.array"]

`_any` and `_all` filters on an indexed array (with `_eq`, `_in`, `_nin`) use the index and return the matching documents; `_none` returns correct results without using the index.

# `go.index.array.nil-elements`: Null elements in indexed arrays

- **Parents:** ["go.index.array"]

Arrays containing null elements are indexed; `_any`, `_all` and `_none` filters with `_eq: null` or a value return the matching documents.

# `go.index.array.update`: Array index maintenance on update

- **Parents:** ["go.index.array"]

After an update adds, removes or reorders array elements, `_any` filters return the document only if the sought element is present in the new array.

# `go.index.array.composite`: Composite indexes with array fields

- **Parents:** ["go.index.array"]

Composite indexes with two array fields (adjacent or separated by a scalar) serve `_any`, `_all` and `_none` filters and stay correct after updates and deletes.

# `go.index.json.scalar-ops`: Comparison filters on JSON paths

- **Parents:** ["go.index.json"]

Filters on a path inside an indexed JSON field (`custom: {height: {_eq: 168}}`) for numbers, strings and booleans with `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in` (including mixed-type lists), `_nin`, `_like`, `_ilike`, `_nlike`, `_nilike`, `_and`, and top-level `_gt` return the matching documents through the index, also for `JSON!` and after updates; `_not` over `_in` does not use the index.

# `go.index.json.null`: Null semantics on indexed JSON

- **Parents:** ["go.index.json"]

`_eq: null` on a JSON path or on the whole field returns documents where it is null; `_ne: <value>` also returns documents where the path is null or missing; `_ne: null` returns non-null ones. `_le: null` on the whole field uses the index; `_ge: null` (top-level or nested) and `_le: null` on a nested path do not, with correct results.

# `go.index.json.object-values`: Object and array values in JSON filters

- **Parents:** ["go.index.json"]

`_eq`, `_ne`, `_in` and `_nin` whose value is a JSON object or nested array (`nested: {_eq: {foo: "bar"}}`) return the matching documents by full-scan comparison without using the index (0 index fetches).

# `go.index.json.array`: JSON arrays in an index

- **Parents:** ["go.index.json"]

Elements of JSON arrays are indexed: `_any: {_eq|_gt: ...}` matches top-level elements only (not elements of nested arrays or a scalar in place of the array); `_all` with `_eq`/`_gt` uses the index; `_none` returns correct results without the index; elements of mixed types are matched correctly.

# `go.index.maintenance.delete-with-filter`: Filter deletes clean up index entries

- **Parents:** ["go.index.maintenance"]

`delete_<Type>(filter: ...)` removes the index entries of every deleted document, so re-adding documents with the same unique or non-unique indexed values succeeds.

# `go.txn.docs.uncommitted-invisible`: Uncommitted document writes are invisible

- **Parents:** ["go.txn.docs"]

Before commit, documents added, updated (by ID or filter), deleted or created by a mutation inside a transaction are not visible to queries outside it, which still return the previous state.

# `go.txn.docs.read-own-writes`: Transactions see their own document writes

- **Parents:** ["go.txn.docs"]

Inside a transaction, `UpdateDoc`, update-with-filter and `DeleteDoc` operate on documents created earlier in the same transaction, and the result is visible after commit.

