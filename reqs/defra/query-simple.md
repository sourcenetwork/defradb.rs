# `go.query.simple.select.unknown-field`: Selecting an undefined field is rejected

- **Parents:** ["go.query.simple.select"]
- **Scenarios:** ["query.simple.with-similarity-on-query-should-error", "query.simple.with-undefined-field"]

Selecting a field the type does not define fails validation with `Cannot query field "ThisFieldDoesNotExists" on type "Users".`; `query { SIMILARITY }` at the root fails with `Cannot query field "SIMILARITY" on type "Query".`.

# `go.query.simple.select.collection-isolation`: A collection query sees only its own documents

- **Parents:** ["go.query.simple.select"]
- **Scenarios:** ["query.simple.test-simple-with-eight-dummy-types-after", "query.simple.test-simple-with-eight-dummy-types-before", "query.simple.test-simple-with-eight-dummy-types-before-in-split-declaration", "query.simple.test-simple-with-seven-dummy-types-before", "query.simple.test-simple-with-seven-dummy-types-before-and-one-after", "query.simple.with-deleted-docs-in-collection2-should-not-yield-deleted-docs-on-collection1-query"]

`query { User { ... } }` returns exactly that collection's live documents however many other collections exist and in whatever order or SDL block they were declared (seven or eight extra types before or after, Go #2810); deleted documents of the next collection never appear in it (Go #3242).

# `go.query.simple.select.alias`: Field and root aliases rename result keys

- **Parents:** ["go.query.simple.select"]
- **Scenarios:** ["query.simple.alias", "query.simple.collection-alias"]

`Users { username: Name age: Age }` returns the values under the alias keys, and `allUsers: Users { ... }` returns the document list under `allUsers`.

# `go.query.simple.select.typename`: `__typename` meta field

- **Parents:** ["go.query.simple.select"]
- **Scenarios:** ["query.simple.typename", "query.simple.typename-alias", "query.simple.with-group-by-with-child-type-name", "query.simple.with-group-by-with-type-name"]

Selecting `__typename` (also under an alias such as `t1: __typename`) returns `"Users"` on every document, on every group row of a `groupBy` query, and on every document inside `GROUP`.

# `go.query.simple.select.show-deleted`: `showDeleted` and the `_deleted` field

- **Parents:** ["go.query.simple.select"]
- **Scenarios:** ["query.simple.with-cid-after-delete-operation-should-return-user", "query.simple.with-deleted-field"]

`User(showDeleted: true) { _deleted name }` returns deleted documents alongside live ones, with `_deleted: true` on the deleted ones; querying with the CID of the delete commit and `showDeleted: true` returns the document with `_deleted: true`.

# `go.query.simple.filter.equality.eq`: `_eq` on a non-null value

- **Parents:** ["go.query.simple.filter.equality"]
- **Scenarios:** ["query.simple.with_filter.filter-float-eq-value", "query.simple.with_filter.filter-int-eq", "query.simple.with_filter.filter-select-different-field", "query.simple.with_filter.filter-select-same-field", "query.simple.with_filter.filter-string-eq", "query.simple.with_filter.filter-string-no-match", "query.simple.with_filter.with-date-time-equals-filter-block", "query.simple.with_filter.with-eq-op-on-blob-field-should-filter", "query.simple.with_filter.with-nil-date-time-equals-and-non-nil-filter-block-should-succeed"]

`filter: {Age: {_eq: 21}}` (and the same on Float, String, DateTime and Blob hex such as `"00FF99AA"`) returns only documents whose field equals the value; documents with the field unset never match, the filtered field need not be selected, and no match returns an empty list.

# `go.query.simple.filter.equality.eq-null`: `_eq: null` matches unset fields

- **Parents:** ["go.query.simple.filter.equality"]
- **Scenarios:** ["query.simple.with_filter.filter-float-eq-null", "query.simple.with_filter.filter-int-eq-null", "query.simple.with_filter.filter-string-eq-null", "query.simple.with_filter.with-date-time-equals-nil-filter-block"]

`filter: {Age: {_eq: null}}` on Int, Float, String and DateTime fields returns exactly the documents whose field is null or unset.

# `go.query.simple.filter.equality.neq`: `_neq` on a non-null value

- **Parents:** ["go.query.simple.filter.equality"]
- **Scenarios:** ["query.simple.with_filter.filter-bool-neq-false", "query.simple.with_filter.filter-bool-neq-true", "query.simple.with_filter.filter-float-neq-value", "query.simple.with_filter.filter-int-neq", "query.simple.with_filter.filter-string-neq-value", "query.simple.with_filter.with-date-time-not-equals-filter-block", "query.simple.with_filter.with-nil-date-time-not-equal-and-non-nil-filter-block-should-succeed"]

`filter: {X: {_neq: v}}` on Int, Float, String, DateTime and Boolean returns every document whose field differs from `v`, including documents where the field is unset (e.g. `Verified: {_neq: true}` returns the `false` and the unset documents).

# `go.query.simple.filter.equality.neq-null`: `_neq: null` matches set fields

- **Parents:** ["go.query.simple.filter.equality"]
- **Scenarios:** ["query.simple.with_filter.filter-bool-neq-null", "query.simple.with_filter.filter-float-neq-null", "query.simple.with_filter.filter-int-neq-null", "query.simple.with_filter.filter-string-neq-null", "query.simple.with_filter.with-date-time-not-equals-nil-filter-block"]

`filter: {X: {_neq: null}}` on Int, Float, String, DateTime and Boolean returns exactly the documents whose field has a value (a Boolean `false` counts as a value).

# `go.query.simple.filter.range.gt`: `_gt` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Scenarios:** ["query.simple.with_filter.filter-float-gt-all", "query.simple.with_filter.filter-float-gt-integer-bound", "query.simple.with_filter.filter-float-gt-none", "query.simple.with_filter.filter-float-gt-one", "query.simple.with_filter.filter-int-gt-all", "query.simple.with_filter.filter-int-gt-none", "query.simple.with_filter.filter-int-gt-one", "query.simple.with_filter.with-date-time-gtfilter-block-with-equal-value", "query.simple.with_filter.with-date-time-gtfilter-block-with-greater-value", "query.simple.with_filter.with-date-time-gtfilter-block-with-lesser-value", "query.simple.with_filter.with-nil-date-time-gtand-non-nil-filter-block-should-succeed"]

`filter: {Age: {_gt: 20}}` returns documents whose value is strictly greater (zero, one or all may match); a Float field accepts an Int operand (`HeightM: {_gt: 2}`), DateTime compares RFC 3339 instants across offsets, and documents with the field unset never match a non-null operand.

# `go.query.simple.filter.range.geq`: `_geq` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Scenarios:** ["query.simple.with_filter.filter-float-geq-equal", "query.simple.with_filter.filter-float-geq-greater", "query.simple.with_filter.filter-float-geq-integer-bound", "query.simple.with_filter.filter-int-geq-equal", "query.simple.with_filter.filter-int-geq-greater", "query.simple.with_filter.with-date-time-gefilter-block-with-equal-value", "query.simple.with_filter.with-date-time-gefilter-block-with-greater-value", "query.simple.with_filter.with-date-time-gefilter-block-with-lesser-value", "query.simple.with_filter.with-nil-date-time-geand-non-nil-filter-block-should-succeed"]

`filter: {Age: {_geq: 32}}` returns documents whose value is greater than or equal to the operand, an equal value included; Float accepts Int operands and fine-grained values like `2.0999999999999`; documents with the field unset do not match.

# `go.query.simple.filter.range.lt`: `_lt` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Scenarios:** ["query.simple.with_filter.filter-float-lt-integer-bound", "query.simple.with_filter.filter-float-lt-one", "query.simple.with_filter.filter-int-lt-one", "query.simple.with_filter.with-date-time-ltfilter-block-with-greater-value", "query.simple.with_filter.with-nil-date-time-ltand-non-nil-filter-block-should-succeed"]

`filter: {Age: {_lt: 22}}` returns documents whose value is strictly less than the operand (Int operand accepted on Float); documents with the field unset do not match a non-null operand.

# `go.query.simple.filter.range.leq`: `_leq` on a non-null value

- **Parents:** ["go.query.simple.filter.range"]
- **Scenarios:** ["query.simple.with_filter.filter-float-leq-equal", "query.simple.with_filter.filter-float-leq-integer-bound", "query.simple.with_filter.filter-float-leq-less", "query.simple.with_filter.filter-int-leq-equal", "query.simple.with_filter.filter-int-leq-less", "query.simple.with_filter.with-date-time-lefilter-block-with-equal-value", "query.simple.with_filter.with-date-time-lefilter-block-with-greater-value", "query.simple.with_filter.with-nil-date-time-leand-non-nil-filter-block-should-succeed"]

`filter: {Age: {_leq: 21}}` returns documents whose value is less than or equal to the operand, an equal value included (Int operand accepted on Float); documents with the field unset do not match a non-null operand.

# `go.query.simple.filter.range.null-operand`: Range operators with a `null` operand

- **Parents:** ["go.query.simple.filter.range"]
- **Scenarios:** ["query.simple.with_filter.filter-float-geq-null", "query.simple.with_filter.filter-float-gt-null", "query.simple.with_filter.filter-float-leq-null", "query.simple.with_filter.filter-float-lt-null", "query.simple.with_filter.filter-int-geq-null", "query.simple.with_filter.filter-int-gt-null", "query.simple.with_filter.filter-int-leq-null", "query.simple.with_filter.filter-int-lt-null", "query.simple.with_filter.with-date-time-gefilter-block-with-nil-value", "query.simple.with_filter.with-date-time-gtfilter-block-with-nil-value", "query.simple.with_filter.with-date-time-lefilter-block-with-null-value", "query.simple.with_filter.with-date-time-ltfilter-block-with-null-value"]

On Int, Float and DateTime, `null` sorts below every value: `{_gt: null}` returns documents with the field set, `{_geq: null}` returns all documents, `{_lt: null}` returns none, and `{_leq: null}` returns only documents with the field unset.

# `go.query.simple.filter.membership`: `_in` and `_nin`

- **Parents:** ["go.query.simple.filter"]
- **Scenarios:** ["query.simple.with_filter.with-in-op-on-blob-field-should-filter", "query.simple.with_filter.with-int-in-filter", "query.simple.with_filter.with-int-in-filter-on-float", "query.simple.with_filter.with-int-in-filter-with-null-value", "query.simple.with_filter.with-not-in-filter"]

`filter: {Age: {_in: [19, 40, 55]}}` returns documents whose value is in the list; a `null` element also matches documents with the field unset, Int elements match equal Float values (`HeightM: {_in: [21, 21.2]}` matches 21.0 and 21.2), and Blob hex strings work. `_nin: [19, 40, 55, null]` returns documents whose value is set and not in the list.

# `go.query.simple.filter.pattern.like`: `_like` and `_ilike`

- **Parents:** ["go.query.simple.filter.pattern"]
- **Scenarios:** ["query.simple.with_filter.with-case-insensitive-like-should-match-string", "query.simple.with_filter.with-case-insensitive-like-string-should-match-exact-string", "query.simple.with_filter.with-case-insensitive-like-string-should-match-prefix-string", "query.simple.with_filter.with-case-insensitive-like-string-should-match-suffix-string", "query.simple.with_filter.with-like-op-on-blob-field-should-filter", "query.simple.with_filter.with-like-string-contains-filter-block-as-prefix-string", "query.simple.with_filter.with-like-string-contains-filter-block-as-suffix-string", "query.simple.with_filter.with-like-string-contains-filter-block-contains-string", "query.simple.with_filter.with-like-string-contains-filter-block-contains-string-muplitple-results", "query.simple.with_filter.with-like-string-contains-filter-block-exact-string", "query.simple.with_filter.with-like-string-contains-filter-block-has-both", "query.simple.with_filter.with-like-string-contains-filter-block-has-either", "query.simple.with_filter.with-like-string-contains-filter-block-has-start-and-end", "query.simple.with_filter.with-like-string-contains-filter-block-prop-not-set"]

`_like` matches contains (`"%Stormborn%"`), prefix (`"Viserys%"`), suffix (`"%Andals"`), exact and start-and-end (`"Daenerys%Name"`) patterns case-sensitively; `_ilike` does the same ignoring case. Patterns combine under `_and`/`_or`, documents with the field unset never match, and `_like: "%FF99%"` matches a Blob by its hex form.

# `go.query.simple.filter.pattern.nlike`: `_nlike` and `_nilike`

- **Parents:** ["go.query.simple.filter.pattern"]
- **Scenarios:** ["query.simple.with_filter.with-not-case-insensitive-like-string-match-exact-string", "query.simple.with_filter.with-not-case-insensitive-like-string-should-match-prefix-string", "query.simple.with_filter.with-not-case-insensitive-like-string-should-match-string", "query.simple.with_filter.with-not-case-insensitive-like-string-should-match-suffix-string", "query.simple.with_filter.with-not-like-string-contains-filter-block-as-prefix-string", "query.simple.with_filter.with-not-like-string-contains-filter-block-as-suffix-string", "query.simple.with_filter.with-not-like-string-contains-filter-block-contains-string", "query.simple.with_filter.with-not-like-string-contains-filter-block-contains-string-muplitple-results", "query.simple.with_filter.with-not-like-string-contains-filter-block-exact-string", "query.simple.with_filter.with-not-like-string-contains-filter-block-has-both", "query.simple.with_filter.with-not-like-string-contains-filter-block-has-either", "query.simple.with_filter.with-not-like-string-contains-filter-block-has-start-and-end", "query.simple.with_filter.with-not-like-string-contains-filter-block-prop-not-set"]

`_nlike` (case-sensitive) and `_nilike` (case-insensitive) return the documents that the matching `_like`/`_ilike` pattern would not, for contains, prefix, suffix, exact and start-and-end patterns and under `_and`/`_or`.

# `go.query.simple.filter.logical.and-or`: `_and` and `_or`, including list-field `_all`/`_any`

- **Parents:** ["go.query.simple.filter.logical"]
- **Scenarios:** ["query.simple.with_filter.filter-and", "query.simple.with_filter.filter-or", "query.simple.with_filter.with-inline-int-array-equal-to-xor-yfilter-succeeds", "query.simple.with_filter.with-inline-int-array-greater-than-and-less-than-filter-succeeds"]

`_and: [{Age: {_gt: 20}}, {Age: {_lt: 50}}]` returns documents matching every condition and `_or: [...]` documents matching any; on an `[Int]` field, `{FavoriteNumbers: {_all: {...}}}` requires every element to match and `{_any: {...}}` at least one.

# `go.query.simple.filter.logical.not`: `_not`

- **Parents:** ["go.query.simple.filter.logical"]
- **Scenarios:** ["query.simple.with_filter.with-empty-not-filter-return-error", "query.simple.with_filter.with-not-and-comparison-xfilter-no-error", "query.simple.with_filter.with-not-equal-to-xand-not-yfilter-no-error", "query.simple.with_filter.with-not-equal-to-xfilter-no-error", "query.simple.with_filter.with-not-equal-to-xor-yfilter-no-error"]
- **Go-Bug-Suspect:** "'_not: {}' returns an empty list with no error although the test is named '..._ReturnError', so the intended result is unclear"

`_not: {cond}` returns the documents not matching `cond`, including compound `_or` conditions and a nested `_not` beside a field condition. `_not: {}` returns no documents.

# `go.query.simple.filter.alias`: Filtering on aliased fields with `_alias`

- **Parents:** ["go.query.simple.filter"]
- **Scenarios:** ["query.simple.with_filter.with-alias-equals-filter-block-should-filter", "query.simple.with_filter.with-alias-with-compound-should-match-filter", "query.simple.with_filter.with-compound-alias-should-match-filter", "query.simple.with_filter.with-empty-alias-should-not-filter", "query.simple.with_filter.with-non-aliased-field-should-match-filter", "query.simple.with_filter.with-non-existant-alias-should-return-error", "query.simple.with_filter.with-non-object-alias-should-filter-all", "query.simple.with_filter.with-null-alias-should-filter-all"]
- **Go-Bug-Suspect:** "'_alias: null' and '_alias: 1' silently return no documents instead of being ignored or rejected, while the order counterpart '_alias: 1' errors with 'invalid order input'"

`filter: {_alias: {UserAge: {_eq: 21}}}` filters on the selection aliased `UserAge` (or on an unaliased field name such as `Age`), and `_alias` works both inside `_and` and with `_and` inside it. `_alias: {}` does not filter, `_alias: null` and `_alias: 1` return an empty list, and an alias that is not selected fails with `field or alias not found. Name: UserAge`.

# `go.query.simple.filter.null-input.arguments`: `null` top-level arguments are ignored

- **Parents:** ["go.query.simple.filter.null-input"]
- **Scenarios:** ["query.simple.with-null-cid-succeeds", "query.simple.with-null-doc-id-succeeds", "query.simple.with-null-doc-ids-succeeds", "query.simple.with-null-filter-succeeds", "query.simple.with-null-group-by-succeeds", "query.simple.with-null-limit-succeed", "query.simple.with-null-offset-succeeds", "query.simple.with-null-order-fields-succeeds", "query.simple.with-null-order-succeeds", "query.simple.with-null-show-deleted-succeeds"]

`Users(filter: null)`, `order: null`, `order: {Name: null}`, `limit: null`, `offset: null`, `docID: null`, `cid: null`, `groupBy: null` and `showDeleted: null` each behave as if the argument were absent and return the documents without error.

# `go.query.simple.filter.null-input.logical`: `null` logical operators and list elements

- **Parents:** ["go.query.simple.filter.null-input"]
- **Scenarios:** ["query.simple.with-filter-with-null-and-element-returns-error", "query.simple.with-filter-with-null-and-succeeds", "query.simple.with-filter-with-null-not-succeeds", "query.simple.with-filter-with-null-or-element-returns-error", "query.simple.with-filter-with-null-or-succeeds"]

`filter: {_or: null}`, `{_and: null}` and `{_not: null}` are ignored and return the documents; a `null` element inside the list (`_or: [null]`, `_and: [null]`) fails validation with `Expected "UsersFilterArg!", found null`.

# `go.query.simple.filter.null-input.field`: `null` as a field condition

- **Parents:** ["go.query.simple.filter.null-input"]
- **Scenarios:** ["query.simple.with-filter-with-null-and-field-returns-error", "query.simple.with-filter-with-null-not-field-succeeds", "query.simple.with-filter-with-null-or-field-returns-error", "query.simple.with-null-filter-fields-succeeds"]

A field condition given as `null` (`filter: {Name: null}`, also inside `_or: [{Name: null}]` and `_and: [{Name: null}]`) is accepted and returns the document whose `Name` is null; `_not: {Name: null}` returns a document whose `Name` is set. Two tests are named `..._ReturnsError` but assert success.

# `go.query.simple.doc-id.argument`: `docID` argument, single or list

- **Parents:** ["go.query.simple.doc-id"]
- **Scenarios:** ["query.simple.returns-nothin-given-empty-doc-ids-filter", "query.simple.test-query-with-doc-ids-filter-duplicate-doc-ids", "query.simple.test-query-with-doc-ids-filter-single-target-found", "query.simple.test-query-with-doc-ids-filter-single-target-not-found", "query.simple.with-doc-idfilter-multiple-documents-target-found", "query.simple.with-doc-idfilter-single-document-target-found", "query.simple.with-doc-idfilter-target-not-found", "query.simple.with-doc-ids-filter-all-found-from-multiple-targets", "query.simple.with-doc-ids-filter-one-found-from-multiple-targets"]

`Users(docID: "bae-...")` or `docID: [...]` returns only the listed documents that exist, once each even when an ID is repeated; an unknown DocID returns an empty list without error, and `docID: []` returns an empty list.

# `go.query.simple.doc-id.filter`: `_docID` in `filter`

- **Parents:** ["go.query.simple.doc-id"]
- **Scenarios:** ["query.simple.with-doc-idfilter-block", "query.simple.with-doc-idfilter-in-block"]

`filter: {_docID: {_eq: "<id>"}}` returns that document and `filter: {_docID: {_in: [...]}}` returns the listed ones.

# `go.query.simple.order.single-field`: Ordering by one field

- **Parents:** ["go.query.simple.order"]
- **Scenarios:** ["query.simple.order-int-ascending", "query.simple.order-int-descending", "query.simple.orders-by-doc-id", "query.simple.wit-float64-order-descending", "query.simple.with-blob-order-ascending", "query.simple.with-blob-order-descending", "query.simple.with-date-time-order-ascending", "query.simple.with-date-time-order-descending", "query.simple.with-empty-order", "query.simple.with-float32-order-ascending", "query.simple.with-float32-order-descending", "query.simple.with-float64-order-ascending", "query.simple.with-numeric-greater-than-filter-and-numeric-order-descending"]

`order: {Age: ASC}` / `DESC` sorts by Int, Float32, Float64, Blob, DateTime or `_docID` in the requested direction; ordering combines with `filter`, and `order: {}` returns documents unsorted without error.

# `go.query.simple.order.multi-field`: Ordering by a list of fields

- **Parents:** ["go.query.simple.order"]
- **Scenarios:** ["query.simple.order-multiple-asc-asc", "query.simple.order-multiple-asc-desc", "query.simple.order-multiple-desc-asc", "query.simple.order-multiple-desc-desc", "query.simple.with-numeric-order-descending-and-boolean-order-ascending"]

`order: [{Name: ASC}, {Age: DESC}]` sorts by the first entry and breaks ties with the next, in every ASC/DESC combination.

# `go.query.simple.order.validation`: Invalid order arguments are rejected

- **Parents:** ["go.query.simple.order"]
- **Scenarios:** ["query.simple.with-invalid-order-enum-returns-error", "query.simple.with-multiple-order-fields-nested-within-multple-fields-returns-error", "query.simple.with-multiple-order-fields-returns-error"]

`order: {Age: INVALID}` fails with `Argument "order" has invalid value {Age: INVALID}`; an order object naming two fields (`{Age: ASC, Name: DESC}`, also as a list element) fails with `each order argument can only define one field`.

# `go.query.simple.order.alias`: Ordering by aliased fields with `_alias`

- **Parents:** ["go.query.simple.order"]
- **Scenarios:** ["query.simple.order-alias", "query.simple.order-alias-unaliased-field", "query.simple.with-alias-order-on-non-existant-field-should-error", "query.simple.with-compound-alias-order-should-order-results", "query.simple.with-empty-alias-order-should-do-nothing", "query.simple.with-int-alias-order-should-error", "query.simple.with-invalid-alias-order-should-error", "query.simple.with-null-alias-order-should-do-nothing"]

`order: {_alias: {UserAge: ASC}}` sorts by the selection aliased `UserAge` (or an unaliased field name), and a list of `_alias` entries sorts by each in turn. `_alias: {}` and `_alias: null` leave order unchanged; an unselected alias fails with `field or alias not found. Name: UserAge`, a bad direction with `invalid order direction`, and `_alias: 1` with `invalid order input`.

# `go.query.simple.order.aggregate-alias`: Ordering by an aliased per-document aggregate

- **Parents:** ["go.query.simple.order"]
- **Scenarios:** ["query.simple.with-average-with-order-succeeds", "query.simple.with-max-with-order-succeeds", "query.simple.with-min-with-order-succeeds", "query.simple.with-sum-with-order-succeeds"]

Inside a document, `total: AVG(HeightM: {}, Age: {})` (and `MAX`, `MIN`, `SUM`) aggregates over the listed scalar fields of that document, and `order: {_alias: {total: DESC}}` / `ASC` sorts documents by that computed value.

# `go.query.simple.limit-offset`: `limit` and `offset`

- **Parents:** ["go.query.simple"]
- **Scenarios:** ["query.simple.limit-one", "query.simple.limit-one-offset-one", "query.simple.limit-over-total", "query.simple.limit-two", "query.simple.limit-two-offset-two", "query.simple.limit-zero", "query.simple.limit-zero-offset-zero", "query.simple.offset-one", "query.simple.offset-over-total", "query.simple.offset-two", "query.simple.offset-zero"]

`Users(limit: n, offset: m)` skips the first `m` documents and returns at most `n`; `limit: 0` means no limit, `offset: 0` skips nothing, and a limit or offset beyond the document count returns the remaining documents (or none).

# `go.query.simple.aggregate.validation`: Aggregates without a target are rejected

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-average-on-undefined-field", "query.simple.with-average-on-undefined-object", "query.simple.with-count-on-undefined", "query.simple.with-max-on-undefined-field-returns-error", "query.simple.with-max-on-undefined-object-returns-error", "query.simple.with-min-on-undefined-field-returns-error", "query.simple.with-min-on-undefined-object-returns-error", "query.simple.with-sum-on-undefined-field", "query.simple.with-sum-on-undefined-object"]

A bare `query { AVG }` (likewise `COUNT`, `SUM`, `MIN`, `MAX`) fails with `aggregate must be provided with a property to aggregate`; `AVG(Users: {})` (likewise `SUM`, `MIN`, `MAX`) without `field` fails with `Argument "Users" has invalid value {}.\nIn field "field": Expected "UsersNumericFieldsArg!", found null.`.

# `go.query.simple.aggregate.count`: `COUNT`

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-aliased-count-on-empty-collection-succeeds", "query.simple.with-count", "query.simple.with-count-on-empty-collection", "query.simple.with-count-with-date-time-filter", "query.simple.with-count-with-filter"]

`COUNT(Users: {})` returns the number of documents (0 on an empty collection, also under an alias such as `number:`), and `COUNT(Users: {filter: {...}})` counts only documents matching the filter, DateTime comparisons included.

# `go.query.simple.aggregate.count-group-by`: `COUNT` with `groupBy` counts groups

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-count-with-group-by-all-same-group-value", "query.simple.with-count-with-group-by-and-filter", "query.simple.with-count-with-group-by-on-empty-collection", "query.simple.with-count-with-group-by-on-multiple-fields", "query.simple.with-count-with-group-by-on-single-field"]

`COUNT(Users: {groupBy: [Age]})` returns the number of distinct groups (distinct value tuples for `[Age, Name]`), 0 on an empty collection, 1 when all documents share a value, and applies `filter` before grouping.

# `go.query.simple.aggregate.sum`: `SUM`

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-sum", "query.simple.with-sum-on-empty-collection", "query.simple.with-sum-with-filter"]

`SUM(Users: {field: Age})` returns the total of the field over all documents (0 on an empty collection), restricted to filter matches when `filter` is given.

# `go.query.simple.aggregate.average`: `AVG`

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-aliased-average-on-empty-collection-succeeds", "query.simple.with-average", "query.simple.with-average-on-empty-collection", "query.simple.with-average-with-date-time-filter", "query.simple.with-average-with-filter"]

`AVG(Users: {field: Age})` returns the mean as a Float (`0` on an empty collection, also under an alias), restricted to filter matches when `filter` is given, DateTime filters included.

# `go.query.simple.aggregate.min-max`: `MIN` and `MAX`

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-aliased-max-on-empty-collection-succeeds", "query.simple.with-aliased-min-on-empty-collection-succeeds", "query.simple.with-max-filter-succeeds", "query.simple.with-max-on-empty-collection-succeeds", "query.simple.with-max-succeeds", "query.simple.with-min-filter-succeeds", "query.simple.with-min-on-empty-collection-succeeds", "query.simple.with-min-succeeds"]

`MAX(Users: {field: Age})` / `MIN(...)` return the largest / smallest value of the field (Int stays Int), restricted to filter matches when `filter` is given; on an empty collection they return `null`, also under an alias.

# `go.query.simple.aggregate.int64-extreme`: `MAX` of a 64-bit Int

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-max-and-max-value-int-succeeds", "query.simple.with-min-and-max-value-int-succeeds"]
- **Scope:** "client types: go, cli, http (not JS); mutation types: collection save and collection named (not GraphQL), because the document is created with Age = math.MaxInt64"
- **Go-Bug-Suspect:** "'TestQuerySimple_WithMinAndMaxValueInt_Succeeds' queries 'MAX', not 'MIN', so 'MIN' at MaxInt64 is untested"

With a document whose `Age` is `9223372036854775807`, `MAX(Users: {field: Age})` returns exactly `9223372036854775807` with no float precision loss.

# `go.query.simple.aggregate.non-nillable`: Aggregates over non-nullable fields

- **Parents:** ["go.query.simple.aggregate"]
- **Scenarios:** ["query.simple.with-average-on-non-nillable-float32", "query.simple.with-average-on-non-nillable-float64", "query.simple.with-average-on-non-nillable-int", "query.simple.with-count-on-non-nillable-float32", "query.simple.with-count-on-non-nillable-float64", "query.simple.with-count-on-non-nillable-int", "query.simple.with-max-on-non-nillable-float32", "query.simple.with-max-on-non-nillable-float64", "query.simple.with-max-on-non-nillable-int", "query.simple.with-min-on-non-nillable-float32", "query.simple.with-min-on-non-nillable-float64", "query.simple.with-min-on-non-nillable-int", "query.simple.with-sum-on-non-nillable-float32", "query.simple.with-sum-on-non-nillable-float64", "query.simple.with-sum-on-non-nillable-int"]
- **Go-Bug-Suspect:** "on 'score: Int!' the tests expect 'MAX'/'MIN' as float64 (30, 10) and 'SUM' as int64, while 'MAX' on nullable 'Age: Int' expects int64; possibly a type quirk or only harness numeric normalisation"

`SUM`, `AVG`, `MIN`, `MAX` and `COUNT` work on `Int!`, `Float64!` and `Float32!` fields (e.g. scores 10, 20, 30 give SUM 60, AVG 20, MIN 10, MAX 30, COUNT 3).

# `go.query.simple.group.basic`: Grouping by one or more fields

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.with-group-by-boolean-then-number", "query.simple.with-group-by-date-time", "query.simple.with-group-by-empty", "query.simple.with-group-by-number", "query.simple.with-group-by-number-with-group-string", "query.simple.with-group-by-string", "query.simple.with-group-by-string-then-boolean", "query.simple.with-group-by-string-with-inner-group-boolean", "query.simple.with-group-by-without-grouped-field-selected-with-inner-group"]

`Users(groupBy: [Age]) { Age GROUP { Name } }` returns one row per distinct value (Int, String, Boolean, DateTime) with the grouped documents under `GROUP`; several fields group by value tuple, `GROUP(groupBy: [...])` nests a further grouping, and `groupBy: []` puts every document in one group.

# `go.query.simple.group.undefined-value`: Documents with the grouped field unset

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.with-group-by-number-on-undefined", "query.simple.with-group-by-number-on-undefined-with-children"]

Documents whose grouped field is unset form one group with key `null` (e.g. `{"Age": null}`), alongside the groups of set values.

# `go.query.simple.group.validation`: Invalid grouped selections are rejected

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.errors-with-non-group-fields-selected", "query.simple.with-group-by-number-with-count-on-inner-non-existant-group", "query.simple.with-group-by-number-with-undefined-field", "query.simple.with-group-by-string-without-rendered-group-and-average-of-undefined", "query.simple.with-group-by-string-without-rendered-group-and-max-of-undefined-returns-error", "query.simple.with-group-by-string-without-rendered-group-and-min-of-undefined-returns-error", "query.simple.with-group-by-string-without-rendered-group-and-sum-of-undefined", "query.simple.without-group-by-with-count-on-group"]

Selecting a non-grouped field at group level fails with `cannot select a non-group-by field at group-level`; `COUNT(GROUP: {})` outside a `groupBy` level (also inside `GROUP` with no inner groupBy) fails with `group may only be referenced when within a groupBy request`; a bare `COUNT`, `AVG`, `SUM`, `MIN` or `MAX` at group level fails with `aggregate must be provided with a property to aggregate`.

# `go.query.simple.group.filter`: Filtering groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.with-group-by-string-with-group-number-filter", "query.simple.with-group-by-string-with-group-number-with-parent-filter", "query.simple.with-group-by-string-with-inner-group-boolean-then-inner-number-filter-that-excludes-all", "query.simple.with-group-by-string-with-multiple-group-number-filter", "query.simple.with-group-by-string-with-unrendered-group-number-with-parent-filter"]

A root `filter` applies before grouping, so groups with no matching documents disappear; `GROUP(filter: {...})` filters only the documents inside each group (groups stay, possibly with an empty `GROUP`), independently for each aliased `GROUP` and at nested levels.

# `go.query.simple.group.limit-offset`: Paging groups and their documents

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.with-group-by-number-with-group-limit", "query.simple.with-group-by-number-with-group-limit-and-offset", "query.simple.with-group-by-number-with-group-limit-and-root-order", "query.simple.with-group-by-number-with-limit-and-group-with-higher-limit", "query.simple.with-group-by-number-with-limit-and-group-with-lower-limit", "query.simple.with-group-by-number-with-limit-and-offset-and-with-group-limit-and-offset", "query.simple.with-group-by-number-with-multiple-groups-with-different-limits"]

Root `limit`/`offset` page the group rows; `GROUP(limit: n, offset: m)` pages documents within each group, independently per aliased `GROUP` and combined with root `order`.

# `go.query.simple.group.doc-id`: `docID` on `GROUP`

- **Parents:** ["go.query.simple.group"]
- **Scenarios:** ["query.simple.with-group-by-with-group-with-doc-id", "query.simple.with-group-by-with-group-with-doc-ids"]

`GROUP(docID: "<id>")` or `GROUP(docID: [...])` keeps only those documents inside each group; every group row is still returned, with an empty `GROUP` where none match.

# `go.query.simple.group.aggregate.count`: `COUNT(GROUP: {})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-number-with-rendered-group-and-child-count", "query.simple.with-group-by-number-without-rendered-group-and-aliases-child-count", "query.simple.with-group-by-number-without-rendered-group-and-child-count", "query.simple.with-group-by-number-without-rendered-group-and-child-count-on-empty-collection", "query.simple.with-group-by-number-without-rendered-group-and-duplicated-aliased-child-counts"]

`COUNT(GROUP: {})` returns each group's document count, with or without `GROUP` also selected and under one or several aliases; an empty collection returns no rows.

# `go.query.simple.group.aggregate.sum`: `SUM(GROUP: {field})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-without-rendered-group-and-child-empty-float-sum", "query.simple.with-group-by-string-without-rendered-group-and-child-float-sum", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-sum", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-sum-on-empty-collection", "query.simple.with-group-by-string-without-rendered-group-and-child-nil-sum"]

`SUM(GROUP: {field: Age})` returns each group's total over Int (negative values included) or Float fields, ignoring documents where the field is unset; an empty collection returns no rows.

# `go.query.simple.group.aggregate.average`: `AVG(GROUP: {field})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-without-rendered-group-and-child-empty-float-average", "query.simple.with-group-by-string-without-rendered-group-and-child-float-average", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-on-empty-collection", "query.simple.with-group-by-string-without-rendered-group-and-child-nil-average"]

`AVG(GROUP: {field: Age})` returns each group's mean over Int (negative values included) or Float fields, ignoring documents where the field is unset; a group with no set values averages to 0, and an empty collection returns no rows.

# `go.query.simple.group.aggregate.min-max`: `MIN` and `MAX` over `GROUP`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-without-rendered-group-and-child-empty-float-max-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-empty-float-min-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-float-max-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-float-min-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-max-on-empty-collection-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-max-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-min-on-empty-collection-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-min-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-nil-max-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-nil-min-succeeds"]

`MAX(GROUP: {field: Age})` / `MIN(...)` return each group's largest / smallest value over Int (negative values included) or Float fields, ignoring unset values; a group with no set values returns `null`, and an empty collection returns no rows.

# `go.query.simple.group.aggregate.multiple`: Several different aggregates on one group

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-and-count", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-and-max-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-and-min-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-and-sum"]

`AVG(GROUP: {field: Age})` selected together with `COUNT(GROUP: {})`, `MAX`, `MIN` or `SUM` on the same group returns each value correctly, negative values included.

# `go.query.simple.group.aggregate.of-same`: Aggregate of the same aggregate over inner groups

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-with-inner-group-boolean-and-average-of-average-of-average-of-float", "query.simple.with-group-by-string-with-inner-group-boolean-and-average-of-average-of-float", "query.simple.with-group-by-string-with-inner-group-boolean-and-average-of-average-of-int", "query.simple.with-group-by-string-with-inner-group-boolean-and-max-of-max-of-float-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-max-of-max-of-int-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-max-of-max-of-max-of-float-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-min-of-min-of-float-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-min-of-min-of-int-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-min-of-min-of-min-of-float-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-sum-of-sum-of-float", "query.simple.with-group-by-string-with-inner-group-boolean-and-sum-of-sum-of-int", "query.simple.with-group-by-string-with-inner-group-boolean-and-sum-of-sum-of-sum-of-float"]

With `GROUP(groupBy: [Verified]) { AVG(GROUP: {field: Age}) }`, an outer `AVG(GROUP: {field: AVG})` aggregates the inner groups' values; the same holds for `SUM` of `SUM`, `MIN` of `MIN` and `MAX` of `MAX`, over Int and Float and across three nesting levels.

# `go.query.simple.group.aggregate.of-other`: Aggregate of a different aggregate over inner groups

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.test-query-simple-with-group-by-string-with-inner-group-boolean-and-max-of-average-of-int-succeeds", "query.simple.test-query-simple-with-group-by-string-with-inner-group-boolean-and-min-of-average-of-int-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-max-of-count-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-min-of-count-succeeds", "query.simple.with-group-by-string-with-inner-group-boolean-and-sum-of-count", "query.simple.with-group-by-string-with-inner-group-boolean-and-sum-of-count-of-int"]

`MAX`, `MIN` and `SUM(GROUP: {field: AVG})` aggregate the inner groups' `AVG` values, and `MAX`, `MIN` and `SUM(GROUP: {field: COUNT})` aggregate the inner groups' `COUNT` values.

# `go.query.simple.group.aggregate.filtered-average`: `AVG(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-string-with-rendered-group-and-child-average-with-date-time-filter", "query.simple.with-group-by-string-with-rendered-group-and-child-average-with-filter", "query.simple.with-group-by-string-with-rendered-group-with-filter-and-child-average-with-different-filter", "query.simple.with-group-by-string-with-rendered-group-with-filter-and-child-average-with-matching-date-time-filter", "query.simple.with-group-by-string-with-rendered-group-with-filter-and-child-average-with-matching-filter", "query.simple.with-group-by-string-without-rendered-group-and-child-average-with-filter", "query.simple.with-group-by-string-without-rendered-group-and-child-average-with-filter-and-nil-item", "query.simple.with-group-by-string-without-rendered-group-and-child-averages-with-different-filters"]

`AVG(GROUP: {field: Age, filter: {...}})` averages only the group's documents matching the filter (Int and DateTime conditions), unaffected by a different `GROUP(filter: ...)` selected alongside or by other aliased averages with different filters, and ignoring documents where the field is unset.

# `go.query.simple.group.aggregate.filtered-count`: `COUNT(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-number-with-rendered-group-and-child-count-with-filter", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-count-with-different-filter", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-count-with-matching-filter", "query.simple.with-group-by-number-without-rendered-group-and-child-count-with-filter", "query.simple.with-group-by-number-without-rendered-group-and-child-counts-with-different-filters"]

`COUNT(GROUP: {filter: {...}})` counts only the group's documents matching the filter, independently of any `GROUP(filter: ...)` selected alongside and of other aliased counts with different filters.

# `go.query.simple.group.aggregate.filtered-sum`: `SUM(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-number-with-rendered-group-and-child-sum-with-filter", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-sum-with-different-filter", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-sum-with-matching-filter", "query.simple.with-group-by-number-without-rendered-group-and-child-sum-with-filter", "query.simple.with-group-by-number-without-rendered-group-and-child-sums-with-different-filters"]

`SUM(GROUP: {field: Age, filter: {...}})` sums only the group's documents matching the filter (0 when none match), independently of any `GROUP(filter: ...)` and of other aliased sums with different filters.

# `go.query.simple.group.aggregate.filtered-min-max`: `MIN`/`MAX(GROUP: {filter})`

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-number-with-rendered-group-and-child-max-with-filter-succeeds", "query.simple.with-group-by-number-with-rendered-group-and-child-min-with-filter-succeeds", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-max-with-different-filter-succeeds", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-max-with-matching-filter-succeeds", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-min-with-different-filter-succeeds", "query.simple.with-group-by-number-with-rendered-group-with-filter-and-child-min-with-matching-filter-succeeds", "query.simple.with-group-by-number-without-rendered-group-and-child-max-with-different-filters-succeeds", "query.simple.with-group-by-number-without-rendered-group-and-child-max-with-filter-succeeds", "query.simple.with-group-by-number-without-rendered-group-and-child-min-with-different-filters-succeeds", "query.simple.with-group-by-number-without-rendered-group-and-child-min-with-filter-succeeds"]

`MAX`/`MIN(GROUP: {field: Age, filter: {...}})` consider only the group's documents matching the filter (`null` when none match), independently of any `GROUP(filter: ...)` and of other aliased aggregates with different filters.

# `go.query.simple.group.aggregate.limit-offset`: `limit`/`offset` inside a group aggregate

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-by-number-with-rendered-group-with-limit-and-child-count-with-limit", "query.simple.with-group-by-number-with-rendered-group-with-limit-and-child-count-with-limit-and-offset", "query.simple.with-group-by-number-without-rendered-group-and-child-count-with-limit", "query.simple.with-group-by-number-without-rendered-group-and-child-count-with-limit-and-offset", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-with-limit", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-average-with-limit-and-offset", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-max-with-limit-and-offset-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-max-with-limit-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-min-with-limit-and-offset-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-min-with-limit-succeeds", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-sum-with-limit", "query.simple.with-group-by-string-without-rendered-group-and-child-integer-sum-with-limit-and-offset"]

`COUNT`, `SUM`, `AVG`, `MIN` and `MAX(GROUP: {..., offset: m, limit: n})` aggregate only the window of the group's documents after skipping `m` and taking `n`, independently of a `GROUP(limit: ...)` selected alongside.

# `go.query.simple.group.aggregate.alias-filter`: Filtering group rows by an aliased aggregate

- **Parents:** ["go.query.simple.group.aggregate"]
- **Scenarios:** ["query.simple.with-group-average-alias-filter-filters-results", "query.simple.with-group-count-alias-filter-filters-results", "query.simple.with-group-max-alias-filter-filters-results", "query.simple.with-group-min-alias-filter-filters-results", "query.simple.with-group-sum-alias-filter-filters-results"]

`Users(groupBy: [Name], filter: {_alias: {totalScore: {_eq: 40}}}) { Name totalScore: SUM(GROUP: {field: Score}) }` returns only the groups whose aliased aggregate matches; likewise for `AVG`, `MIN`, `MAX` and `COUNT`.

# `go.query.simple.cid.doc-version`: `cid` returns the document state at that commit

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.with-cid", "query.simple.with-cid-and-doc-id", "query.simple.with-cid-multiple-docs", "query.simple.with-shared-field-cid-returns-all-owners", "query.simple.with-update-and-first-cid-and-doc-id", "query.simple.with-update-and-first-cid-and-doc-idand-schema-version", "query.simple.with-update-and-last-cid-and-doc-id", "query.simple.with-update-and-middle-cid-and-doc-id"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers (CIDs are templated/hardcoded)"

`Users(cid: "<composite CID>")` (with or without a matching `docID`) returns only that document, with its field values and `_version` as of that commit, even after later updates and when other documents exist. A field-level CID shared by several documents with identical field content returns every owning document.

# `go.query.simple.cid.counter`: `cid` on documents with counter fields

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.test-cid-and-doc-idquery-contains-pcounter-with-float-kind-no-error", "query.simple.test-cid-and-doc-idquery-contains-pcounter-with-int-kind-no-error", "query.simple.test-cid-and-doc-idquery-contains-pncounter-with-float-kind-no-error", "query.simple.test-cid-and-doc-idquery-contains-pncounter-with-int-kind-no-error", "query.simple.with-cidand-counter-after-update-should-succeed"]
- **Scope:** "excluded from the secondary-index multiplier (counter fields cannot be indexed, Go #4439) and the encrypted-docs and signed-docs multipliers"

For a `@crdt(type: pncounter)` or `pcounter` field of Int or Float kind, querying with a CID and `docID` returns the counter value accumulated as of that commit (e.g. the first CID returns the initial value, a later CID the incremented value), without error.

# `go.query.simple.cid.list`: `cid` given as a list

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.duplicate-cids-for-same-doc", "query.simple.list-of-one-cid", "query.simple.multiple-cids", "query.simple.multiple-cids-for-same-doc"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers"

`Users(cid: ["<a>", "<b>"])` returns one result per distinct CID: each document at its version, two versions of the same document as two results, and a repeated CID only once.

# `go.query.simple.cid.doc-id-match`: `cid` combined with a `docID` list

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.with-version-and-cid", "query.simple.with-version-and-cid-and-correct-and-incorrect-doc-id", "query.simple.with-version-and-cid-and-correct-doc-id", "query.simple.with-version-and-cid-and-incorrect-doc-id"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers"

`Users(cid: "<c>", docID: [...])` returns the document at that commit with its `_version` when the CID's document is among the listed DocIDs (other listed IDs are ignored), and an empty list when it is not.

# `go.query.simple.cid.branchable`: Collection-level CIDs of a `@branchable` collection

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.with-cid-of-branchable-collection-and-doc-id", "query.simple.with-cid-of-branchable-collection-first-cid", "query.simple.with-cid-of-branchable-collection-last-cid", "query.simple.with-cid-of-branchable-collection-middle-cid"]
- **Scope:** "excluded from the encrypted-docs and signed-docs multipliers (hardcoded collection CIDs, Go #4744)"

On a `@branchable` collection, `Users(cid: "<collection commit CID>")` returns every document as of that collection commit (after create `Fred` → `Fred`; after update → `Freddddd`; after a second create → `John` and `Freddddd`); adding `docID` restricts the result to that document at that commit.

# `go.query.simple.cid.errors`: Invalid, unknown and foreign CIDs

- **Parents:** ["go.query.simple.cid"]
- **Scenarios:** ["query.simple.unknown-cid", "query.simple.with-cid-from-another-collection-returns-empty", "query.simple.with-invalid-cid", "query.simple.with-invalid-cid-and-invalid-doc-id", "query.simple.with-unknown-cid-and-invalid-doc-id"]
- **Go-Bug-Suspect:** "an unknown but well-formed CID errors with 'failed to get block in blockstore: ipld: could not find'; the test comment says this is documented only and the desired behaviour is an empty result"

A string that is not a CID fails with `invalid cid: selected encoding not supported`; a well-formed CID with no block fails with `failed to get block in blockstore: ipld: could not find` (with or without a `docID`); a CID belonging to another collection's document returns an empty list without panicking.

# `go.query.simple.version.latest-commit`: `_version` returns the latest composite commit

- **Parents:** ["go.query.simple.version"]
- **Scenarios:** ["query.simple.test-query-add-doc-with-nested-latest-commit", "query.simple.test-query-update-doc-with-nested-latest-commit", "query.simple.test-query-with-all-commit-fields-no-error", "query.simple.test-query-with-all-commit-fields-with-update-no-error", "query.simple.with-embedded-latest-commit-with-collection-version-id", "query.simple.with-embedded-latest-commit-with-doc-id", "query.simple.with-nested-latest-commit", "query.simple.with-version-and-order"]
- **Scope:** "the tests asserting CIDs and link order are excluded from the encrypted-docs and signed-docs multipliers"

`Users { _version { cid delta docID fieldName height links { cid fieldName } heads { cid } collectionVersionId } }` returns the document's composite commits, newest first, each with its CID, delta, DocID, `fieldName: "_C"`, height and collection version ID. A new document has one entry (height 1) that links every field commit, sorted by child CID, with empty `heads`. After one update there are two: the update commit (height 2) links only the changed field's commit and lists the create commit under `heads`, followed by the create commit. It combines with `order`.

# `go.query.simple.version.aliased`: Aliased and filtered `links`/`heads`

- **Parents:** ["go.query.simple.version"]
- **Scenarios:** ["query.simple.test-query-with-multiple-aliased-filtered-embedded-latest-commit", "query.simple.with-multiple-aliased-embedded-latest-commit", "query.simple.with-multiple-aliased-interleaved-nested-latest-commit"]
- **Scope:** "the tests asserting link order are excluded from the encrypted-docs and signed-docs multipliers"

Within `_version`, `links` and `heads` can be selected several times under different aliases with different sub-fields, interleaved, and `links(filter: {fieldName: {_eq: "Age"}})` returns only the matching links.

# `go.query.simple.similarity.full-scan`: `SIMILARITY` values without an index

- **Parents:** ["go.query.simple.similarity"]
- **Scenarios:** ["query.simple.query-simple-with-float32-similarity-should-succeed", "query.simple.query-simple-with-float64-similarity-should-succeed", "query.simple.query-simple-with-int-similarity-should-succeed", "query.simple.query-simple-with-json-doc-creation-similarity-should-succeed", "query.simple.with-similarity-on-empty-collection-should-succeed"]

`User { name SIMILARITY(pointsList: {vector: [1, 2, 0]}) }` returns, per document, the cosine similarity between the `[Int!]`, `[Float32!]` or `[Float64!]` field and the vector, also for documents created from JSON; an empty collection returns an empty list.

# `go.query.simple.similarity.alias`: Filtering and ordering by an aliased `SIMILARITY`

- **Parents:** ["go.query.simple.similarity"]
- **Scenarios:** ["query.simple.query-simple-with-similarity-and-filtering-on-similarity-result-should-succeed", "query.simple.query-simple-with-similarity-and-ordering-with-limit-on-similarity-result-should-succeed", "query.simple.query-simple-with-two-similarity-and-filtering-on-second-should-succeed", "query.simple.with-two-similarity-and-filtering-on-both-should-succeed"]
- **Go-Bug-Suspect:** "'filter: {_or: [{_alias: {sim2: {_gt: 20}}}, {_alias: {sim: {_lt: 10}}}]}' is asserted to return no documents although every cosine similarity is below 10; Go #3468 documents this as a bug"

`filter: {_alias: {sim: {_lt: 0.9}}}` and `order: {_alias: {sim: DESC}}, limit: 2` filter and sort by an aliased `SIMILARITY`; with two similarity selections, filtering on the second works independently of the first.

# `go.query.simple.similarity.vector-index`: k-nearest search through a vector index

- **Parents:** ["go.query.simple.similarity"]
- **Scenarios:** ["query.simple.query-simple-with-similarity-on-vector-index-excludes-deleted-doc", "query.simple.query-simple-with-similarity-on-vector-index-is-magnitude-invariant", "query.simple.query-simple-with-similarity-on-vector-index-reflects-updated-vector", "query.simple.query-simple-with-similarity-on-vector-index-respects-offset", "query.simple.query-simple-with-similarity-on-vector-index-returns-k-nearest"]

On a field with `@index(vector: {dimensions: 3, hnsw: {metric: COSINE}})`, `User(order: {_alias: {sim: DESC}}, limit: k, offset: m)` with `sim: SIMILARITY(vector: {...})` returns the k nearest documents after skipping m, reading only those through the index (`@explain(type: execute)` shows 1 index fetch and k doc fetches). Updated vectors are re-ranked, deleted documents are excluded, and similarity ignores magnitude (`[10,0,0]` and `[1,0,0]` both score 1).
