# `go.mutation.add.basic`: Adding documents returns them and makes them queryable

- **Parents:** ["go.mutation.add"]
- **Scenarios:** ["mutation.add.basic", "mutation.add.mutation-add-given-empty-input", "mutation.add.mutation-add-many", "mutation.add.returns-version-cid", "mutation.add.with10-collections"]

A created document (single or a JSON array of several) is returned by a later `Users { _docID name age }` query with its generated `_docID`. `add_Users(input: {})` creates a document and returns a valid `_docID`; in a schema with ten collections `add_Foo1` returns the given fields; selecting `_version { cid }` on `add_Users` returns one valid CID.

# `go.mutation.add.null_input`: Null and omitted add arguments

- **Parents:** ["go.mutation.add"]
- **Scenarios:** ["mutation.add.with-null-encrypt-fields-succeeds", "mutation.add.with-null-encrypt-succeeds", "mutation.add.with-null-input-entry-returns-error", "mutation.add.with-null-input-succeeds", "mutation.add.with-omitted-value-and-explicit-null-value"]

`add_Users(encrypt: null, ...)` and `add_Users(encryptFields: null, ...)` behave as if the argument were absent; `add_Users(input: null)` returns an empty list; `add_Users(input: [null])` fails with `Expected "UsersMutationInputArg!", found null.`. Adding `{"name": "John"}` and then `{"name": "John", "age": null}` creates two distinct documents, both with `age` null.

# `go.mutation.add.default_values`: `@default` field values on create

- **Parents:** ["go.mutation.add"]
- **Scenarios:** ["mutation.add.with-default-jsonbool-value-should-be-set", "mutation.add.with-default-jsondeep-object-value-should-be-set", "mutation.add.with-default-jsonfloat-value-should-be-set", "mutation.add.with-default-jsonint-value-should-be-set", "mutation.add.with-default-jsonnull-value-return-error", "mutation.add.with-default-jsonobject-values-should-be-set", "mutation.add.with-default-value-no-value-provided-added-twice-unique-index-returns-error", "mutation.add.with-default-values-nil-values-provided-sets-nil-value", "mutation.add.with-default-values-no-values-provided-sets-default-value", "mutation.add.with-default-values-values-provided-sets-value"]

Omitted fields take their `@default(value: ...)` for Boolean, DateTime, String, Int, Float, Float32, Float64, JSON and Blob; explicit nulls store null and provided values win. JSON defaults may be int, float, bool, an object or a nested object (returned as canonical sorted-key JSON text); `@default(value: null)` on a JSON field fails schema creation with `default value is invalid`. A defaulted value still counts for a unique index: a second doc defaulting to the same `name` fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.duplicate`: Identical content yields the same docID and is rejected

- **Parents:** ["go.mutation"]
- **Scenarios:** ["mutation.add.field_kinds.test-mutation-add-with-duplicate-jsonfield-returns-error", "mutation.add.given-duplicate-errors", "mutation.add.with-default-value-no-value-provided-added-twice-returns-error"]

Creating a document whose content matches an existing one (including two all-default documents, or JSON objects whose keys differ only in order) fails with `a document with the given ID already exists`, because the docID is derived deterministically from content.

# `go.mutation.unknown_field`: Writing a field the collection does not have

- **Parents:** ["go.mutation"]
- **Scenarios:** ["mutation.add.field_kinds.one_to_many.aliased-relation-name-non-existing-relation-single-side-no-idfield-error", "mutation.add.field_kinds.one_to_many.aliased-relation-name-with-invalid-field-error", "mutation.add.field_kinds.one_to_many.non-existing-relation-single-side-no-idfield-error", "mutation.add.field_kinds.one_to_many.with-invalid-field-error", "mutation.add.field_kinds.one_to_one.use-alias-with-invalid-field-error", "mutation.add.field_kinds.one_to_one.with-invalid-field-error", "mutation.add.given-non-existant-field-errors", "mutation.update.field_kinds.one_to_many.alias-relation-name-to-link-from-many-side-with-wrong-field-error", "mutation.update.field_kinds.one_to_many.relation-idto-link-from-many-side-with-wrong-field-error"]

Creating or updating a document with an unknown field fails with `the given field does not exist. Name: <field>` (e.g. `fieldDoesNotExist`, `notName`) and writes nothing. Setting a relation from the one side of a one-to-many (`published` or `_publishedID` on `Author`) fails the same way, naming that field.

# `go.mutation.null_args`: Null filter or docID on update and delete

- **Parents:** ["go.mutation"]
- **Scenarios:** ["mutation.delete.with-null-doc-id-succeeds", "mutation.delete.with-null-doc-ids-succeeds", "mutation.delete.with-null-filter-succeeds", "mutation.update.with-null-doc-id-succeeds", "mutation.update.with-null-doc-ids-succeeds", "mutation.update.with-null-filter-succeeds"]
- **Go-Bug-Suspect:** "'docID: null' is treated as no restriction, so the update or delete hits every document in the collection; truncate rejects the same null ('truncate docID cannot be null'). The NullDocIDs tests are copies of the NullDocID tests (both pass 'docID: null')."

`update_Users(filter: null, ...)` / `update_Users(docID: null, ...)` update every document, and `delete_Users(filter: null)` / `delete_Users(docID: null)` delete every document, returning them.

# `go.mutation.constraints`: `@constraints(size:)` on array fields

- **Parents:** ["go.mutation"]
- **Scenarios:** ["mutation.add.constraints.with-size-constrain-mismatch-should-error", "mutation.add.constraints.with-size-constrain-should-succeed", "mutation.update.constraints.test-mutation-update-with-size-constrain-mismatch-should-error", "mutation.update.constraints.test-mutation-update-with-size-constrain-should-succeed"]

For `numbers: [Int!] @constraints(size: 2)`, creating or updating with a two-element array succeeds and is returned; any other length fails with `array size mismatch`.

# `go.mutation.utc_now`: The `UTC_NOW` DateTime literal

- **Parents:** ["go.mutation"]
- **Scenarios:** ["mutation.add.field_kinds.test-mutation-add-with-date-time-sets-two-equal-utcnow-values", "mutation.add.field_kinds.with-date-time-with-utcnow", "mutation.add.mutation-add-with-default-values-no-values-provided-sets-utcnow-default-value", "mutation.add.with-default-values-no-values-provided-sets-two-equal-utcnow-default-value", "mutation.update.field_kinds.with-date-time-field-with-utcnow", "mutation.update.field_kinds.with-date-time-field-with-utcnow-should-be-equal", "mutation.upsert.mutation-upsert-with-date-time-field-with-utcnow-should-be-equal"]

`UTC_NOW` as an add, update or upsert input value, or as `@default(value: UTC_NOW)`, stores the current UTC timestamp. Every `UTC_NOW` within one request resolves to the same timestamp, so two aliased mutations in one request return equal `created` values.

# `go.mutation.field_kinds.other_scalars`: Boolean, String, Blob and DateTime scalars round-trip

- **Parents:** ["go.mutation.field_kinds"]
- **Scenarios:** ["mutation.add.field_kinds.with-blob", "mutation.add.field_kinds.with-bool", "mutation.add.field_kinds.with-date-time", "mutation.add.field_kinds.with-date-times-nano-seconds-appart", "mutation.add.field_kinds.with-non-nillable-blob", "mutation.add.field_kinds.with-non-nillable-bool", "mutation.add.field_kinds.with-non-nillable-date-time", "mutation.add.field_kinds.with-non-nillable-string", "mutation.add.field_kinds.with-string"]

Boolean, String, Blob (hex string `"00FF"` returned unchanged) and DateTime (RFC 3339 string or time value) fields, nillable or `!`, return the stored value. DateTime keeps nanosecond precision: values 1 ns apart are stored as three distinct documents with distinct times.

# `go.mutation.field_kinds.null_scalars`: Nillable scalars accept null

- **Parents:** ["go.mutation.field_kinds"]
- **Scenarios:** ["mutation.add.field_kinds.with-nillable-blob-nil", "mutation.add.field_kinds.with-nillable-bool-nil", "mutation.add.field_kinds.with-nillable-date-time-nil", "mutation.add.field_kinds.with-nillable-float32-nil", "mutation.add.field_kinds.with-nillable-float64-nil", "mutation.add.field_kinds.with-nillable-int-nil", "mutation.add.field_kinds.with-nillable-string-nil"]

Creating a document with null for a nillable Int, Float32, Float64, Boolean, String, Blob or DateTime field returns null for that field.

# `go.mutation.field_kinds.json`: JSON fields hold any JSON value

- **Parents:** ["go.mutation.field_kinds"]
- **Scenarios:** ["mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-boolean-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-int-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-list-of-objects-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-list-of-scalars-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-null-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-object-value-succeeds", "mutation.add.field_kinds.test-mutation-add-with-jsonfield-given-string-value-succeeds"]

A `JSON` field stores and returns an object, a list of scalars, a list of objects, a number (returned as float64, e.g. 250), a string, a boolean or null.

# `go.mutation.relation.one_to_one_unique`: A one-to-one primary can link only one document

- **Parents:** ["go.mutation.relation"]
- **Scenarios:** ["mutation.add.field_kinds.one_to_one.errors-given-relation-already-established-via-primary", "mutation.update.field_kinds.one_to_one.alias-relation-name-to-link-from-primary-side", "mutation.update.field_kinds.one_to_one.relation-idto-link-from-primary-side"]

Creating or updating a second `Author` whose `_publishedID`/`published` points at a book another author already links fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.relation.dangling`: Links to non-existent documents are accepted

- **Parents:** ["go.mutation.relation"]
- **Scenarios:** ["mutation.add.field_kinds.one_to_many.aliased-relation-name-non-existing-relation-many-side-added-doc", "mutation.add.field_kinds.one_to_many.non-existing-relation-many-side-added-doc", "mutation.add.field_kinds.one_to_one.no-child", "mutation.add.field_kinds.one_to_one.use-alias-with-non-existing-relation-primary-side-added-doc", "mutation.update.field_kinds.one_to_one.no-child"]
- **Go-Bug-Suspect:** "each test carries the comment 'This test should probably not pass, as it contains a reference to a document that doesnt exist'"

Creating or updating a document whose relation ID (`_publishedID`, `published`, `_authorID`, `author`) is a well-formed docID that does not exist succeeds, and the document is returned by a query.

# `go.mutation.update.by_id`: Update by docID

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.with-id", "mutation.update.with-id-and-version-return-results", "mutation.update.with-ids", "mutation.update.with-non-existant-id"]

`update_Users(docID: "<id>", input: {points: 59})` updates and returns only that document; a list of docIDs updates exactly those documents; an unknown docID (`bae-does-not-exist`) returns an empty list without error. Selecting `_version { cid }` returns two entries, the update commit then the create commit.

# `go.mutation.update.collection_filter`: Collection API update-with-filter

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.update-with-filter-succeeds", "mutation.update.update-with-map-filter-large-integer-straddling2-to53-updates-only-exact-match", "mutation.update.update-with-map-filter-succeeds", "mutation.update.update-with-none-option-filter-updates-all-documents", "mutation.update.update-with-some-option-filter-succeeds"]

The collection's update-with-filter accepts a filter as GraphQL text, a map, or an optional filter, plus a JSON updater (`{"name": "Eric"}`), and updates matching documents; no filter updates every document. An int filter `_eq: 9007199254740993` updates only that document, not its neighbour 9007199254740992.

# `go.mutation.update.set_null`: Setting a scalar to null

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.field_kinds.if-blob-field-set-to-null-should-be-nil", "mutation.update.field_kinds.if-bool-field-set-to-null-should-be-nil", "mutation.update.field_kinds.if-date-time-field-set-to-null-should-be-nil", "mutation.update.field_kinds.if-float-field-set-to-null-should-be-nil", "mutation.update.field_kinds.if-int-field-set-to-null-should-be-nil", "mutation.update.field_kinds.if-jsonfield-set-to-null-should-be-nil", "mutation.update.field_kinds.if-string-field-set-to-null-should-be-nil"]

Updating a nillable Blob, Boolean, DateTime, Float, Int, JSON or String field to `null` makes a later query return null for it.

# `go.mutation.update.array_clear`: Updating an array to null or empty

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.field_kinds.with-array-of-booleans-to-empty", "mutation.update.field_kinds.with-array-of-booleans-to-nil", "mutation.update.field_kinds.with-array-of-floats-to-empty", "mutation.update.field_kinds.with-array-of-floats-to-nil", "mutation.update.field_kinds.with-array-of-ints-to-empty", "mutation.update.field_kinds.with-array-of-ints-to-nil", "mutation.update.field_kinds.with-array-of-strings-to-empty", "mutation.update.field_kinds.with-array-of-strings-to-nil"]

Updating a `[Boolean!]`, `[Float!]`, `[Int!]` or `[String!]` field to `null` returns null, and to `[]` returns an empty list (distinct from null).

# `go.mutation.update.array_replace`: Updating an array replaces it whole

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.field_kinds.with-array-of-booleans-to-larger-size", "mutation.update.field_kinds.with-array-of-booleans-to-same-size", "mutation.update.field_kinds.with-array-of-booleans-to-smaller-size", "mutation.update.field_kinds.with-array-of-floats-to-larger-size", "mutation.update.field_kinds.with-array-of-floats-to-same-size", "mutation.update.field_kinds.with-array-of-floats-to-smaller-size", "mutation.update.field_kinds.with-array-of-ints-to-larger-size-positive-values", "mutation.update.field_kinds.with-array-of-ints-to-same-size-mixed-values", "mutation.update.field_kinds.with-array-of-ints-to-same-size-positive-values", "mutation.update.field_kinds.with-array-of-ints-to-smaller-size-positive-values", "mutation.update.field_kinds.with-array-of-strings-to-larger-size", "mutation.update.field_kinds.with-array-of-strings-to-same-size", "mutation.update.field_kinds.with-array-of-strings-to-smaller-size"]

Updating `[Boolean!]`, `[Float!]`, `[Int!]` or `[String!]` with a same-size, shorter or longer list returns exactly the new list (e.g. `[1, 2, 3, 5, 8]` to `[1, 2, 3]` returns `[1, 2, 3]`; negative ints kept), with no leftover elements.

# `go.mutation.update.nillable_arrays`: Updating arrays with null elements

- **Parents:** ["go.mutation.update"]
- **Scenarios:** ["mutation.update.field_kinds.with-array-of-nillable-booleans", "mutation.update.field_kinds.with-array-of-nillable-floats", "mutation.update.field_kinds.with-array-of-nillable-ints", "mutation.update.field_kinds.with-array-of-strings-ints"]

Updating a `[Boolean]`, `[Float]`, `[Int]` or `[String]` field from e.g. `[1, null, 3]` to `[null, 2, 3, null, 8]` returns the new list with nulls in their positions.

# `go.mutation.upsert.apply`: Upsert adds on no match and updates on one match

- **Parents:** ["go.mutation.upsert"]
- **Scenarios:** ["mutation.upsert.mutation-upsert-simple-with-filter-match-and-version-updates-doc", "mutation.upsert.mutation-upsert-simple-with-filter-match-on-same-field-updates-doc", "mutation.upsert.mutation-upsert-simple-with-filter-match-updates-doc", "mutation.upsert.mutation-upsert-simple-with-no-filter-match-adds-new-doc"]

With no document matching `filter`, `upsert_Users` creates one from `add` and returns it; with exactly one match it applies `update` to that document (including changing the filtered field, `name` "Bob" to "John") and returns the updated document.

# `go.mutation.upsert.errors`: Upsert rejects ambiguous or missing arguments

- **Parents:** ["go.mutation.upsert"]
- **Scenarios:** ["mutation.upsert.mutation-upsert-simple-with-filter-match-multiple-returns-error", "mutation.upsert.mutation-upsert-simple-with-null-add-input-returns-error", "mutation.upsert.mutation-upsert-simple-with-null-filter-input-returns-error", "mutation.upsert.mutation-upsert-simple-with-null-update-input-returns-error", "mutation.upsert.mutation-upsert-simple-with-unique-composite-index-and-duplicate-update-returns-error"]

A filter matching several documents fails with `cannot upsert multiple matching documents`; `add: null`, `update: null` or `filter: null` fail with `Argument "add" has invalid value <nil>` (resp. `"update"`, `"filter"`); an update that collides on a unique composite index fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.delete.by_id`: Delete by docID

- **Parents:** ["go.mutation.delete"]
- **Scenarios:** ["mutation.delete.with-deleted-field", "mutation.delete.with-empty-ids", "mutation.delete.with-idand-alias", "mutation.delete.with-ids", "mutation.delete.with-ids-and-empty-filter", "mutation.delete.with-ids-and-select-alias", "mutation.delete.with-ids-known-and-unknown", "mutation.delete.with-ids-multiple-unknown-id", "mutation.delete.with-ids-single-unknown-id", "mutation.delete.with-idunknown-value", "mutation.delete.with-idunknown-value-and-unrelated-record-in-collection", "mutation.delete.with-update-and-ids-and-select-alias"]

`delete_User(docID: ...)` with one ID or a list deletes and returns those documents (`_deleted` is true in the response; `_docID` can be aliased, also after an update); unknown IDs are skipped and return nothing, without error; `docID: []` deletes nothing; an empty `filter: {}` alongside docIDs does not widen the deletion.

# `go.mutation.delete.by_filter`: Delete by GraphQL filter

- **Parents:** ["go.mutation.delete"]
- **Scenarios:** ["mutation.delete.with-empty-filter", "mutation.delete.with-filter", "mutation.delete.with-filter-matching-multiple-docs", "mutation.delete.with-filter-no-match", "mutation.delete.with-filter-on-empty-collection"]

`delete_User(filter: {name: {_eq: "Shahzad"}})` deletes and returns every matching document; `filter: {}` deletes all; a filter with no match, or on an empty collection, returns an empty list.

# `go.mutation.delete.collection_filter`: Collection API delete-with-filter

- **Parents:** ["go.mutation.delete"]
- **Scenarios:** ["mutation.delete.delete-with-filter-succeeds", "mutation.delete.delete-with-filter-with-multiple-matching-docs-deletes-all", "mutation.delete.delete-with-map-filter-large-integer-straddling2-to53-deletes-only-exact-match", "mutation.delete.delete-with-map-filter-succeeds", "mutation.delete.delete-with-none-option-filter-deletes-all-documents", "mutation.delete.delete-with-some-option-filter-succeeds"]

The collection's delete-with-filter accepts GraphQL filter text, a map or an optional filter and deletes every matching document; no filter deletes all documents. An int filter `_eq: 9007199254740993` deletes only that document, not 9007199254740992.

# `go.mutation.delete.relation`: Deleting related documents

- **Parents:** ["go.mutation.delete"]
- **Scenarios:** ["mutation.delete.field_kinds.one_to_many.test-deletion-of-adocument-using-single-doc-idwith-show-deleted-document-query", "mutation.delete.field_kinds.one_to_one_to_one.test-relational-deletion-of-adocument-using-single-key-success", "mutation.delete.field_kinds.one_to_one_to_one.test-relational-deletion-of-adocument-using-single-key-with-alias-success", "mutation.delete.field_kinds.one_to_one_to_one.test-relational-deletion-of-adocument-using-single-key-with-multi-documents-with-alias-success", "mutation.delete.field_kinds.one_to_one_to_one.txn-deletion-of-related-doc-from-non-primary-side-backward-direction", "mutation.delete.field_kinds.one_to_one_to_one.txn-deletion-of-related-doc-from-non-primary-side-forward-direction", "mutation.delete.field_kinds.one_to_one_to_one.txn-deletion-of-related-doc-from-primary-side-backward-direction", "mutation.delete.field_kinds.one_to_one_to_one.txn-deletion-of-related-doc-from-primary-side-forward-direction"]

Deleting a document that takes part in relations (Book/Author/Publisher one-to-one-to-one) returns its docID. After a committed delete of a linked book, `Publisher { published }` is null and `Book` is empty; after deleting the publisher, `Book { publisher }` is null. `Author(showDeleted: true) { published { _deleted } }` lists the deleted book with `_deleted: true` next to live ones.

# `go.mutation.txn.isolation`: Uncommitted writes are visible only inside their transaction

- **Parents:** ["go.mutation.txn"]
- **Scenarios:** ["mutation.delete.field_kinds.one_to_one_to_one.atxn-can-read-arecord-that-is-deleted-in-anon-commited-txn-backward-direction", "mutation.delete.field_kinds.one_to_one_to_one.atxn-can-read-arecord-that-is-deleted-in-anon-commited-txn-forward-direction", "mutation.delete.mutation-deletion-with-idand-txn", "mutation.delete.mutation-deletion-with-ids-and-txn", "mutation.mix.mutation-with-txn-deletes-user-given-same-transaction", "mutation.mix.mutation-with-txn-does-not-deletes-user-given-different-transactions", "mutation.mix.mutation-with-txn-does-not-update-user-given-different-transactions", "mutation.mix.mutation-with-txn-does-update-user-given-same-transactions"]

A transaction sees its own adds, updates and deletes (a doc deleted in txn 0 is absent from txn 0's queries). Another open transaction does not: it cannot delete a doc added in an uncommitted txn (`delete_User` returns empty), sees the pre-update value, and still reads a doc (and its relation, both directions) deleted in an uncommitted txn until that txn commits.

# `go.mutation.txn.atomic`: Discarded and concurrently used transactions stay atomic

- **Parents:** ["go.mutation.txn"]
- **Scenarios:** ["mutation.add.mutation-add-add-after-discard", "mutation.update.concurrent-commit", "mutation.update.concurrent-discard", "mutation.update.concurrent-write"]

A write to a discarded transaction fails with `transaction not found` and nothing is stored. Two concurrent updates in one transaction leave either one update or the other, never a mix of their fields; an update racing a commit leaves either the old or the new values whole; an add racing a discard leaves the collection empty.

