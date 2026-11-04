# `go.query.commits.filter.fieldname-eq`: Equality on fieldName

- **Parents:** ["go.query.commits.filter"]
- **Scenarios:** ["query.commits.with-doc-idand-field", "query.commits.with-doc-idand-unknown-field", "query.commits.with-doc-idand-unknown-field-id", "query.commits.with-field", "query.commits.with-field-id"]

`filter: {fieldName: {_eq: "age"}}` (alone or with `docID`) returns only the `age` field commit; a field ID string such as `"1"` or `"999999"`, or an unknown name such as `"not a field"`, matches nothing and returns an empty list rather than an error.

# `go.query.commits.filter.composite`: Filtering on the composite block

- **Parents:** ["go.query.commits.filter"]
- **Scenarios:** ["query.commits.with-composite-field", "query.commits.with-composite-field-id-with-returned-collection-version-id", "query.commits.with-doc-idand-composite-field", "query.commits.with-filter-field-name-not-equal-composite-returns-field-commits"]
- **Go-Bug-Suspect:** "TestQueryCommitsWithCompositeFieldIdWithReturnedCollectionVersionID is marked 'for documentation reasons only. This is not desired behaviour (Users should not be specifying field ids)', though it now filters on '_C'; the comment looks stale"

`filter: {fieldName: {_eq: "_C"}}` returns only the composite commit (with its `collectionVersionId`), with or without `docID`; `{fieldName: {_neq: "_C"}}` returns the `age` and `name` field commits.

# `go.query.one_to_one_to_many.join`: Two-hop resolution in both directions

- **Parents:** ["go.query.one_to_one_to_many"]
- **Scenarios:** ["query.one_to_one_to_many.", "query.one_to_one_to_many.from-secondary", "query.one_to_one_to_many.from-secondary-on-one-to-many", "query.one_to_one_to_many.from-secondary-on-one-to-one"]

With `Indicator`–`Observable` one-to-one (either side primary) and `Observable { observations: [Observation] }`, `Observation { observable { indicator } }` and `Indicator { observable { observations } }` both resolve.

# `go.query.commits.branchable.collection-commits`: Collection-level commit graph

- **Parents:** ["go.query.commits.branchable"]
- **Scenarios:** ["query.commits.branchables.basic", "query.commits.branchables.with-all-fields", "query.commits.branchables.with-delete", "query.commits.branchables.with-doc-update", "query.commits.branchables.with-if-directive-false", "query.commits.branchables.with-if-directive-true", "query.commits.branchables.with-multiple-add"]
- **Scope:** "TestQueryCommitsBranchables_WithAllFields excludes the signed-docs and encrypted-docs multipliers (it asserts plaintext delta bytes)"

For a `type Users @branchable` (or `@branchable(if: true)`) collection, every create, update and delete adds one collection-level commit with `docID: null`, `fieldName: null`, height 1 for the first, whose `links` point to the document's composite (`_C`) commit and whose `heads` point to the previous collection commit; one doc create yields 4 `_commits` (name, age, `_C`, collection). With `@branchable(if: false)` no collection-level commit exists and a create yields 3 commits.

# `go.query.commits.branchable.query-args`: Looking up branchable collection commits

- **Parents:** ["go.query.commits.branchable"]
- **Scenarios:** ["query.commits.branchables.with-cid-and-doc-idparam", "query.commits.branchables.with-cid-param", "query.commits.branchables.with-field-name-filter"]
- **Scope:** "TestQueryCommitsBranchables_WithCidParam excludes the signed-docs and encrypted-docs multipliers"
- **Go-Bug-Suspect:** "TestQueryCommitsBranchables_WithCidAndDocIDParam asserts that '_commits(docID: {doc}, cid: {collection cid})' fails with 'cid either does not exist or belong to document'; the test comment says it would be nice if this worked and links Go issue #3213"

`_commits(cid: <collection commit cid>)` returns that single commit with `docID` and `fieldName` null; `_commits(filter: {fieldName: {_eq: null}})` returns only the collection-level commit (with its `collectionVersionId`). Combining a document's `docID` with a collection-level `cid` errors with `cid either does not exist or belong to document`.

# `go.query.commits.listing`: Listing commits and their fields

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.after-doc-deletion-should-still-fetch", "query.commits.basic", "query.commits.multiple-docs", "query.commits.test-query-commits-with-all-fields-with-update-no-error", "query.commits.with-alias-succeeds", "query.commits.with-collection-version-idfield", "query.commits.with-doc-idproperty", "query.commits.with-doc-idwith-type-name", "query.commits.with-field-name-field", "query.commits.with-field-name-field-and-update"]
- **Scope:** "TestQuery_CommitsWithAllFieldsWithUpdate_NoError excludes the signed-docs and encrypted-docs multipliers"

Unfiltered `_commits` on a non-branchable collection returns one commit per field plus one composite (`fieldName: "_C"`) per create (3 for one doc, 6 for two) and one more field and composite commit per update. Each commit exposes `cid`, `collectionVersionId`, `delta`, `docID`, `fieldName`, `height`, `links`, `heads`, `signature` (null when unsigned) and `__typename: "Commit"`; the query can be aliased (`history: _commits`). After the document is deleted, its commits, including the delete composite at height 2 heading to the create composite, are still returned.

# `go.query.commits.links.structure`: Link and head structure

- **Parents:** ["go.query.commits.links"]
- **Scenarios:** ["query.commits.with-doc-idand-links", "query.commits.with-doc-idand-update-and-links", "query.commits.with-single-add-nested-links-composite-filter-succeed", "query.commits.with-single-add-nested-links-nested-filter-succeed", "query.commits.with-single-add-nested-links-succeed", "query.commits.with-single-update-double-nested-links-succeeds"]
- **Scope:** "TestQueryCommits_WithSingleUpdateDoubleNestedLinks_Succeeds excludes the signed-docs and encrypted-docs multipliers"
- **Go-Bug-Suspect:** "TestQueryCommitsWithDocIDAndUpdateAndLinks is marked in Go as 'for documentation reasons only. This is not desired behaviour' (the first result includes the head link, the second the field links)"

A composite `_C` commit links to the field commits it changed (create: `age` and `name` at height 1; update: only `age`) and heads to the previous composite; field commits have empty `links` and head to their previous version. Nested selections (`links { heads { links } }`) resolve recursively with `cid`, `height`, `fieldName` and `docID`, and `links(filter: {fieldName: {_eq: "age"}})` filters the nested list.

# `go.query.commits.links.count`: Counting links and heads

- **Parents:** ["go.query.commits.links"]
- **Scenarios:** ["query.commits.with-doc-idand-link-count", "query.commits.with-doc-updates-and-link-head-count"]

`COUNT(field: links)` and `COUNT(field: heads)` inside `_commits(docID: ...)` return per-commit counts: after one create the composite has 2 links and field commits 0; after an update the new composite has 1 link and 1 head, the new `age` commit 0 links and 1 head.

# `go.query.commits.docid`: Commits by document ID

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.doc-idempty-list", "query.commits.doc-idlist-of-many", "query.commits.doc-idlist-of-one", "query.commits.with-doc-id", "query.commits.with-doc-id-shared-field-block-across-documents", "query.commits.with-doc-idand-update", "query.commits.with-unknown-doc-id"]
- **Go-Bug-Suspect:** "TestQueryCommits_DocIDListOfMany asserts '_commits(docID: [a, b])' errors with 'querying by multiple docIDs is not yet supported' although the argument accepts a list; a gap, not a design choice"

`_commits(docID: X)` returns only that document's commits (3 after create, 5 after one update with heights 2,1,1,2,1); an unknown docID or `docID: []` returns an empty list and `docID: [X]` behaves like `docID: X`. Two docs with an identical field value share a byte-identical field block, and each doc's `_commits(docID:)` still returns its own `name`, `age` and `_C` commits attributed to that docID. `docID` with two or more entries errors with `querying by multiple docIDs is not yet supported`.

# `go.query.commits.cid.errors`: Invalid or unknown CIDs

- **Parents:** ["go.query.commits.cid"]
- **Scenarios:** ["query.commits.with-doc-idand-cid-for-different-doc", "query.commits.with-doc-idand-cid-for-different-doc-with-update", "query.commits.with-invalid-cid", "query.commits.with-invalid-short-cid", "query.commits.with-unknown-cid"]

A malformed CID string (`"fhbnjfahfhfhanfhga"`, `"bafybeidfhbnjfahfhfhanfhga"`) errors with `invalid cid`; a well-formed CID that is not in the store errors with `cid either does not exist or belong to document`.

# `go.query.commits.depth`: Depth-limited commit traversal

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.with-depth1", "query.commits.with-depth1-and-multiple-docs", "query.commits.with-depth1-with-update", "query.commits.with-depth2-with-update", "query.commits.with-filter-field-name-and-depth-returns-commits-at-all-heights"]

`_commits(depth: N)` returns the current heads of every block plus up to N-1 prior versions: depth 1 after a create returns 3 commits (6 for two docs), after an update the heads only (age at 2, name at 1, composite at 2); depth 2 after two updates adds the head-1 versions. Combined with `filter: {fieldName: {_eq: "age"}}` and depth 2 it returns `age` at heights 3 and 2.

# `go.query.commits.filter.compound`: Compound and set operators on fieldName

- **Parents:** ["go.query.commits.filter"]
- **Scenarios:** ["query.commits.with-filter-field-name-and-condition-returns-only-name-commit", "query.commits.with-filter-field-name-in-and-condition-returns-filtered-commits", "query.commits.with-filter-field-name-in-composite-returns-composite-commit", "query.commits.with-filter-field-name-in-empty-returns-no-commits", "query.commits.with-filter-field-name-in-returns-matching-commits", "query.commits.with-filter-field-name-not-in-composite-excludes-composite-commit", "query.commits.with-filter-field-name-not-in-empty-returns-all-commits", "query.commits.with-filter-field-name-not-in-excludes-matching-commits", "query.commits.with-filter-field-name-not-in-or-condition-returns-filtered-commits", "query.commits.with-filter-field-name-or-condition-returns-matching-commits"]

`_or`, `_and`, `_in` and `_nin` work on `fieldName`: `_in: ["age", "name"]` returns those two field commits, `_in: ["_C"]` the composite, `_in: []` nothing, `_nin: ["_C", "age"]` only `name`, `_nin: []` all three, and nested combinations (e.g. `_and: [{_in: [age, name]}, {_neq: age}]` gives `name`) evaluate as boolean logic.

# `go.query.commits.order`: Ordering commits

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.with-doc-idand-order-and-multi-updates-cid-asc", "query.commits.with-doc-idand-order-cid-asc", "query.commits.with-doc-idand-order-cid-desc", "query.commits.with-doc-idand-order-height-asc", "query.commits.with-doc-idand-order-height-desc"]
- **Scope:** "the two order-by-cid tests exclude the signed-docs and encrypted-docs multipliers"

`_commits(docID: X, order: {height: ASC|DESC})` sorts commits by height (after three updates: 1,1,1,2,2,3,3,4,4 ascending); `order: {cid: ASC|DESC}` sorts by CID bytes.

# `go.query.commits.paging`: Limit and offset on commits

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.with-doc-idand-limit", "query.commits.with-doc-idand-limit-and-offset", "query.commits.with-doc-idand-order-and-limit-and-offset"]

`_commits(docID: X, limit: 2)` returns 2 commits and `limit: 2, offset: 1` skips one; with `order: {height: ASC}, limit: 2, offset: 4` after three updates it returns the commits at heights 2 and 3.

# `go.query.commits.null-args`: Null arguments are ignored

- **Parents:** ["go.query.commits"]
- **Scenarios:** ["query.commits.with-null-cid", "query.commits.with-null-depth", "query.commits.with-null-field", "query.commits.with-null-group-by", "query.commits.with-null-limit", "query.commits.with-null-offset", "query.commits.with-null-order", "query.commits.with-null-order-field"]

Passing `null` for `depth`, `cid`, `docID`, `order`, `limit`, `offset` or `groupBy` behaves as if the argument were absent (all 3 commits returned); `filter: {fieldName: {_eq: null}}` on a non-branchable collection returns an empty list.

# `go.query.json.eq`: JSON equality

- **Parents:** ["go.query.json"]
- **Scenarios:** ["query.json.with-aggregate-filter-succeeds", "query.json.with-compound-filter-condition-should-filter", "query.json.with-equal-filter-with-all-types-should-filter", "query.json.with-equal-filter-with-nested-objects-should-filter", "query.json.with-equal-filter-with-null-value-should-filter", "query.json.with-equal-filter-with-object-should-filter", "query.json.with-equal-filter-with-object-value-on-nested-path-should-filter"]

`{custom: {_eq: {tree: "oak", age: 450}}}` matches documents whose JSON equals that object (deep, including nested arrays in order); `_eq: null` matches a null value but not `{}`; an object literal never matches strings, numbers, bools or arrays. A path filter (`{custom: {tree: {_eq: "oak"}}}`, `{custom: {nested: {_eq: {foo: "bar"}}}}`) compares the value at that path, combines under `_and`, and works inside `COUNT(Users: {filter: ...})` (returns 1).

# `go.query.json.neq`: JSON inequality

- **Parents:** ["go.query.json"]
- **Scenarios:** ["query.json.with-ne-filter-against-boolean-field-should-filter", "query.json.with-ne-filter-against-null-field-should-filter", "query.json.with-ne-filter-against-number-field-should-filter", "query.json.with-ne-filter-against-string-field-should-filter", "query.json.with-not-equal-filter-with-nested-object-should-filter", "query.json.with-not-equal-filter-with-nested-objects-should-filter", "query.json.with-not-equal-filter-with-null-value-should-filter", "query.json.with-not-equal-filter-with-object-should-filter"]
- **Go-Bug-Suspect:** "'{custom: {age: {_neq: 48}}}' includes a doc whose 'age' is explicitly null, but '{custom: {nested: {_neq: {foo: 'bar'}}}}' excludes a doc that lacks the 'nested' key entirely; a missing path and a null path are treated differently"

`_neq` is the negation of JSON `_eq`: a whole-value `_neq` object returns every other doc including one whose JSON is null; `_neq: null` returns non-null docs. On a path (`{custom: {age: {_neq: 48}}}`, string and bool variants) docs where the path holds null are included and `{age: {_neq: null}}` returns docs with a non-null age; on an object-valued path, docs whose path holds a different object or a scalar match, while docs missing the path do not.

# `go.query.json.in`: JSON membership

- **Parents:** ["go.query.json"]
- **Scenarios:** ["query.json.with-in-filter-should-filter", "query.json.with-not-in-filter-should-filter"]

`{custom: {_in: [{tree: "oak", age: 450}]}}` matches docs whose JSON equals one of the listed objects and `_nin` returns the others.

# `go.query.json.ordering.greater`: Greater-than on JSON numbers

- **Parents:** ["go.query.json.ordering"]
- **Scenarios:** ["query.json.with-greater-equal-filter-with-all-types-should-filter", "query.json.with-greater-equal-filter-with-equal-value-should-filter", "query.json.with-greater-equal-filter-with-greater-value-should-filter", "query.json.with-greater-equal-filter-with-nested-equal-value-should-filter", "query.json.with-greater-equal-filter-with-nested-greater-value-should-filter", "query.json.with-greater-than-filter-block-with-greater-value-should-filter", "query.json.with-greater-than-filter-block-with-lesser-value-should-filter", "query.json.with-greater-than-filter-block-with-nested-greater-value-should-filter", "query.json.with-greater-than-filter-block-with-nested-lesser-value-should-filter", "query.json.with-greater-than-filter-with-all-types-should-filter"]

`{Custom: {_gt: 20}}` / `_geq` with a number match docs whose JSON number (top level or at a path such as `{Custom: {age: {_gt: 20}}}`) is greater (or equal); non-number JSON values (strings like `"32"`, arrays, objects, bools) never match. The returned top-level number renders as an integer (`21`) and a number nested in an object as a float (`21.0`).

# `go.query.json.ordering.lesser`: Less-than on JSON numbers

- **Parents:** ["go.query.json.ordering"]
- **Scenarios:** ["query.json.with-lesser-equal-filter-with-all-types-should-filter", "query.json.with-lesser-equal-filter-with-equal-value-should-filter", "query.json.with-lesser-equal-filter-with-lesser-value-should-filter", "query.json.with-lesser-equal-filter-with-nested-equal-value-should-filter", "query.json.with-lesser-equal-filter-with-nested-lesser-value-should-filter", "query.json.with-lesser-than-filter-block-with-greater-value-should-filter", "query.json.with-lesser-than-filter-block-with-lesser-value-should-filter", "query.json.with-lesser-than-filter-block-with-nested-greater-value-should-filter", "query.json.with-lesser-than-filter-block-with-nested-lesser-value-should-filter", "query.json.with-lesser-than-filter-with-all-types-should-filter"]

`_lt` / `_leq` with a number match docs whose JSON number (top level or at a path) is less (or equal); non-number JSON values never match (`{Custom: {_lt: 33}}` over `"32"`, `[1,2]`, `{one:1}`, `false`, `32` returns only the `32` doc).

# `go.query.json.ordering.null`: Range comparisons against null

- **Parents:** ["go.query.json.ordering"]
- **Scenarios:** ["query.json.with-greater-equal-filter-with-nested-null-value-should-filter", "query.json.with-greater-equal-filter-with-null-value-should-filter", "query.json.with-greater-than-filter-block-with-nested-null-filter-value-should-filter", "query.json.with-greater-than-filter-block-with-null-filter-value-should-filter", "query.json.with-lesser-equal-filter-with-nested-null-value-should-filter", "query.json.with-lesser-equal-filter-with-null-value-should-filter", "query.json.with-lesser-than-filter-block-with-nested-null-filter-value-should-filter", "query.json.with-lesser-than-filter-block-with-null-filter-value-should-filter"]

Null sorts below every value: `_gt: null` matches only docs with a non-null value, `_geq: null` matches all docs (including unset and explicit-null path values), `_lt: null` matches none, and `_leq: null` matches only docs where the value or path is unset or null.

# `go.query.json.ordering.type-errors`: Non-numeric range operands

- **Parents:** ["go.query.json.ordering"]
- **Scenarios:** ["query.json.with-greater-equal-filter-with-array-value-returns-error", "query.json.with-greater-equal-filter-with-bool-value-returns-error", "query.json.with-greater-equal-filter-with-object-value-returns-error", "query.json.with-greater-equal-filter-with-string-value-returns-error", "query.json.with-greater-than-filter-block-with-array-value-returns-error", "query.json.with-greater-than-filter-block-with-bool-value-returns-error", "query.json.with-greater-than-filter-block-with-object-value-returns-error", "query.json.with-greater-than-filter-block-with-string-value-returns-error", "query.json.with-lesser-equal-filter-with-array-value-returns-error", "query.json.with-lesser-equal-filter-with-bool-value-returns-error", "query.json.with-lesser-equal-filter-with-object-value-returns-error", "query.json.with-lesser-equal-filter-with-string-value-returns-error", "query.json.with-lesser-than-filter-block-with-array-value-returns-error", "query.json.with-lesser-than-filter-block-with-bool-value-returns-error", "query.json.with-lesser-than-filter-block-with-object-value-returns-error", "query.json.with-lesser-than-filter-block-with-string-value-returns-error"]
- **Go-Bug-Suspect:** "the asserted messages leak Go type names: 'unexpected type. Property: condition, Actual: map[string]interface {}' and '[]interface {}'; Rust cannot reproduce them literally"

`_gt`, `_geq`, `_lt` or `_leq` on a JSON field with a bool, string, object or array operand errors with `unexpected type. Property: condition, Actual: bool` (resp. `string`, `map[string]interface {}`, `[]interface {}`).

# `go.query.json.like`: Pattern matching on JSON strings

- **Parents:** ["go.query.json"]
- **Scenarios:** ["query.json.with-like-filter-should-filter", "query.json.with-not-like-filter-should-filter"]

`{custom: {_like: "Daenerys%Name"}}` matches JSON string values by pattern and never matches arrays, objects, bools or numbers; `_nlike: "%Stormborn%"` returns every doc whose value does not match, including non-string values.

# `go.query.json.array-ops`: `_any`, `_all`, `_none` on JSON arrays

- **Parents:** ["go.query.json"]
- **Scenarios:** ["query.json.with-all-filter-and-nested-array-should-filter", "query.json.with-all-filter-with-all-types-should-filter", "query.json.with-any-filter-and-nested-array-should-filter", "query.json.with-any-filter-with-all-types-should-filter", "query.json.with-none-filter-and-nested-array-should-filter", "query.json.with-none-filter-should-filter"]

`_all`, `_any` and `_none` apply the inner condition to the top-level elements of a JSON array and never match a non-array value (null, scalar 0/1/3, `""`, `true`). Nested arrays are not flattened: `_any: {_eq: 3}` does not match `[..., [1, [2, 3]]]` and `_all: {_eq: 1}` does not match `[1, [1, [1]]]`.

# `go.query.inline_array.read.bool-int`: Boolean and Int arrays

- **Parents:** ["go.query.inline_array.read"]
- **Scenarios:** ["query.inline_array.with-booleans-empty-list", "query.inline_array.with-booleans-not-empty", "query.inline_array.with-booleans-null", "query.inline_array.with-integers-empty-list", "query.inline_array.with-integers-missing", "query.inline_array.with-integers-not-empty-list", "query.inline_array.with-integers-null", "query.inline_array.with-mix-integers-not-empty-list", "query.inline_array.with-negative-integers-not-empty-list", "query.inline_array.with-nillable-booleans", "query.inline_array.with-nillable-ints"]

`[Boolean!]`, `[Boolean]`, `[Int!]` and `[Int]` fields return exactly what was stored: null when set to null or omitted, `[]` for an empty list, values in insertion order (including negatives), and null elements in nillable arrays (`[-1, null, -1, 2, 0]`).

# `go.query.inline_array.read.float-string`: Float and String arrays

- **Parents:** ["go.query.inline_array.read"]
- **Scenarios:** ["query.inline_array.with-floats-empty-list", "query.inline_array.with-floats-not-empty", "query.inline_array.with-floats-null", "query.inline_array.with-nillable-floats", "query.inline_array.with-nillable-string", "query.inline_array.with-strings-empty-list", "query.inline_array.with-strings-not-empty", "query.inline_array.with-strings-null"]

`[Float!]`, `[Float]`, `[String!]` and `[String]` fields return null, `[]`, or the stored elements in order, preserving precision (`0.00000000001`, `-0.00000000001`), empty strings and null elements in nillable arrays.

# `go.query.inline_array.non-nillable.scalar-required`: Non-nillable DateTime, Blob and JSON fields

- **Parents:** ["go.query.inline_array.non-nillable"]
- **Scenarios:** ["query.inline_array.test-query-non-nillable-blob-not-empty", "query.inline_array.test-query-non-nillable-blob-null", "query.inline_array.test-query-non-nillable-blob-omitted", "query.inline_array.test-query-non-nillable-date-time-not-empty", "query.inline_array.test-query-non-nillable-date-time-null", "query.inline_array.test-query-non-nillable-date-time-omitted", "query.inline_array.test-query-non-nillable-json-not-empty", "query.inline_array.test-query-non-nillable-json-null", "query.inline_array.test-query-non-nillable-json-omitted"]

For `DateTime!`, `Blob!` and `JSON!` fields a create with an explicit null fails with `null value provided for non-nillable field. Name: <field>` and a create omitting the field fails with `value not provided for non-nillable field. Name: <field>`; a provided value is stored and returned (`"ff0011"`, `{"role": "admin"}`).

# `go.query.inline_array.filter.all`: `_all` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.test-query-inline-boolean-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-float-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-int-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-not-null-boolean-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-not-null-float-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-not-null-int-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-not-null-string-array-with-all-filter-succeeds", "query.inline_array.test-query-inline-string-array-with-all-filter-and-null-value-succeeds", "query.inline_array.test-query-inline-string-array-with-all-filter-succeeds"]

`Users(filter: {field: {_all: {...}}})` returns docs whose every element satisfies the condition (`_all: {_neq: null}` excludes arrays holding a null; `_all: {_lt: 70}` keeps `[0, 60]`) for String, Int, Float and Boolean arrays, nillable or not; a doc whose array is null does not match `_all: {_eq: null}`.

# `go.query.inline_array.filter.any`: `_any` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.test-query-inline-boolean-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-float-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-int-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-not-null-boolean-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-not-null-float-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-not-null-int-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-not-null-string-array-with-any-filter-succeeds", "query.inline_array.test-query-inline-string-array-with-any-filter-and-null-value-succeeds", "query.inline_array.test-query-inline-string-array-with-any-filter-succeeds"]

`_any` returns docs with at least one matching element (`_any: {_eq: null}` matches `[null, "second"]`, `_any: {_gt: 70}` matches `[50, 80]`) for String, Int, Float and Boolean arrays; a null array does not match `_any: {_eq: null}`.

# `go.query.inline_array.filter.none`: `_none` on inline arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.test-query-inline-boolean-array-with-none-filter", "query.inline_array.test-query-inline-float-array-with-none-filter", "query.inline_array.test-query-inline-int-array-with-none-filter", "query.inline_array.test-query-inline-non-null-boolean-array-with-none-filter", "query.inline_array.test-query-inline-non-null-float-array-with-none-filter", "query.inline_array.test-query-inline-non-null-int-array-with-none-filter", "query.inline_array.test-query-inline-non-null-string-array-with-none-filter", "query.inline_array.test-query-inline-string-array-with-none-filter"]

`_none` returns docs where no element matches (`_none: {_eq: null}` keeps arrays without nulls, `_none: {_gt: 70}` keeps `[0, 60]`, `_none: {_neq: true}` keeps `[true, true]`) for String, Int, Float and Boolean arrays.

# `go.query.inline_array.filter.eq`: Whole-array equality

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.test-query-inline-boolean-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-float-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-integer-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-nullable-boolean-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-nullable-float-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-nullable-integer-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-nullable-string-array-with-eq-filter-returns-results", "query.inline_array.test-query-inline-string-array-with-eq-filter-returns-results"]

`{field: {_eq: [..]}}` matches docs whose array equals the literal element by element, including null elements (`[90, null, 85]`), for Boolean, Int, Float and String arrays.

# `go.query.inline_array.filter.neq`: Whole-array inequality

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.test-query-inline-boolean-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-float-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-integer-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-nullable-boolean-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-nullable-float-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-nullable-integer-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-nullable-string-array-with-neq-filter-returns-results", "query.inline_array.test-query-inline-string-array-with-neq-filter-returns-results"]

`{field: {_neq: [..]}}` returns the docs whose array differs from the literal, for Boolean, Int, Float and String arrays with or without null elements.

# `go.query.inline_array.filter.datetime`: Filtering DateTime arrays

- **Parents:** ["go.query.inline_array.filter"]
- **Scenarios:** ["query.inline_array.with-date-time-filter-all", "query.inline_array.with-date-time-filter-any", "query.inline_array.with-date-time-filter-none", "query.inline_array.with-date-time-index", "query.inline_array.with-nillable-date-time-filter-any"]

`_any`, `_all` and `_none` on `[DateTime!]` / `[DateTime]` compare elements as timestamps against RFC 3339 strings (`_all: {_geq: "2024-01-15T09:00:00Z"}`, `_none: {_lt: "2024-01-01T00:00:00Z"}`), skip null elements, and return the same result when the array field has an `@index`.

# `go.query.inline_array.aggregate.count.basic`: Plain COUNT

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Scenarios:** ["query.inline_array.test-query-inline-integer-array-with-count-and-empty-array", "query.inline_array.test-query-inline-integer-array-with-count-and-null-array", "query.inline_array.test-query-inline-integer-array-with-count-and-populated-array", "query.inline_array.test-query-inline-nillable-bool-array-with-count-and-populated-array"]

`COUNT(favouriteIntegers: {})` returns 0 for a null or empty array and the number of elements otherwise; null elements are counted (`[true, true, false, null]` gives 4).

# `go.query.inline_array.aggregate.count.filter`: COUNT with an element filter

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Scenarios:** ["query.inline_array.test-query-inline-bool-array-with-count-with-filter", "query.inline_array.test-query-inline-date-time-array-with-count-with-filter", "query.inline_array.test-query-inline-float-array-with-count-with-filter", "query.inline_array.test-query-inline-float32-array-with-count-with-filter", "query.inline_array.test-query-inline-float64-array-with-count-with-filter", "query.inline_array.test-query-inline-integer-array-with-count-with-filter", "query.inline_array.test-query-inline-integer-array-withs-with-count-with-and-filter-and-populated-array", "query.inline_array.test-query-inline-nillable-bool-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-date-time-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-float-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-float32-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-float64-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-integer-array-with-count-with-filter", "query.inline_array.test-query-inline-nillable-string-array-with-count-with-filter", "query.inline_array.test-query-inline-string-array-with-count-with-filter"]

`COUNT(field: {filter: {...}})` counts only elements matching the condition (`_eq`, `_gt`, `_lt`, `_in`, `_and` of ranges), for Boolean, Int, Float, Float32, Float64, String and DateTime arrays; null elements never match a value condition.

# `go.query.inline_array.aggregate.count.paging`: COUNT with limit and offset

- **Parents:** ["go.query.inline_array.aggregate.count"]
- **Scenarios:** ["query.inline_array.test-query-inline-integer-array-with-count-with-limit", "query.inline_array.test-query-inline-integer-array-with-count-with-limit-greater-than-length", "query.inline_array.test-query-inline-integer-array-with-count-with-offset-with-limit", "query.inline_array.test-query-inline-integer-array-with-count-with-offset-with-limit-greater-than-length"]

`COUNT(field: {limit: 3})` returns min(limit, length) and `{offset: 1, limit: 3}` counts at most `limit` elements after skipping `offset` (2 for a 3-element array).

# `go.query.inline_array.aggregate.sum.basic`: Plain SUM

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-sum-and-empty-array", "query.inline_array.test-query-inline-float-array-with-sum-and-null-array", "query.inline_array.test-query-inline-float-array-with-sum-and-populated-array", "query.inline_array.test-query-inline-integer-array-with-sum-and-empty-array", "query.inline_array.test-query-inline-integer-array-with-sum-and-null-array", "query.inline_array.test-query-inline-integer-array-with-sum-and-populated-array", "query.inline_array.test-query-inline-nillable-float-array-with-sum-and-populated-array", "query.inline_array.test-query-inline-nillable-integer-array-with-sum-and-populated-array"]

`SUM(field: {})` returns an Int for Int arrays and a Float for Float arrays, 0 for a null or empty array, and ignores null elements (`[-1, 2, null, 1, 0]` gives 2; `[3.1425, 0.00000000001, 10]` gives 13.14250000001).

# `go.query.inline_array.aggregate.sum.filter`: SUM with an element filter

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-sum-with-filter", "query.inline_array.test-query-inline-integer-array-with-sum-with-filter", "query.inline_array.test-query-inline-nillable-float-array-with-sum-with-filter", "query.inline_array.test-query-inline-nillable-integer-array-with-sum-with-filter"]

`SUM(field: {filter: {_gt: 0}})` sums only matching elements (3 for `[-1, 2, -1, 1, 0]`; 3.14250000001 for floats `_lt: 9`), in nillable and non-nillable Int and Float arrays.

# `go.query.inline_array.aggregate.sum.paging`: SUM with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.sum"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-sum-with-offset-with-limit-with-order-asc", "query.inline_array.test-query-inline-float-array-with-sum-with-offset-with-limit-with-order-desc", "query.inline_array.test-query-inline-integer-array-with-sum-with-limit", "query.inline_array.test-query-inline-integer-array-with-sum-with-offset-with-limit", "query.inline_array.test-query-inline-integer-array-with-sum-with-offset-with-limit-with-order-asc", "query.inline_array.test-query-inline-integer-array-with-sum-with-offset-with-limit-with-order-desc", "query.inline_array.test-query-inline-nillable-float-array-with-sum-with-offset-with-limit-with-order-asc", "query.inline_array.test-query-inline-nillable-float-array-with-sum-with-offset-with-limit-with-order-desc", "query.inline_array.test-query-inline-nillable-integer-array-with-sum-with-offset-with-limit-with-order-asc", "query.inline_array.test-query-inline-nillable-integer-array-with-sum-with-offset-with-limit-with-order-desc"]

Without `order`, `limit`/`offset` take elements in stored order (`{limit: 2}` on `[-1, 2, ...]` gives 1; `{offset: 1, limit: 2}` on `[-1, 2, 5, ...]` gives 7). With `order: ASC|DESC` elements are sorted first, nulls dropped, then offset and limit applied (`[-1, 2, 5, 1, 0, 7]` with offset 1, limit 3: ASC 0+1+2 = 3, DESC 5+2+1 = 8).

# `go.query.inline_array.aggregate.avg.basic`: Plain AVG

- **Parents:** ["go.query.inline_array.aggregate.avg"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-average-and-empty-array", "query.inline_array.test-query-inline-float-array-with-average-and-null-array", "query.inline_array.test-query-inline-float-array-with-average-and-populated-array", "query.inline_array.test-query-inline-float-array-with-average-and-zero-array", "query.inline_array.test-query-inline-integer-array-with-average-and-empty-array", "query.inline_array.test-query-inline-integer-array-with-average-and-null-array", "query.inline_array.test-query-inline-integer-array-with-average-and-populated-array", "query.inline_array.test-query-inline-integer-array-with-average-and-zero-array", "query.inline_array.test-query-inline-nillable-float-array-with-average-and-populated-array", "query.inline_array.test-query-inline-nillable-integer-array-with-average-and-populated-array"]
- **Go-Bug-Suspect:** "AVG of a null or empty array is asserted as 0, while MIN and MAX of the same input return null; an average over no values is undefined"

`AVG(field: {})` returns a Float: 0 for a null, empty or all-zero array, and the mean of the non-null elements otherwise (`[-1, null, 13, 0]` gives 4; `[-0.1, 0, 0.9, 0]` gives 0.2).

# `go.query.inline_array.aggregate.avg.filter`: AVG with a filter, and AVG alongside SUM

- **Parents:** ["go.query.inline_array.aggregate.avg"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-average-with-filter", "query.inline_array.test-query-inline-integer-array-with-average-and-sum", "query.inline_array.test-query-inline-integer-array-with-average-with-filter", "query.inline_array.test-query-inline-nillable-float-array-with-average-with-filter", "query.inline_array.test-query-inline-nillable-integer-array-with-average-with-filter"]

`AVG(field: {filter: ...})` averages only matching non-null elements (`_gt: 0` on `[-1, 2, -1, 1, 0]` gives 1.5; `_gt: -1` on `[-1, null, 13, 0]` gives 6.5). `AVG` and `SUM` selected together on the same array under `groupBy: [name]` return 2 and 8 for `[-1, 0, 9, 0]`.

# `go.query.inline_array.aggregate.min.basic`: Plain MIN

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-min-and-empty-array-succeeds", "query.inline_array.test-query-inline-float-array-with-min-and-null-array-succeeds", "query.inline_array.test-query-inline-float-array-with-min-and-populated-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-and-empty-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-and-null-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-and-populated-array-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-doc-idand-min-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-min-and-populated-array-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-min-and-populated-array-succeeds"]

`MIN(field: {})` returns null for a null or empty array and the smallest non-null element otherwise (`-1`; `0.00000000001`), with the element type preserved, including when the parent is selected by `docID`.

# `go.query.inline_array.aggregate.min.filter`: MIN with a filter

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-min-with-filter-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-with-filter-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-min-with-filter-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-min-with-filter-succeeds"]

`MIN(field: {filter: {_gt: 0}})` returns the smallest matching element (1 for `[-1, 2, -1, 1, 0]`; 3.1425 for floats `_gt: 1`).

# `go.query.inline_array.aggregate.min.paging`: MIN with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.min"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-min-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-float-array-with-min-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-with-offset-with-limit-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-min-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-min-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-min-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-min-with-offset-with-limit-with-order-desc-succeeds"]

MIN is taken over the window selected by offset/limit: in stored order without `order` (`{offset: 1, limit: 2}` on `[-1, 2, 5, 1, 0]` gives 2), or after sorting (nulls dropped) with `order` (`[-1, 2, 5, 1, 0, 7]`, offset 1, limit 3: ASC gives 0, DESC gives 1).

# `go.query.inline_array.aggregate.max.basic`: Plain MAX

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-max-and-empty-array-succeeds", "query.inline_array.test-query-inline-float-array-with-max-and-null-array-succeeds", "query.inline_array.test-query-inline-float-array-with-max-and-populated-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-and-empty-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-and-null-array-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-and-populated-array-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-doc-idand-max-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-doc-idmax-and-populated-array-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-max-and-populated-array-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-max-and-populated-array-succeeds"]

`MAX(field: {})` returns null for a null or empty array and the largest non-null element otherwise (2; 10.0), with the element type preserved, including when the parent is selected by `docID`.

# `go.query.inline_array.aggregate.max.filter`: MAX with a filter

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-max-with-filter-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-with-filter-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-max-with-filter-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-max-with-filter-succeeds"]

`MAX(field: {filter: {_lt: 2}})` returns the largest matching element (1 for `[-1, 2, -1, 1, 0]`; 3.1425 for floats `_lt: 9`).

# `go.query.inline_array.aggregate.max.paging`: MAX with limit, offset and order

- **Parents:** ["go.query.inline_array.aggregate.max"]
- **Scenarios:** ["query.inline_array.test-query-inline-float-array-with-max-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-float-array-with-max-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-with-offset-with-limit-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-max-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-nillable-float-array-with-max-with-offset-with-limit-with-order-desc-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-max-with-offset-with-limit-with-order-asc-succeeds", "query.inline_array.test-query-inline-nillable-integer-array-with-max-with-offset-with-limit-with-order-desc-succeeds"]

MAX is taken over the offset/limit window: stored order without `order` (`{offset: 1, limit: 2}` on `[-1, 2, 5, 1, 0]` gives 5), sorted with nulls dropped when `order` is given (`[-1, 2, 5, 1, 0, 7]`, offset 1, limit 3: ASC gives 2, DESC gives 5).

# `go.query.inline_array.aggregate.alias-order`: Ordering parents by an aggregate alias

- **Parents:** ["go.query.inline_array.aggregate"]
- **Scenarios:** ["query.inline_array.test-query-inline-integer-array-with-average-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-count-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-max-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-min-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-null-and-count-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-null-and-max-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-null-and-min-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-null-and-sum-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-null-with-average-and-order-succeeds", "query.inline_array.test-query-inline-integer-array-with-sum-and-order-succeeds"]

An aggregate over two array fields (`total: SUM(testScores: {}, pageRatings: {})`, likewise COUNT, AVG, MIN, MAX) combines elements of both arrays, and `Users(order: {_alias: {total: ASC|DESC}})` orders docs by it. Null elements are ignored by SUM, AVG, MIN and MAX but counted by COUNT (`[3,4,5,null]` + `[1,2,3,null]` counts 8).

# `go.query.inline_array.group`: Grouping with inline arrays

- **Parents:** ["go.query.inline_array"]
- **Scenarios:** ["query.inline_array.with-group-by-array", "query.inline_array.with-group-by-string"]

Array fields can be selected inside `GROUP` of a `groupBy: [name]` query, and `groupBy: [favouriteIntegers]` groups docs whose arrays are element-wise equal.

# `go.query.one_to_many.join`: Resolving the relation in both directions

- **Parents:** ["go.query.one_to_many"]
- **Scenarios:** ["query.one_to_many.one-sided", "query.one_to_many.primary-direction", "query.one_to_many.secondary-direction", "query.one_to_many.with-non-existant-parent", "query.one_to_many.with-same-field-name-multi-side", "query.one_to_many.with-same-field-name-single-side", "query.one_to_many.with-type-name"]

`Book { author { ... } }` returns the related author object, or null when the book has none; `Author { published { ... } }` returns the list of books whose `_authorID` points at the author. A relation declared only on the many side (`Book.author` without `Author.published`) still resolves, the relation field may share its name on both types (`relationship1`), and `__typename` returns `Book` / `Author` at each level.

# `go.query.one_to_many.relation-id`: The `_<field>ID` relation field

- **Parents:** ["go.query.one_to_many"]
- **Scenarios:** ["query.one_to_many.with-id-field-on-primary", "query.one_to_many.with-related-type-idfrom-many-side", "query.one_to_many.with-related-type-idfrom-single-side"]

`Book { _authorID }` returns the related author's docID. Selecting `_authorID` on `Author` errors with `Cannot query field "_authorID" on type "Author".`; declaring a field named `_authorID` alongside `author: Author` fails schema creation with `duplicate field. Name: _authorID`.

# `go.query.one_to_many.filter.related-id`: Filtering by related docID

- **Parents:** ["go.query.one_to_many.filter"]
- **Scenarios:** ["query.one_to_many.test-query-from-many-side-with-eq-filter-on-related-type", "query.one_to_many.test-query-from-many-side-with-filter-on-related-object-id", "query.one_to_many.test-query-from-many-side-with-same-filters-in-different-way-on-related-type", "query.one_to_many.test-query-from-single-side-with-eq-filter-on-related-type", "query.one_to_many.test-query-from-single-side-with-filter-on-related-object-id-error"]

`Book(filter: {author: {_docID: {_eq: X}}})`, `Book(filter: {_authorID: {_eq: X}})` and both together return the books of author X; `Author(filter: {published: {_docID: {_eq: B}}})` returns the author of book B. `Author(filter: {_publishedID: ...})` errors with `Unknown field.`.

# `go.query.one_to_many.filter.parent-child`: Parent filters, child filters and compound relation filters

- **Parents:** ["go.query.one_to_many.filter"]
- **Scenarios:** ["query.one_to_many.with-alias-filter-on-related-succeeds", "query.one_to_many.with-compound-operator-in-filter-and-relation", "query.one_to_many.with-compound-operator-in-filter-and-relation-and-case-insensitive-like-no-error", "query.one_to_many.with-multiple-aliased-filtered-children", "query.one_to_many.with-numeric-greater-than-child-filter-on-parent-with-unrendered-child", "query.one_to_many.with-numeric-greater-than-filter-on-parent", "query.one_to_many.with-numeric-greater-than-filter-on-parent-and-child"]

A parent filter on a child field (`Author(filter: {published: {rating: {_gt: 4.8}}})`, also via an alias `{_alias: {books: {...}}}`) keeps authors with at least one matching book but still renders all their books; a filter on the child selection (`published(filter: ...)`) narrows only the rendered list, and two aliased children with different filters are independent. Relation conditions nest in `_and`, `_or` and `_not` (e.g. `_and: [{_not: {published: {rating: {_gt: 4.8}}}}, {_not: {published: {rating: {_lt: 4.8}}}}]` returns only the author whose only book is rated 4.8) and accept `_like` / `_ilike`.

# `go.query.one_to_many.child-docid`: Selecting children by docID

- **Parents:** ["go.query.one_to_many"]
- **Scenarios:** ["query.one_to_many.with-child-doc-id", "query.one_to_many.with-child-doc-ids"]

`published(docID: X)` renders only that child and `published(docID: [X, Y])` only those children.

# `go.query.one_to_many.time-travel`: Querying a parent at a commit with its relation

- **Parents:** ["go.query.one_to_many"]
- **Scenarios:** ["query.one_to_many.with-child-update-and-first-cid-and-doc-id", "query.one_to_many.with-cid-and-doc-id", "query.one_to_many.with-parent-update-and-first-cid-and-doc-id", "query.one_to_many.with-parent-update-and-last-cid-and-doc-id"]
- **Go-Bug-Suspect:** "TestQueryOneToManyWithChildUpdateAndFirstCidAndDocID is marked 'for documentation reasons only. This is not desired behaviour': the related author renders at its latest state (age 22), not as of the book's commit"

`Book(cid: C, docID: X) { author { ... } }` returns the book's fields as of commit C (first CID gives rating 4.9 after an update to 4.5; last CID gives 4.5) while the related author is rendered at its current state.

# `go.query.one_to_many.child-order-paging`: Ordering and limiting rendered children

- **Parents:** ["go.query.one_to_many"]
- **Scenarios:** ["query.one_to_many.with-multiple-child-limits", "query.one_to_many.with-numeric-greater-than-filter-and-numeric-sort-descending-on-child", "query.one_to_many.with-numeric-greater-than-filter-on-parent-and-numeric-sort-ascending-and-limit-on-child", "query.one_to_many.with-numeric-greater-than-filter-on-parent-and-numeric-sort-ascending-on-child", "query.one_to_many.with-numeric-greater-than-filter-on-parent-and-numeric-sort-descending-and-limit-on-child", "query.one_to_many.with-single-child-limit"]

`published(order: {rating: ASC|DESC}, limit: N)` sorts and truncates each parent's child list independently of the parent filter and order; aliased children (`p1: published(limit: 1)`, `p2: published(limit: 2)`) apply their own limits.

# `go.query.one_to_many.count.basic`: Plain COUNT, alias filter and alias order

- **Parents:** ["go.query.one_to_many.count"]
- **Scenarios:** ["query.one_to_many.with-count-alias-filter-should-match-all", "query.one_to_many.with-count-alias-filter-should-match-one", "query.one_to_many.with-count-alias-order-should-order-results", "query.one_to_many.with-count-nothing-to-count", "query.one_to_many.with-count-should-match-all"]

`COUNT(published: {})` returns the number of related books (0 when none). An aliased count can filter parents (`Author(filter: {_alias: {publishedCount: {_gt: 1}}})`) and order them (`order: {_alias: {publishedCount: DESC}}`).

# `go.query.one_to_many.count.filter`: COUNT with a child filter

- **Parents:** ["go.query.one_to_many.count"]
- **Scenarios:** ["query.one_to_many.with-count-with-filter", "query.one_to_many.with-count-with-filter-and-child-filter", "query.one_to_many.with-count-with-jsonfilter-and-child-filter-succeeds"]

`COUNT(published: {filter: {rating: {_gt: 4.8}}})` counts only matching children (`_neq: null` excludes books without a rating) and coexists with a separately filtered child selection; a top-level `COUNT(Author: {filter: {metadata: {yearOfBirth: {_eq: 1958}}, published: {name: {_ilike: "%lord%"}}}})` combines a JSON path filter with a relation filter (returns 1).

# `go.query.one_to_many.count.groupby`: COUNT of grouped children

- **Parents:** ["go.query.one_to_many.count"]
- **Scenarios:** ["query.one_to_many.with-count-with-group-by-all-same-name", "query.one_to_many.with-count-with-group-by-counting-distinct-names", "query.one_to_many.with-count-with-group-by-counting-zero"]

`COUNT(published: {groupBy: [name]})` counts distinct child groups: 0 with no books, 2 for three books with two distinct names, 1 when all share a name.

# `go.query.one_to_many.count.paging`: COUNT with limit and offset

- **Parents:** ["go.query.one_to_many.count"]
- **Scenarios:** ["query.one_to_many.with-count-and-different-limits", "query.one_to_many.with-count-and-different-offsets", "query.one_to_many.with-count-and-limit", "query.one_to_many.with-count-and-limit-and-offset", "query.one_to_many.with-count-with-limit", "query.one_to_many.with-count-with-limit-with-offset"]

`COUNT(published: {limit: N, offset: M})` counts at most N children after skipping M, independently of the `limit`/`offset` on a rendered `published(...)` selection (an unbounded `COUNT(published: {})` still reports 4 while `published(limit: 2, offset: 1)` renders 2).

# `go.query.one_to_many.aggregate.alias`: Aggregates used through aliases

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Scenarios:** ["query.one_to_many.with-average-alias-filter-should-match-all", "query.one_to_many.with-average-alias-filter-should-match-one", "query.one_to_many.with-max-alias-filter-should-match-all", "query.one_to_many.with-max-alias-filter-should-match-one", "query.one_to_many.with-min-alias-filter-should-match-all", "query.one_to_many.with-min-alias-filter-should-match-one", "query.one_to_many.with-sum-alias-filter-on-float32-should-match-one", "query.one_to_many.with-sum-alias-filter-should-match-all", "query.one_to_many.with-sum-alias-filter-should-match-one", "query.one_to_many.with-sum-with-alias-order-should-order-results"]
- **Go-Bug-Suspect:** "SUM over a 'Float32' field is asserted as 9.400000095367432 (float32 values widened to float64 before summing), not 9.4"

`averageRating: AVG(published: {field: rating})` (and MIN, MAX, SUM) returns the per-author aggregate (AVG 4.7 for 4.9 and 4.5; MIN 4.5; MAX 4.9; SUM 9.4), and the alias can filter authors (`filter: {_alias: {totalRating: {_gt: 5}}}`) or order them (`order: {_alias: {totalRating: DESC}}`).

# `go.query.one_to_many.aggregate.sum-ordered-paging`: SUM with order, limit, offset and parent filters

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Scenarios:** ["query.one_to_many.test-one-to-many-asc-order-and-filter-on-parent-with-agg-sum-on-sub-type-field", "query.one_to_many.test-one-to-many-desc-order-and-filter-on-parent-with-agg-sum-on-sub-type-field", "query.one_to_many.test-one-to-many-limit-asc-order-sum-of-sub-type-and-limit-asc-order-fields-of-subtype", "query.one_to_many.test-one-to-many-limit-desc-order-sum-of-sub-type-and-limit-asc-order-fields-of-subtype", "query.one_to_many.test-oneto-many-sum-by-sub-type-field-and-sum-by-syb-type-field-with-asc-ordering-on-field-with-limit", "query.one_to_many.test-oneto-many-sum-by-sub-type-field-and-sum-by-syb-type-field-with-desc-ordering-on-field-with-limit", "query.one_to_many.with-sum-with-limit-with-offset-with-order-asc", "query.one_to_many.with-sum-with-limit-with-offset-with-order-asc-and-desc", "query.one_to_many.with-sum-with-limit-with-offset-with-order-desc", "query.one_to_many.with-sum-with-limit-with-offset-with-order-desc-and-rendered-children-ordered-asc", "query.one_to_many.with-sum-with-limit-with-offset-with-order-on-different-fields"]

`SUM(published: {field: rating, order: {name|rating: ASC|DESC}, offset: M, limit: N})` sums the children in that window (DESC by rating, limit 2 gives 4.9 + 4.5); several aliased SUMs with different windows, and a rendered `published(...)` with its own order and window, are evaluated independently. A SUM over no children is 0.0, and parent `filter` and `order` apply to the authors as usual.

# `go.query.one_to_many.aggregate.sum-unordered-paging`: SUM with limit and offset but no order

- **Parents:** ["go.query.one_to_many.aggregate"]
- **Scenarios:** ["query.one_to_many.with-sum-with-limit", "query.one_to_many.with-sum-with-limit-and-offset"]
- **Go-Bug-Suspect:** "without 'order' the window depends on internal child ordering (the test comment says 'internal ordering selects Painted House + A Time for Mercy'); the expected sums 9.4 and 8.7 encode docID order, not any defined order"

`SUM(published: {field: rating, limit: 2})` and `{offset: 1, limit: 2}` sum the children in Go's internal (docID) order: 9.4 and 8.7 for John Grisham, and 4.8 / 0 for an author with one book.

# `go.query.one_to_one.join`: Resolving the relation in both directions

- **Parents:** ["go.query.one_to_one"]
- **Scenarios:** ["query.one_to_one.primary-direction", "query.one_to_one.secondary-direction", "query.one_to_one.with-fragment", "query.one_to_one.with-fragment-with-object-with-fragment", "query.one_to_one.with-multiple-records", "query.one_to_one.with-multiple-records-secondary-direction", "query.one_to_one.with-nil-child", "query.one_to_one.with-nil-parent"]

From either side the relation renders as a single object (not a list) for each of several linked pairs, and as null when no counterpart exists. Relation selections work inside named fragments, including a fragment nested in another fragment's object.

# `go.query.one_to_one.relation-id`: Relation ID fields

- **Parents:** ["go.query.one_to_one"]
- **Scenarios:** ["query.one_to_one.with-clashing-id-field-on-primary", "query.one_to_one.with-clashing-id-field-on-secondary", "query.one_to_one.with-relation-idfrom-primary-side", "query.one_to_one.with-relation-idfrom-secondary-side"]

Both sides of a one-to-one expose the counterpart's docID: `Author { _publishedID }` on the primary side and `Book { _authorID }` on the secondary side. Declaring a field `_authorID` next to `author: Author` fails with `duplicate field. Name: _authorID` whichever side is primary.

# `go.query.one_to_one.filter`: Filtering across the relation

- **Parents:** ["go.query.one_to_one"]
- **Scenarios:** ["query.one_to_one.with-boolean-filter-on-child", "query.one_to_one.with-boolean-filter-on-child-with-no-sub-type-selection", "query.one_to_one.with-compound-and-filter-that-includes-relation", "query.one_to_one.with-compound-filters-that-includes-relation-should-return-results", "query.one_to_one.with-compound-or-filter-that-includes-relation", "query.one_to_one.with-count-with-compound-or-filter-that-includes-relation", "query.one_to_one.with-filter-through-child-back-to-parent", "query.one_to_one.with-numeric-filter-on-parent", "query.one_to_one.with-string-filter-on-child"]

`Book(filter: {author: {verified: {_eq: true}}})` filters books by their author's fields even when `author` is not selected, a filter can round-trip (`{author: {published: {rating: {_eq: 4.9}}}}`), and `author(filter: ...)` filters the rendered child. Relation conditions combine with scalar ones under `_and`, `_or` and `_not` (a `_not` over an object means not-all of its conditions), also inside `COUNT(Book: {filter: ...})` (returns 2).

# `go.query.one_to_one.group`: Grouping by the one-to-one relation

- **Parents:** ["go.query.one_to_one"]
- **Scenarios:** ["query.one_to_one.with-group-related-id", "query.one_to_one.with-group-related-idalias", "query.one_to_one.with-group-related-idalias-from-secondary-with-inner-group", "query.one_to_one.with-group-related-idalias-from-secondary-with-inner-group-with-join", "query.one_to_one.with-group-related-idalias-from-secondary-without-inner-group", "query.one_to_one.with-group-related-idalias-from-secondary-without-inner-group-with-join", "query.one_to_one.with-group-related-idfrom-secondary-with-group", "query.one_to_one.with-group-related-idfrom-secondary-with-group-with-join", "query.one_to_one.with-group-related-idfrom-secondary-without-group", "query.one_to_one.with-group-related-idfrom-secondary-without-group-with-join"]

`Book(groupBy: [_authorID])` or `Book(groupBy: [author])` groups books per author whether `Book` or `Author` holds the primary side; the group row may render `_authorID`, the `author` object, and/or `GROUP`.

# `go.query.one_to_one.version`: `_version` alongside a relation

- **Parents:** ["go.query.one_to_one"]
- **Scenarios:** ["query.one_to_one.with-version-on-outer-after-join", "query.one_to_one.with-version-on-outer-before-join"]

`Book { name _version { docID } author { name } }` returns the book's version list with its docID and the related author regardless of whether `_version` is selected before or after the relation (with non-ASCII string values).

# `go.query.many_to_many.join-collection`: Querying through the join collection

- **Parents:** ["go.query.many_to_many"]
- **Scenarios:** ["query.many_to_many.test-many-to-many-query-from-join-collection-should-succeed", "query.many_to_many.test-many-to-many-query-from-secondary-succeeds"]

With `Enrollment { student: Student, course: Course }` (named relations), `Enrollment(filter: {course: {name: {_eq: "Math"}}}, order: {student: {name: ASC}})` returns enrolments in that order, and `Student { enrollments(order: {course: {name: ASC}}) { course { name } } }` traverses from the other side.

# `go.query.one_to_many_multiple.count`: COUNT across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Scenarios:** ["query.one_to_many_multiple.with-count", "query.one_to_many_multiple.with-count-on-multiple-joins", "query.one_to_many_multiple.with-count-on-multiple-joins-with-and-without-filter", "query.one_to_many_multiple.with-count-on-multiple-joins-with-filters"]

Separate `COUNT(books: {})` and `COUNT(articles: {})` count each relation; a single `COUNT(books: {...}, articles: {...})` adds both counts, each source applying its own filter.

# `go.query.one_to_many_multiple.sum`: SUM across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Scenarios:** ["query.one_to_many_multiple.with-sum-on-multiple-joins", "query.one_to_many_multiple.with-sum-on-multiple-joins-with-and-without-filter", "query.one_to_many_multiple.with-sum-on-multiple-joins-with-filters"]

`SUM(books: {field: score}, articles: {field: rating})` returns one Int sum over both relations' fields (9 and 7), each source applying its own filter; with no matching children the sum is 0.

# `go.query.one_to_many_multiple.avg`: AVG across multiple relations

- **Parents:** ["go.query.one_to_many_multiple"]
- **Scenarios:** ["query.one_to_many_multiple.with-average-on-multiple-joins", "query.one_to_many_multiple.with-average-on-multiple-joins-with-and-without-filter", "query.one_to_many_multiple.with-average-on-multiple-joins-with-filters"]

`AVG(books: {field: score}, articles: {field: rating})` averages over the union of selected values (2.25; 2.3333333333333335), each source applying its own filter; with no matching values it returns 0.

# `go.query.one_to_many_multiple.filter`: Filtering on several relations at once

- **Parents:** ["go.query.one_to_many_multiple"]
- **Scenarios:** ["query.one_to_many_multiple.with-multiple-many-filters"]

`Author(filter: {name: {_eq: "John Grisham"}, books: {score: {_eq: 1}}, articles: {rating: {_eq: 3}}})` requires every condition, each relation condition satisfied by some child.

# `go.query.one_to_many_to_many.joins`: Two-hop many relations link correctly

- **Parents:** ["go.query.one_to_many_to_many"]
- **Scenarios:** ["query.one_to_many_to_many.test-one-to-many-to-many-joins-are-linked-properly"]

`Author { book { publisher { ... } } }` with `Book.publisher: [Publisher]` renders each book's own publishers (zero, one or two) under the right author, and authors without books get an empty list.

# `go.query.one_to_many_to_one.joins`: Two-hop links

- **Parents:** ["go.query.one_to_many_to_one"]
- **Scenarios:** ["query.one_to_many_to_one.test-one-to-many-to-one-joins-are-linked-properly", "query.one_to_many_to_one.test-query-one-to-one-relations"]

`Author { book { publisher { ... } } }` and `Book { author { ... } publisher { ... } }` render each book's own publisher, null for a book without one, and an empty list for an author without books.

# `go.query.one_to_many_to_one.deep-filter`: Filters two relations deep

- **Parents:** ["go.query.one_to_many_to_one"]
- **Scenarios:** ["query.one_to_many_to_one.test-one-to-many-to-one-with-compound-operator-in-filter-and-relation", "query.one_to_many_to_one.test-one-to-many-to-one-with-compound-operator-in-sub-filter-and-relation", "query.one_to_many_to_one.test-one-to-many-to-one-with-two-level-deep-filter", "query.one_to_many_to_one.test-query-complex-with-deep-filter-on-rendered-children"]

`Author(filter: {book: {publisher: {yearOpened: {_gt: 2021}}}})` keeps authors with at least one such book but renders all their books; deep conditions combine under `_and`, `_or`, `_not` both in the parent filter and in a `book(filter: ...)` child filter.

# `go.query.one_to_many_to_one.sum`: SUM with deep filters and deep ordering

- **Parents:** ["go.query.one_to_many_to_one"]
- **Scenarios:** ["query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-filter-sub-type-and-deep-order-by-subtype-opposite-directions", "query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-filter-sub-type-of-both-desc-and-asc", "query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-order-by-sub-type-and-deep-order-by-subtype-asc-directions", "query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-order-by-sub-type-and-deep-order-by-subtype-desc-directions", "query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-order-by-sub-type-and-deep-order-by-subtype-opposite-directions", "query.one_to_many_to_one.test-one-to-many-to-one-with-sum-of-deep-order-by-sub-type-of-both-desc-and-asc", "query.one_to_many_to_one.test-query-with-sum-on-inline-and-sum-on-one-to-many-field"]

`SUM(book: {field: rating, filter: {publisher: {yearOpened: ...}}})` sums books whose publisher matches, and `SUM(book: {field: rating, order: {publisher: {yearOpened: ASC|DESC}}, limit: 2})` sums the first two books by publisher year, with books lacking a publisher sorting first ascending (John: DESC 4.8 + 4.5, ASC 4.2 + 4.9); the same window on a rendered `book(...)` selection agrees. Several such sums may be selected, under `query @exhaustive`, alongside a SUM over an inline array field.

# `go.query.one_to_many_to_one.order`: Ordering by a field two relations deep

- **Parents:** ["go.query.one_to_many_to_one"]
- **Scenarios:** ["query.one_to_many_to_one.test-multiple-order-by-with-depth-greater-than-one", "query.one_to_many_to_one.test-multiple-order-by-with-depth-greater-than-one-order-switched", "query.one_to_many_to_one.test-one-to-many-to-one-deep-order-by-sub-type-of-both-desc-and-asc"]
- **Scope:** "TestOneToManyToOneDeepOrderBySubTypeOfBothDescAndAsc excludes the secondary-index multiplier; TestMultipleOrderByWithDepthGreaterThanOneOrderSwitched uses 'query @exhaustive' so books without a publisher are kept under the secondary-index multiplier"

`Book(order: [{rating: ASC}, {publisher: {yearOpened: DESC}}])` and the switched order sort by multiple keys including a related field, with books lacking a publisher sorting last on `yearOpened: DESC`; `book(order: {publisher: {yearOpened: DESC|ASC}}, limit: 1)` under each author picks the newest or oldest (a book with no publisher counts as oldest).

# `go.query.one_to_one_multiple.join`: Resolving two one-to-one relations

- **Parents:** ["go.query.one_to_one_multiple"]
- **Scenarios:** ["query.one_to_one_multiple.from-mixed-primary-and-secondary", "query.one_to_one_multiple.from-primary", "query.one_to_one_multiple.from-secondary"]

`Book { publisher { name } author { name } }` resolves both one-to-one relations whether `Book` is primary for both, one, or neither.

# `go.query.one_to_one_multiple.cyclic-schema`: Cyclic named relations are accepted

- **Parents:** ["go.query.one_to_one_multiple"]
- **Scenarios:** ["query.one_to_one_multiple.test-add-collection-with-cyclic-mutually-referential-relations-does-not-error"]

An SDL with mutually referential named relations (`Player`/`Deck`/`Game` with `deck_player`, `game_players`, `game_decks` and an unnamed `winner: Player`) is added without error.

# `go.query.one_to_two_many.named`: Named parallel relations

- **Parents:** ["go.query.one_to_two_many"]
- **Scenarios:** ["query.one_to_two_many.with-named-and-unnamed-relationships", "query.one_to_two_many.with-named-and-unnamed-relationships-from-many-side", "query.one_to_two_many.with-nil-unnamed-relationship-from-many-side", "query.one_to_two_many.with-nil-unnamed-relationship-from-one-side", "query.one_to_two_many.with-order"]

With `Book.author` / `Author.written` (`written_books`) and `Book.reviewedBy` / `Author.reviewed` (`reviewed_books`), plus an unnamed `Book.price` / `Price.books`, each relation resolves to its own counterpart from both sides, and `written(order: {rating: ASC})` / `reviewed(order: {rating: DESC})` order each list independently.
