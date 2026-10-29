# `go.mutation.add.basic`: Adding documents returns them and makes them queryable

- **Parents:** ["go.mutation.add"]

A created document (single or a JSON array of several) is returned by a later `Users { _docID name age }` query with its generated `_docID`. `add_Users(input: {})` creates a document and returns a valid `_docID`; in a schema with ten collections `add_Foo1` returns the given fields; selecting `_version { cid }` on `add_Users` returns one valid CID.

# `go.mutation.add.null_input`: Null and omitted add arguments

- **Parents:** ["go.mutation.add"]

`add_Users(encrypt: null, ...)` and `add_Users(encryptFields: null, ...)` behave as if the argument were absent; `add_Users(input: null)` returns an empty list; `add_Users(input: [null])` fails with `Expected "UsersMutationInputArg!", found null.`. Adding `{"name": "John"}` and then `{"name": "John", "age": null}` creates two distinct documents, both with `age` null.

# `go.mutation.add.default_values`: `@default` field values on create

- **Parents:** ["go.mutation.add"]

Omitted fields take their `@default(value: ...)` for Boolean, DateTime, String, Int, Float, Float32, Float64, JSON and Blob; explicit nulls store null and provided values win. JSON defaults may be int, float, bool, an object or a nested object (returned as canonical sorted-key JSON text); `@default(value: null)` on a JSON field fails schema creation with `default value is invalid`. A defaulted value still counts for a unique index: a second doc defaulting to the same `name` fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.duplicate`: Identical content yields the same docID and is rejected

- **Parents:** ["go.mutation"]

Creating a document whose content matches an existing one (including two all-default documents, or JSON objects whose keys differ only in order) fails with `a document with the given ID already exists`, because the docID is derived deterministically from content.

# `go.mutation.unknown_field`: Writing a field the collection does not have

- **Parents:** ["go.mutation"]

Creating or updating a document with an unknown field fails with `the given field does not exist. Name: <field>` (e.g. `fieldDoesNotExist`, `notName`) and writes nothing. Setting a relation from the one side of a one-to-many (`published` or `_publishedID` on `Author`) fails the same way, naming that field.

# `go.mutation.null_args`: Null filter or docID on update and delete

- **Parents:** ["go.mutation"]
- **Go-Bug-Suspect:** "'docID: null' is treated as no restriction, so the update or delete hits every document in the collection; truncate rejects the same null ('truncate docID cannot be null'). The NullDocIDs tests are copies of the NullDocID tests (both pass 'docID: null')."

`update_Users(filter: null, ...)` / `update_Users(docID: null, ...)` update every document, and `delete_Users(filter: null)` / `delete_Users(docID: null)` delete every document, returning them.

# `go.mutation.constraints`: `@constraints(size:)` on array fields

- **Parents:** ["go.mutation"]

For `numbers: [Int!] @constraints(size: 2)`, creating or updating with a two-element array succeeds and is returned; any other length fails with `array size mismatch`.

# `go.mutation.utc_now`: The `UTC_NOW` DateTime literal

- **Parents:** ["go.mutation"]

`UTC_NOW` as an add, update or upsert input value, or as `@default(value: UTC_NOW)`, stores the current UTC timestamp. Every `UTC_NOW` within one request resolves to the same timestamp, so two aliased mutations in one request return equal `created` values.

# `go.mutation.field_kinds.other_scalars`: Boolean, String, Blob and DateTime scalars round-trip

- **Parents:** ["go.mutation.field_kinds"]

Boolean, String, Blob (hex string `"00FF"` returned unchanged) and DateTime (RFC 3339 string or time value) fields, nillable or `!`, return the stored value. DateTime keeps nanosecond precision: values 1 ns apart are stored as three distinct documents with distinct times.

# `go.mutation.field_kinds.null_scalars`: Nillable scalars accept null

- **Parents:** ["go.mutation.field_kinds"]

Creating a document with null for a nillable Int, Float32, Float64, Boolean, String, Blob or DateTime field returns null for that field.

# `go.mutation.field_kinds.json`: JSON fields hold any JSON value

- **Parents:** ["go.mutation.field_kinds"]

A `JSON` field stores and returns an object, a list of scalars, a list of objects, a number (returned as float64, e.g. 250), a string, a boolean or null.

# `go.mutation.relation.one_to_one_unique`: A one-to-one primary can link only one document

- **Parents:** ["go.mutation.relation"]

Creating or updating a second `Author` whose `_publishedID`/`published` points at a book another author already links fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.relation.dangling`: Links to non-existent documents are accepted

- **Parents:** ["go.mutation.relation"]
- **Go-Bug-Suspect:** "each test carries the comment 'This test should probably not pass, as it contains a reference to a document that doesnt exist'"

Creating or updating a document whose relation ID (`_publishedID`, `published`, `_authorID`, `author`) is a well-formed docID that does not exist succeeds, and the document is returned by a query.

# `go.mutation.update.by_id`: Update by docID

- **Parents:** ["go.mutation.update"]

`update_Users(docID: "<id>", input: {points: 59})` updates and returns only that document; a list of docIDs updates exactly those documents; an unknown docID (`bae-does-not-exist`) returns an empty list without error. Selecting `_version { cid }` returns two entries, the update commit then the create commit.

# `go.mutation.update.collection_filter`: Collection API update-with-filter

- **Parents:** ["go.mutation.update"]

The collection's update-with-filter accepts a filter as GraphQL text, a map, or an optional filter, plus a JSON updater (`{"name": "Eric"}`), and updates matching documents; no filter updates every document. An int filter `_eq: 9007199254740993` updates only that document, not its neighbour 9007199254740992.

# `go.mutation.update.set_null`: Setting a scalar to null

- **Parents:** ["go.mutation.update"]

Updating a nillable Blob, Boolean, DateTime, Float, Int, JSON or String field to `null` makes a later query return null for it.

# `go.mutation.update.array_clear`: Updating an array to null or empty

- **Parents:** ["go.mutation.update"]

Updating a `[Boolean!]`, `[Float!]`, `[Int!]` or `[String!]` field to `null` returns null, and to `[]` returns an empty list (distinct from null).

# `go.mutation.update.array_replace`: Updating an array replaces it whole

- **Parents:** ["go.mutation.update"]

Updating `[Boolean!]`, `[Float!]`, `[Int!]` or `[String!]` with a same-size, shorter or longer list returns exactly the new list (e.g. `[1, 2, 3, 5, 8]` to `[1, 2, 3]` returns `[1, 2, 3]`; negative ints kept), with no leftover elements.

# `go.mutation.update.nillable_arrays`: Updating arrays with null elements

- **Parents:** ["go.mutation.update"]

Updating a `[Boolean]`, `[Float]`, `[Int]` or `[String]` field from e.g. `[1, null, 3]` to `[null, 2, 3, null, 8]` returns the new list with nulls in their positions.

# `go.mutation.upsert.apply`: Upsert adds on no match and updates on one match

- **Parents:** ["go.mutation.upsert"]

With no document matching `filter`, `upsert_Users` creates one from `add` and returns it; with exactly one match it applies `update` to that document (including changing the filtered field, `name` "Bob" to "John") and returns the updated document.

# `go.mutation.upsert.errors`: Upsert rejects ambiguous or missing arguments

- **Parents:** ["go.mutation.upsert"]

A filter matching several documents fails with `cannot upsert multiple matching documents`; `add: null`, `update: null` or `filter: null` fail with `Argument "add" has invalid value <nil>` (resp. `"update"`, `"filter"`); an update that collides on a unique composite index fails with `can not index a doc's field(s) that violates unique index`.

# `go.mutation.delete.by_id`: Delete by docID

- **Parents:** ["go.mutation.delete"]

`delete_User(docID: ...)` with one ID or a list deletes and returns those documents (`_deleted` is true in the response; `_docID` can be aliased, also after an update); unknown IDs are skipped and return nothing, without error; `docID: []` deletes nothing; an empty `filter: {}` alongside docIDs does not widen the deletion.

# `go.mutation.delete.by_filter`: Delete by GraphQL filter

- **Parents:** ["go.mutation.delete"]

`delete_User(filter: {name: {_eq: "Shahzad"}})` deletes and returns every matching document; `filter: {}` deletes all; a filter with no match, or on an empty collection, returns an empty list.

# `go.mutation.delete.collection_filter`: Collection API delete-with-filter

- **Parents:** ["go.mutation.delete"]

The collection's delete-with-filter accepts GraphQL filter text, a map or an optional filter and deletes every matching document; no filter deletes all documents. An int filter `_eq: 9007199254740993` deletes only that document, not 9007199254740992.

# `go.mutation.delete.relation`: Deleting related documents

- **Parents:** ["go.mutation.delete"]

Deleting a document that takes part in relations (Book/Author/Publisher one-to-one-to-one) returns its docID. After a committed delete of a linked book, `Publisher { published }` is null and `Book` is empty; after deleting the publisher, `Book { publisher }` is null. `Author(showDeleted: true) { published { _deleted } }` lists the deleted book with `_deleted: true` next to live ones.

# `go.mutation.txn.isolation`: Uncommitted writes are visible only inside their transaction

- **Parents:** ["go.mutation.txn"]

A transaction sees its own adds, updates and deletes (a doc deleted in txn 0 is absent from txn 0's queries). Another open transaction does not: it cannot delete a doc added in an uncommitted txn (`delete_User` returns empty), sees the pre-update value, and still reads a doc (and its relation, both directions) deleted in an uncommitted txn until that txn commits.

# `go.mutation.txn.atomic`: Discarded and concurrently used transactions stay atomic

- **Parents:** ["go.mutation.txn"]

A write to a discarded transaction fails with `transaction not found` and nothing is stored. Two concurrent updates in one transaction leave either one update or the other, never a mix of their fields; an update racing a commit leaves either the old or the new values whole; an add racing a discard leaves the collection empty.

