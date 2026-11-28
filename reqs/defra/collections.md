# `go.collection_version.define.basic_types`: SDL types become queryable GraphQL types

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.col-version-simple-adds-col-given-empty-type", "collection_version.inline-array-adds-collection-given-second-type", "collection_version.inline-array-adds-collection-given-single-type", "collection_version.simple-adds-collection-given-leading-comment", "collection_version.simple-adds-collection-given-leading-comment-with-multi-byte-char", "collection_version.simple-adds-collection-given-new-types", "collection_version.simple-adds-collection-given-type-with-string-field", "collection_version.simple-adds-collection-with-default-fields-given-empty-type"]

Adding an SDL type (empty, with scalar fields, with inline arrays, several types in one SDL, or preceded by `#` comments including multi-byte characters) produces a GraphQL object type whose fields are the default system fields (`_docID`, `_version`, `_deleted`, aggregates, `_group`) plus the declared fields, as seen through `__type(name: ...)` introspection.

# `go.collection_version.define.default_values`: `@default` field values

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.collection-version-with-default-field-values", "collection_version.with-default-field-value-no-arg-returns-error", "collection_version.with-default-field-value-on-list-returns-error", "collection_version.with-default-field-value-on-relation-returns-error", "collection_version.with-incorrect-default-field-value-type-returns-error", "collection_version.with-invalid-default-field-value-type-returns-error", "collection_version.with-unknown-default-field-arg-returns-error"]

`@default(<kind>: value)` on a scalar field records the default in the collection definition. A value of the wrong kind fails with `default value is invalid. Field: active, Expected: Boolean, Actual: Int, Value: 10`; no argument fails with `default value must specify one argument. Field: name`; an unknown argument fails with `Unknown argument "bool" on directive "@default"`; a default on a relation or list fails with `default value is not allowed for this field type. Name: friend, Type: User` / `Type: List`.

# `go.collection_version.define.crdt_types`: `@crdt` counter types in SDL

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.collection-add-contains-pcounter-type-with-float-kind-no-error", "collection_version.collection-add-contains-pcounter-type-with-float32-kind-no-error", "collection_version.collection-add-contains-pcounter-type-with-float64-kind-no-error", "collection_version.collection-add-contains-pcounter-type-with-int-kind-no-error", "collection_version.collection-add-contains-pncounter-type-with-float-kind-no-error", "collection_version.collection-add-contains-pncounter-type-with-int-kind-no-error", "collection_version.test-collection-add-contains-pcounter-type-with-wrong-kind-error", "collection_version.test-collection-add-contains-pncounter-type-with-wrong-kind-error", "collection_version.test-collection-add-contains-pncounter-with-invalid-type-error"]

`@crdt(type: pncounter)` and `@crdt(type: pcounter)` are accepted on `Int` and float fields (`Float`, `Float32`, `Float64`); on a `String` field they fail with `CRDT type pncounter can't be assigned to field kind String` (resp. `pcounter`); an unknown type fails with `Argument "type" has invalid value "invalid"`.

# `go.collection_version.define.branchable`: `@branchable` directive

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.col-version-branchable-argument-if-false-should-be-false", "collection_version.col-version-branchable-argument-if-true-should-be-true", "collection_version.col-version-branchable-no-arguments-default-true"]

`@branchable` with no argument or `if: true` sets `IsBranchable: true` on the collection; `@branchable(if: false)` leaves it false.

# `go.collection_version.relations.self_ref_ids`: Collection IDs of self-referencing collection sets

- **Parents:** ["go.collection_version.relations"]
- **Scenarios:** ["collection_version.collection-version-self-reference-simple-has-simple-collection-id", "collection_version.collection-version-self-reference-two-pairs-of-two-types-have-different-complex-collection-id", "collection_version.collection-version-self-reference-two-pairs-of-two-types-joined-by-third-circle-across-all-all-have-same-base-collection-id", "collection_version.collection-version-self-reference-two-pairs-of-two-types-joined-by-third-circle-all-have-same-base-collection-id", "collection_version.collection-version-self-reference-two-types-has-complex-collection-id", "collection_version.collection-version-self-reference-two-types-has-complex-collection-id-single-sided-relations", "collection_version.collection-version-with-one-one-one-many-self-ref"]

A collection with a relation to itself gets a plain self kind with no base ID; collections joined in a relation cycle share one base collection ID with distinct index suffixes, separate cycles get different base IDs, and cycles joined by a third cycle all share one base ID. Mixed one-one and one-many self relations produce correct definitions from `GetCollections`.

# `go.collection_version.version_branches`: Branching collection version history

- **Parents:** ["go.collection_version"]
- **Scenarios:** ["collection_version.updates.collection-version-updates-with-branching-collection", "collection_version.updates.collection-version-updates-with-branching-collection-and-get-collection-at-version", "collection_version.updates.collection-version-updates-with-branching-collection-and-set-active-collection-to-other-branch", "collection_version.updates.collection-version-updates-with-branching-collection-and-set-active-collection-to-other-branch-then-patch", "collection_version.updates.collection-version-updates-with-patch-on-branched-collection"]

Patching an older (non-active) version creates a sibling branch of versions: each branch keeps its own fields (`Cannot query field "phone" on type "Users".` on the other branch), either branch can be activated and patched further, and `GetCollections` returns every version in the tree.

# `go.collection_version.patch.add_field.basic`: Added fields become queryable

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.field.collection-version-updates-add-field-does-not-affect-existing-relation", "collection_version.updates.add.field.collection-version-updates-add-field-introspection", "collection_version.updates.add.field.collection-version-updates-add-field-multiple-in-patch", "collection_version.updates.add.field.collection-version-updates-add-field-multiple-patches", "collection_version.updates.add.field.collection-version-updates-add-field-simple", "collection_version.updates.add.field.collection-version-updates-add-field-simple-do-not-set-default-version-is-queryable", "collection_version.updates.add.field.collection-version-updates-add-field-simple-inactive-false-errors", "collection_version.updates.add.field.collection-version-updates-add-field-simple-with-filter", "collection_version.updates.add.field.collection-version-updates-add-field-simple-with-filter-on-populated-database", "collection_version.updates.add.field.constraint.collection-version-updates-add-field-size-contraint-should-succeed"]

After adding one or several fields (in one or successive patches) the new version is active and the fields can be queried, filtered and introspected, existing relations are unaffected, and `@constraints(size: 2)` on an added array field is enforced (`array size mismatch. Actual: 3, Expected: 2`). With `SetAsActiveVersion` false the new field is not queryable (`Cannot query field "email" on type "Users".`) but the version is still returned by `GetCollections`.

# `go.collection_version.patch.add_field.atomicity`: A failing patch changes nothing

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.field.collection-version-updates-add-field-introspection-does-not-amend-gql-types-given-bad-patch", "collection_version.updates.add.field.collection-version-updates-add-field-multiple-in-patch-partial-success"]

When one operation in a patch fails (`no type found for given name. Type: 111`), none of its operations apply: the valid field is not queryable, the original definition remains and the GraphQL types are not amended.

# `go.collection_version.patch.add_field.validation`: Added field validation

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.field.collection-version-updates-add-field-simple-duplicate-field", "collection_version.updates.add.field.collection-version-updates-add-field-simple-duplicate-of-existing-field", "collection_version.updates.add.field.collection-version-updates-add-field-simple-duplicate-of-existing-field-multiple", "collection_version.updates.add.field.collection-version-updates-add-field-simple-without-name"]

An added field without a name fails with `Names must match /^[_a-zA-Z][_a-zA-Z0-9]*$/ but "" does not.`; a name that duplicates an existing or another added field fails with `duplicate field. Name: name`.

# `go.collection_version.patch.add_field.non_nillable`: Non-nillable fields cannot be added

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.test.collection-version-updates-add-non-nillable-blob-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-bool-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-date-time-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-float32-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-float64-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-int-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-json-field-error", "collection_version.updates.test.collection-version-updates-add-non-nillable-string-field-error"]

Adding a non-nillable field (Boolean, Int, Float64, Float32, String, DateTime, Blob or JSON) to an existing collection fails with `adding a non-nillable field to an existing collection is not supported`.

# `go.collection_version.patch.add_field.existing_docs`: Documents across a field addition

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.field.collection-version-updates-add-field-with-add", "collection_version.updates.add.field.collection-version-updates-add-field-with-add-after-collection-update", "collection_version.updates.add.field.collection-version-updates-add-field-with-add-with-update-after-collection-update-and-commit-query", "collection_version.updates.add.field.collection-version-updates-add-field-with-add-with-update-after-collection-update-and-version-join", "collection_version.updates.add.field.collection-version-updates-add-field-with-existing-secondary-one-to-one-relationship"]

Documents created before a field is added return `null` for it; documents created afterwards, updates after the patch, version joins and `_commits` queries all work across the version change, including with an existing secondary one-to-one relation.

# `go.collection_version.patch.add_field.by_name_path`: Adding a field addressed by name

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.field.collection-version-updates-add-field-simple-field-indexed-by-name", "collection_version.updates.add.field.collection-version-updates-add-field-simple-field-indexed-by-name-multiple-times", "collection_version.updates.add.field.collection-version-updates-add-field-simple-field-indexed-by-name-with-different-name-defined-in-value", "collection_version.updates.add.field.collection-version-updates-add-field-simple-field-indexed-by-name-with-same-name-defined-in-value"]

A field added at `/Users/Fields/email` takes its name from the path (a matching `Name` in the value is fine, a different one fails with `the index used does not match the given name`), and later operations in the same patch can address it by that name.

# `go.collection_version.patch.add_field.branchable`: Adding fields to a branchable collection

- **Parents:** ["go.collection_version.patch.add_field"]
- **Scenarios:** ["collection_version.updates.add.branchable-collection-add-new-field-should-update-collection-definition", "collection_version.updates.add.branchable-collection-add-new-field-with-multiple-docs-should-add-field"]

Adding a field to a `@branchable` collection updates its definition and documents can then be created and updated with the new field.

# `go.collection_version.patch.add_field.crdt.valid`: Supported CRDT types

- **Parents:** ["go.collection_version.patch.add_field.crdt"]
- **Scenarios:** ["collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-default", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-none", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-typ-as-string-no-error", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdtlww", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdtp-counter-no-error", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdtpn-counter-no-error"]

An added field with no `Typ`, `Typ` 0 (none), LWW, `pcounter` or `pncounter` on a numeric kind is accepted and queryable; `Typ` may also be given as its string name (e.g. `"lww"`).

# `go.collection_version.patch.add_field.crdt.invalid`: Rejected CRDT types

- **Parents:** ["go.collection_version.patch.add_field.crdt"]
- **Scenarios:** ["collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-composite-errors", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-composite-errors-multiple", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-invalid-errors", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-invalid-errors-multiple", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-object-with-bool-field-errors", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-object-with-bool-field-errors-multiple", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdt-typ-as-unknown-string-error", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdtp-counter-with-mismatch-kind-error", "collection_version.updates.add.field.crdt.collection-version-updates-add-field-crdtpn-counter-with-mismatch-kind-error"]

Composite, object or unknown numeric `Typ` values fail with `CRDT type not supported. Name: foo, CRDTType: composite` (`object`, `unknown`), one line per bad field; a counter on a Boolean fails with `CRDT type pcounter can't be assigned to field kind Boolean` (resp. `pncounter`); an unknown string name fails with `unknown crdt string representation`.

# `go.collection_version.patch.add_field.kind.numeric`: Numeric kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Scenarios:** ["collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float32", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float32-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-float32-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-int", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-int-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-int-with-add"]

Adding an `Int`, `Float` (Float64) or `Float32` field by kind ID or by name makes it queryable, and documents created afterwards store and return values of that kind.

# `go.collection_version.patch.add_field.kind.other_scalars`: Boolean, String, DateTime and DocID kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Scenarios:** ["collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-bool", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-bool-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-bool-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-date-time", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-date-time-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-date-time-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-doc-id", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-doc-id-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-doc-id-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-string", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-string-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-string-with-add"]

Adding a `Boolean`, `String`, `DateTime` or `ID` (DocID) field by kind ID or by name makes it queryable, and documents created afterwards store and return values of that kind.

# `go.collection_version.patch.add_field.kind.blob_json`: Blob and JSON kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Scenarios:** ["collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-blob", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-blob-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-blob-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-json", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-json-substitution-with-add", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-json-with-add"]

Adding a `Blob` or `JSON` field by kind ID or by name makes it queryable, and documents created afterwards store and return hex blobs and JSON values.

# `go.collection_version.patch.add_field.kind.invalid`: Invalid kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Scenarios:** ["collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-invalid", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind-none", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind198", "collection_version.updates.add.field.kind.collection-version-updates-add-field-kind32"]

An added field with kind 0, an unassigned kind ID (32, 198) or an unknown kind name fails with `no type found for given name. Type: 32` / `Field: foo, Kind: InvalidKind`.

# `go.collection_version.patch.add_collection`: Collections cannot be added by patch

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.add.col-version-update-add-collections-errors", "collection_version.updates.add.col-version-update-add-collections-with-empty-id-errors", "collection_version.updates.add.col-version-update-add-collections-with-no-index-errors", "collection_version.updates.add.col-version-update-add-collections-with-undefined-id-errors", "collection_version.updates.add.collection-version-updates-add-simple-errors-adding-collection", "collection_version.updates.copy.collection-version-updates-copy-collection-with-remove-id-and-replace-name", "collection_version.updates.replace.collection-version-updates-replace-collection-errors"]

A patch that would create a collection (add at an unknown top-level path, `/-`, a copy of a collection under a new name, or a replace of a collection with another) fails with `adding collections via patch is not supported`, naming the new collection (e.g. `. Name: Book`).

# `go.collection_version.patch.invalid_paths`: Patches to unknown paths or properties

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.add.collection-version-updates-add-simple-errors-adding-collection-prop", "collection_version.updates.add.collection-version-updates-add-simple-errors-adding-unsupported-collection-prop", "collection_version.updates.add.collection-version-updates-add-simple-errors-adding-unsupported-collection-prop-at-root", "collection_version.updates.add.field.collection-version-updates-add-field-simple-errors-adding-to-unknown-collection"]

Adding an unknown property to a collection fails with `json: unknown field "-"`; adding under a path that does not exist (unknown collection, unsupported property, at the root) fails with `add operation does not apply: doc is missing path`.

# `go.collection_version.patch.field_reorder`: Fields cannot be moved

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.copy.field.collection-version-updates-copy-field-errors", "collection_version.updates.copy.field.collection-version-updates-copy-field-errors-multiple", "collection_version.updates.move.field.collection-version-updates-move-field-errors", "collection_version.updates.move.field.collection-version-updates-move-field-errors-multiple"]

Copying or moving a field onto another position fails with `moving fields is not currently supported. Name: name, ProposedIndex: 1, ExistingIndex: 2`, one line per moved field.

# `go.collection_version.patch.copy_field`: Copying a field as a new field

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.copy.field.collection-version-updates-copy-field-and-replace-name-and-invalid-kind-substitution", "collection_version.updates.copy.field.collection-version-updates-copy-field-introspection-with-remove-id-and-replace-name", "collection_version.updates.copy.field.collection-version-updates-copy-field-with-and-replace-name", "collection_version.updates.copy.field.collection-version-updates-copy-field-with-replace-name-and-kind-substitution"]

Copying a field to the end of the field list and renaming it (optionally replacing its kind by name) adds a new queryable field with no values for existing documents; an invalid substituted kind fails with `no type found for given name. Field: Age, Kind: NotAValidKind`.

# `go.collection_version.patch.immutable_identity`: IDs and sources are immutable

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.remove.collection-version-updates-remove-version-id-errors", "collection_version.updates.replace.col-version-update-replace-collection-id-errors", "collection_version.updates.replace.col-version-update-replace-collection-source-source-collection-id-errors", "collection_version.updates.replace.col-version-update-replace-id-with-empty-errors", "collection_version.updates.replace.col-version-update-replace-id-with-existing-different-root-errors", "collection_version.updates.replace.col-version-update-replace-id-with-existing-errors", "collection_version.updates.replace.col-version-update-replace-id-with-existing-same-root-errors", "collection_version.updates.replace.col-version-update-replace-id-with-new-errors", "collection_version.updates.replace.col-version-update-replace-sources-errors", "collection_version.updates.replace.col-version-update-replace-sources-with-query-source-errors"]

Patching identity properties fails: empty `CollectionID` with `collection ID cannot be empty`, changing it with `collection ID cannot be mutated.`, an invalid version ID with `invalid cid: selected encoding not supported. VersionID: invalid cid`, removing it with `invalid cid: cid too short. VersionID:`, an unknown CID with `unknown CID, collection ids cannot be manually defined`, adding or removing sources with `collection sources cannot be added or removed.`, a source from another collection with `collection source must belong to host collection.`, and a changed source collection ID with `collection source ID cannot be mutated.`.

# `go.collection_version.patch.remove_field`: Removing and replacing fields

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.remove.fields.collection-version-updates-remove-all-fields", "collection_version.updates.remove.fields.collection-version-updates-remove-field", "collection_version.updates.replace.field.collection-version-updates-replace-field"]

Removing a field (or all fields) or replacing it by a different field creates a version in which the old field is no longer queryable (`Cannot query field "name" on type "Users".`) while other fields still are.

# `go.collection_version.patch.field_mutation_errors`: Existing field properties are immutable

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.remove.col-version-update-field-field-size-contraint-should-error", "collection_version.updates.remove.fields.collection-version-updates-remove-field-kind-errors", "collection_version.updates.remove.fields.collection-version-updates-remove-field-name-errors", "collection_version.updates.remove.fields.collection-version-updates-remove-field-typ-errors", "collection_version.updates.replace.col-version-update-replace-default-value-errors", "collection_version.updates.replace.col-version-update-replace-field-size-contraint-should-error", "collection_version.updates.replace.col-version-update-replace-fields-errors"]

Removing or replacing a property of an existing field (name, kind, CRDT type, default value, size constraint) fails with `mutating an existing field is not supported` (with `ProposedName: ...`); replacing the whole fields list with an invalid entry fails with `no type found for given name. Type: 0`.

# `go.collection_version.patch.embeddings.valid`: Valid embedding patches apply

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Scenarios:** ["collection_version.updates.add.col-version-update-add-vector-embedding-should-succeed", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-url-should-succeed", "collection_version.updates.remove.col-version-update-remove-vector-embedding-should-succeed", "collection_version.updates.replace.col-version-update-replace-vector-embedding-params-should-succeed"]

Adding an embedding (a missing URL is allowed), replacing its parameters, or removing it by patch succeeds and is reflected in the `VectorEmbeddings` returned by `GetCollections`.

# `go.collection_version.patch.embeddings.missing_settings`: Embedding settings are required

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Scenarios:** ["collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-field-name-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-field-name-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-fields-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-fields-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-model-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-model-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-provider-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-missing-provider-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unsupported-provider-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unsupported-provider-should-error-multiple"]

An added embedding without a field name fails with `embedding FieldName cannot be empty`, without source fields with `embedding Fields cannot be empty`, without a provider with `embedding Provider cannot be empty` (followed by `unknown embedding provider. Provider: `), with an unknown provider with `unknown embedding provider. Provider: deepseek`, and without a model with `embedding Model cannot be empty`.

# `go.collection_version.patch.embeddings.field_references`: Embedding field references are validated

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Scenarios:** ["collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-another-embedding-in-patch-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-another-embedding-in-patch-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-another-embedding-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-another-embedding-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-self-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-reference-to-self-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-invalid-embedding-generation-field-kind-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unknown-embedding-generation-field-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unknown-embedding-generation-field-should-error-multiple", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unknown-field-name-should-error", "collection_version.updates.add.col-version-update-add-vector-embedding-with-unknown-field-name-should-error-multiple", "collection_version.updates.replace.col-version-update-replace-vector-embedding-with-invalid-embedding-generation-field-kind-should-error", "collection_version.updates.replace.col-version-update-replace-vector-embedding-with-unknown-embedding-generation-field-should-error", "collection_version.updates.replace.col-version-update-replace-vector-embedding-with-unknown-field-name-should-error"]

An added or replaced embedding naming an unknown vector field fails with `the given field does not exist. Vector field: foo`, an unknown source field with `the given field does not exist. Embedding generation field: foo`, a JSON source with `invalid field type for vector embedding generation. Actual: JSON`, and a source that is itself or another embedding field (existing or added in the same patch) with `embedding fields cannot refer to self or another embedding field. Field: name_v`.

# `go.collection_version.patch.test_op`: JSON Patch `test` operations

- **Parents:** ["go.collection_version.patch"]
- **Scenarios:** ["collection_version.updates.test.col-version-update-test-name", "collection_version.updates.test.col-version-update-test-name-by-version-id", "collection_version.updates.test.col-version-update-test-name-by-version-id-fails", "collection_version.updates.test.col-version-update-test-name-fails", "collection_version.updates.test.collection-version-updates-test-add-field", "collection_version.updates.test.collection-version-updates-test-add-field-blocked-by-test", "collection_version.updates.test.field.collection-version-updates-test-field-errors", "collection_version.updates.test.field.collection-version-updates-test-field-name-errors", "collection_version.updates.test.field.collection-version-updates-test-field-name-passes", "collection_version.updates.test.field.collection-version-updates-test-field-passes", "collection_version.updates.test.field.collection-version-updates-test-field-passes-targetting-kind-using-field-name-as-index", "collection_version.updates.test.field.collection-version-updates-test-field-passes-using-field-name-as-index"]

A `test` op on a collection name, field, field name or kind (addressed by version ID, collection name, field index or field name) lets the patch apply when it matches; when it does not, the whole patch fails with `test failed` (e.g. `testing value /<versionID>/Name failed: test failed`) and no field is added.

# `go.collection_version.migrations.configure`: Configuring migrations

- **Parents:** ["go.collection_version.migrations"]
- **Scenarios:** ["collection_version.migrations.collection-migration-configure-migration-skipping-version-errors", "collection_version.migrations.collection-migration-does-not-error-given-unknown-collection-roots", "collection_version.migrations.collection-migration-get-migrations-returns-multiple", "collection_version.migrations.collection-migration-get-migrations-with-txn", "collection_version.migrations.collection-migration-replaces-existing-migation-based-on-source-id"]

`ConfigureMigration` (`client schema migration set`) records a transform between two versions, even for unknown versions; several are returned by `GetCollections`, a second one with the same source replaces the first, a migration set in a transaction is visible inside it, and one between non-adjacent versions fails with `cannot migrate between non-adjacent collection versions`.

# `go.collection_version.migrations.query.forward`: Forward migrations across versions

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-with-migration-only-on-first-step-applies-only-first-step", "collection_version.migrations.query.collection-migration-query-with-migration-only-on-last-step-applies-only-last-step", "collection_version.migrations.query.collection-migration-query-with-migration-registered-before-patch-applies-migration", "collection_version.migrations.query.collection-migration-query-with-migrations-registered-before-patches-applies-all-migrations", "collection_version.migrations.query.collection-migration-query-with-migrations-registered-before-patches-in-reverse-order-applies-all-migrations", "collection_version.migrations.query.collection-migration-query-with-multiple-docs-applies-migration-to-all", "collection_version.migrations.query.collection-migration-query-with-single-migration-applies-migration", "collection_version.migrations.query.collection-migration-query-with-unknown-collection-version-ids-ignores-migration"]

A migration from version 1 to 2 is applied to every document written at version 1, whether registered before or after the patch; with several versions each registered step is applied in order (registered in any order), a missing step is skipped (no transform for that edge), and migrations naming unknown versions are ignored.

# `go.collection_version.migrations.query.field_mapping`: Migrated field values

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-with-migration-copying-field-when-src-and-dst-field-not-requested-applies-copy", "collection_version.migrations.query.collection-migration-query-with-migration-copying-field-when-src-field-not-requested-applies-copy", "collection_version.migrations.query.collection-migration-query-with-migration-mutating-field-not-in-first-request-preserves-unmutated-field-in-second-request", "collection_version.migrations.query.collection-migration-query-with-migration-mutating-inline-array-field-applies-mutation", "collection_version.migrations.query.collection-migration-query-with-migration-mutating-scalar-field-applies-mutation", "collection_version.migrations.query.collection-migration-query-with-migration-removing-field-returns-nil-for-removed-field"]

Migrations that set a scalar, mutate an inline array, remove a field (returned as `null`) or copy a field produce those values even when the source or destination field is not requested, and a later request for a different field set still sees unmutated fields.

# `defra.collection_version.migrations.query.date_time_format`: Migrated DateTime values keep Go's format

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version::migrations::query::with_date_time::collection_migration_query_renders_migrated_date_times_without_trailing_zeros"]

A DateTime field of a document read through a migration returns as Go's `time.RFC3339Nano` writes it, the same as before the migration: no fractional seconds when they are zero and no trailing zeros otherwise. With `Users { required: DateTime! @index, optional: DateTime }` holding `{required: "2026-08-01T12:34:56Z", optional: "1999-12-31T23:59:59Z"}` and `{required: "2026-08-02T12:34:56.120Z", optional: "1999-12-31T23:59:59.000000001Z"}`, a patch whose `set-default` lens sets a new `migrated` field to `true` makes the query return `"2026-08-01T12:34:56Z"`, `"1999-12-31T23:59:59Z"`, `"2026-08-02T12:34:56.12Z"` and `"1999-12-31T23:59:59.000000001Z"`, never `"2026-08-01T12:34:56.000000000Z"`.

# `go.collection_version.migrations.query.active_version`: Migrations follow the active version

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-inverses-across-multiple-versions", "collection_version.migrations.query.collection-migration-query-with-branching-collection", "collection_version.migrations.query.collection-migration-query-with-set-default-to-latest-applies-forward-migration", "collection_version.migrations.query.collection-migration-query-with-set-default-to-original-applies-inverse-migration"]

Activating the latest version applies forward migrations; activating an older version applies inverse migrations, including across several versions; querying a branch of the version tree returns documents migrated along that branch.

# `go.collection_version.migrations.query.filters`: Filtering and docID lookup over migrated documents

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-by-doc-id", "collection_version.migrations.query.collection-migration-query-multiple-queries-by-doc-id", "collection_version.migrations.query.collection-migration-query-with-filter-and-migration-between-old-versions-should-apply-migration", "collection_version.migrations.query.collection-migration-query-with-filter-and-migration-in-old-patch-should-apply-migration2", "collection_version.migrations.query.collection-migration-query-with-filter-should-filter-fmigration"]

Filters are evaluated against migrated values (also when the migration sits between older versions or in an older patch), and querying by one or several `docID`s returns migrated documents without hanging.

# `go.collection_version.migrations.query.txn`: Migrations inside transactions

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-with-txn", "collection_version.migrations.query.collection-migration-query-with-txn-and-commit"]

A migration configured in a transaction applies to queries in that transaction, and after commit to queries outside it.

# `go.collection_version.migrations.query.update`: Migrations and updates

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-with-migration-registered-after-update", "collection_version.migrations.query.collection-migration-query-with-update-request"]

An `update_` mutation on a document written at an older version returns migrated values, and a migration registered after the document was updated still applies.

# `defra.collection_version.migrations.transform_with_version`: A visible version's transform is never missing

- **Parents:** ["go.collection_version.migrations"]
- **Scenarios:** ["collection_version::migrations::query::with_concurrent_update::collection_migration_updates_during_a_lens_patch_find_the_transform", "collection_version::migrations::query::with_txn_concurrent_read::collection_migration_reads_during_a_txn_migration_commit_find_the_transform"]

A collection version whose `PreviousVersion.Transform` names a lens, committed by `PATCH /api/v0/collections` with a lens or by a migration set in an explicit transaction, has its transform available to every request that sees the version, from the moment the commit lands: reads, `update_` mutations, deferred index drains and P2P merges that read the version from storage or from the collection cache while the patch or commit is still returning migrate documents through the lens, and none fails with `transform not found: <lens id>` or `lens migration failed for document <docID>: transform not found: <lens id>`. A commit that fails leaves its lens unknown.

# `go.collection_version.define.scalar_kinds`: Every scalar kind is accepted and introspected

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.simple-adds-collection-given-type-with-blob-field", "collection_version.simple-with-all-types-adds-collection-given-types", "collection_version.simple-with-float-field-adds-collection-given-type", "collection_version.simple-with-float32-field-adds-collection-given-type", "collection_version.simple-with-float64-field-adds-collection-given-type", "collection_version.simple-with-jsonfield-adds-collection-given-type"]
- **Go-Bug-Suspect:** "WithFloatField and WithAllTypes carry a TODO saying they document a graphql-go introspection bug (Go issue #3429); the asserted introspection output for 'Float' may not be the intended one."

Fields of kind `Blob`, `JSON`, `Float32`, `Float64`, `Float` and every other supported scalar can be declared in SDL; introspection reports `Float` and `Float64` fields as the `Float64` scalar and `Float32` as `Float32`.

# `go.collection_version.define.errors`: Invalid SDL definitions are rejected

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.simple-errors-given-duplicate-collection", "collection_version.simple-errors-given-duplicate-collection-in-same-sdl", "collection_version.simple-errors-given-duplicate-collection-in-same-sdlmultiple", "collection_version.simple-errors-given-non-null-many-relation-field", "collection_version.simple-errors-given-type-with-invalid-field-type", "collection_version.simple-errors-given-type-with-invalid-field-type-multiple", "collection_version.with-missing-type-errors"]

Adding a collection whose name already exists (in the store or twice in one SDL) fails with `collection already exists` (one `collection already exists. Name: Users` line per duplicate); an unknown field type fails with `no type found for given name. Field: name, Kind: NotAType` (one line per bad field); a non-null list relation such as `[Dogs!]` fails with `NonNull variants for type are not supported. Type: Dogs`; a field with no type fails with `field type not specified. Object: User, Field: name`.

# `go.collection_version.define.field_type_forms`: Which field type declarations are accepted

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version::field_type_forms::every_scalar_and_a_relation_is_accepted_nillable", "collection_version::field_type_forms::every_scalar_but_id_is_accepted_non_null", "collection_version::field_type_forms::lists_of_array_scalars_and_relations_are_accepted", "collection_version::field_type_forms::lists_of_non_null_array_scalars_are_accepted", "collection_version::field_type_forms::a_non_null_id_is_refused", "collection_version::field_type_forms::a_non_null_relation_is_refused", "collection_version::field_type_forms::a_non_null_scalar_list_is_refused", "collection_version::field_type_forms::a_non_null_list_of_non_null_scalars_is_refused", "collection_version::field_type_forms::a_non_null_relation_list_is_refused", "collection_version::field_type_forms::a_list_of_non_null_ids_is_refused", "collection_version::field_type_forms::a_list_of_non_null_blobs_is_refused", "collection_version::field_type_forms::a_list_of_non_null_json_is_refused", "collection_version::field_type_forms::a_list_of_non_null_relations_is_refused", "collection_version::field_type_forms::a_list_of_ids_is_refused", "collection_version::field_type_forms::a_list_of_blobs_is_refused", "collection_version::field_type_forms::a_list_of_json_is_refused", "collection_version::field_type_forms::a_nested_list_is_refused", "collection_version::field_type_forms::a_refusal_in_a_transaction_keeps_its_code", "collection_version::field_type_forms::an_unknown_type_beside_a_refused_list_keeps_the_code"]

A field declared in SDL takes one of these forms; `Author` stands for any collection (a relation):

| Type | `T` | `T!` | `[T]` | `[T!]` | `[T]!`, `[T!]!` |
|------|-----|------|-------|--------|-----------------|
| `String`, `Int`, `Float`, `Float64`, `Float32`, `Boolean`, `DateTime` | accepted | accepted | accepted | accepted | refused (1) |
| `JSON`, `Blob` | accepted | accepted | refused (3) | refused (2) | refused (1) |
| `ID` | accepted | refused (1) | refused (3) | refused (2) | refused (1) |
| `Author` | accepted | refused (1) | accepted | refused (2) | refused (1) |

A list of lists (`[[Int]]`, with or without `!`) is refused with (4), and with (1) when the outer list is non-null. Each refusal fails with Go's text, for a field `value` on `User`:

1. `NonNull fields are not currently supported`
2. `NonNull variants for type are not supported. Type: ID` (the element type)
3. `no type found for given name. Field: value, Kind: [ID]` (the element type)
4. `nested list types are not supported. Object: User, Field: value`

A field of an unknown type beside a list refused with (3) adds its own line (`no type found for given name. Field: a, Kind: NotAType`); the lines are sorted, one per field. A refusal inside an explicit transaction has the same text.

Accepting a form the table refuses, refusing one it accepts, or changing a refusal's text or code (`defra.collection_version.define.field_type_code`) is a breaking change.

# `defra.collection_version.define.field_type_code`: A refused field type declaration carries a code

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version::field_type_forms::a_non_null_id_is_refused", "collection_version::field_type_forms::a_non_null_relation_is_refused", "collection_version::field_type_forms::a_non_null_scalar_list_is_refused", "collection_version::field_type_forms::a_non_null_list_of_non_null_scalars_is_refused", "collection_version::field_type_forms::a_non_null_relation_list_is_refused", "collection_version::field_type_forms::a_list_of_non_null_ids_is_refused", "collection_version::field_type_forms::a_list_of_non_null_blobs_is_refused", "collection_version::field_type_forms::a_list_of_non_null_json_is_refused", "collection_version::field_type_forms::a_list_of_non_null_relations_is_refused", "collection_version::field_type_forms::a_list_of_ids_is_refused", "collection_version::field_type_forms::a_list_of_blobs_is_refused", "collection_version::field_type_forms::a_list_of_json_is_refused", "collection_version::field_type_forms::a_nested_list_is_refused", "collection_version::field_type_forms::a_refusal_in_a_transaction_keeps_its_code", "collection_version::field_type_forms::an_unknown_type_beside_a_refused_list_keeps_the_code"]

Every refusal in `go.collection_version.define.field_type_forms` reaches an in-process or HTTP client of a Rust node with the code `UNSUPPORTED_FIELD_TYPE` beside Go's text, in autocommit and inside an explicit transaction alike: the in-process error's `code()` (`AddCollectionError::UnsupportedFieldType`), and the `code` member of the HTTP error body (`{"error": "NonNull fields are not currently supported", "code": "UNSUPPORTED_FIELD_TYPE"}`, status 400). An SDL whose fields include such a refusal keeps the code when other fields have unknown types; an SDL whose only bad fields have unknown types fails with no code. `defradb client collection add` prints Go's text alone (`Error: server returned error: <message>`): the refusal is no GraphQL error, and only `client query` prints a GraphQL error's code (`defra.cli.query.error_code`). A Go node sends no code.

# `go.collection_version.define.embeddings`: `@embedding` vector field definitions

- **Parents:** ["go.collection_version.define"]
- **Scenarios:** ["collection_version.reference-to-another-embedding-should-error", "collection_version.reference-to-self-for-embedding-should-error", "collection_version.with-float-for-embedding-should-error", "collection_version.with-float32-for-embedding-should-succeed", "collection_version.with-float64-for-embedding-should-error", "collection_version.with-int-for-embedding-should-error", "collection_version.with-invalid-embedding-generation-field-type-should-error", "collection_version.with-missing-model-for-embedding-should-error", "collection_version.with-nillable-float32-for-embedding-should-error", "collection_version.with-non-existant-field-for-embedding-should-error", "collection_version.with-string-for-embedding-should-error", "collection_version.with-unsupported-provider-for-embedding-should-error"]

`@embedding(fields: [...], provider: ..., model: ...)` is accepted only on a `[Float32!]` field: other kinds (String, Int, Float, Float64, nillable Float32 array) fail with `invalid type for vector embedding`. A missing source field fails with `the given field does not exist. Embedding generation field: about`; a JSON source with `invalid field type for vector embedding generation. Actual: JSON`; an unknown provider with `unknown embedding provider`; a missing model with `embedding Model cannot be empty`; referring to itself or another embedding with `embedding fields cannot refer to self or another embedding field. Field: name_v`.

# `go.collection_version.relations.relation_names`: Relation name uniqueness

- **Parents:** ["go.collection_version.relations"]
- **Scenarios:** ["collection_version.duplicate-relation-name-with-relation-directive", "collection_version.duplicate-relation-name-without-relation-directive", "collection_version.self-relation-primary-secondary-pair-with-same-relation-name"]

Two relation fields in one collection that infer the same relation name fail with `relation name is not unique within collection. Field: author, RelationName: book_person`; giving them distinct `@relation(name: ...)` values succeeds, and a self-relation primary/secondary pair may share one relation name.

# `go.collection_version.introspection.aggregate_inline_array`: Aggregate arguments over inline arrays

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.aggregates.inline-array-adds-users-average", "collection_version.aggregates.inline-array-adds-users-boolean-count-filter", "collection_version.aggregates.inline-array-adds-users-count", "collection_version.aggregates.inline-array-adds-users-float-count-filter", "collection_version.aggregates.inline-array-adds-users-integer-count-filter", "collection_version.aggregates.inline-array-adds-users-nillable-boolean-count-filter", "collection_version.aggregates.inline-array-adds-users-nillable-float-count-filter", "collection_version.aggregates.inline-array-adds-users-nillable-integer-count-filter", "collection_version.aggregates.inline-array-adds-users-nillable-string-count-filter", "collection_version.aggregates.inline-array-adds-users-string-count-filter", "collection_version.aggregates.inline-array-adds-users-sum"]

For inline array fields, `_count`, `_sum` and `_avg` expose arguments for the array field, and `_count` takes a filter input whose operator block matches the element kind (nillable and non-nillable Boolean, Int, Float, String).

# `go.collection_version.introspection.aggregate_fields`: Aggregate fields on collections and relations

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.aggregates.simple-adds-users-average", "collection_version.aggregates.simple-adds-users-count", "collection_version.aggregates.simple-adds-users-sum", "collection_version.aggregates.top-level-adds-average-given-collection", "collection_version.aggregates.top-level-adds-count-given-collection", "collection_version.aggregates.top-level-adds-sum-given-collection"]

Every collection type exposes `_count`, `_sum` and `_avg` with arguments naming its related many-side fields, and the top-level `Query` exposes `_count`, `_sum` and `_avg` taking the collection as argument.

# `go.collection_version.introspection.filter_inputs`: Filter input types

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.filter-for-one-to-one-collection", "collection_version.filter-for-simple-collection", "collection_version.filter-inputs-with-jsonfield-succeeds", "collection_version.filter-inputs-with-non-nillable-jsonfield-uses-jsonscalar", "collection_version.filter-inputs-with-non-nillable-string-field-uses-string-operator-block"]

Each collection gets a `<Name>FilterArg` input with `_and`, `_or`, `_not`, one operator block per field and relation fields filtered through the related type's filter input; non-nillable String fields use the string operator block and JSON fields (nillable or not) use the JSON scalar.

# `go.collection_version.introspection.group_order_inputs`: Group-by and order input types

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.group-by-field-for-the-many-side-in-collection", "collection_version.group-by-field-for-the-single-side-in-collection", "collection_version.input-type-of-order-field-where-collection-has-many-relation-type", "collection_version.input-type-of-order-field-where-collection-has-relation-type"]

`groupBy` on a collection accepts its field enum including the one side's relation ID field, and the `order` argument's input type includes relation fields from both one and many sides as nested order inputs.

# `go.collection_version.introspection.mutation_inputs`: Mutation input and field enum types

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.add-one-to-one-input-primary-object", "collection_version.add-one-to-one-input-secondary-object", "collection_version.encrypt-fields-for-add-mutation"]

For a one-to-one relation, the primary side's `<Name>MutationInputArg` includes the relation object and `_<rel>ID` fields while the secondary side's input lists its scalar fields; each collection gets a `<Name>Field` enum listing its fields (used e.g. for `encryptFields`).

# `go.collection_version.introspection.builtin_types`: Built-in schema types

- **Parents:** ["go.collection_version.introspection"]
- **Scenarios:** ["collection_version.client_introspection.basic", "collection_version.client_introspection.with-one-to-many-collection", "collection_version.introspection-explain-type-defined"]

With or without collections, introspection exposes the `ExplainType` enum with its description, and the full client introspection query succeeds, including with a one-to-many schema.

# `go.collection_version.migrations.query.restart`: Migrations survive restart

- **Parents:** ["go.collection_version.migrations.query"]
- **Scenarios:** ["collection_version.migrations.query.collection-migration-query-with-restart", "collection_version.migrations.query.collection-migration-query-with-restart-and-migration-before-patch-collection"]

Migrations configured before or after the patch are still applied to queries after the node restarts.

# `go.collection.truncate.basic`: Truncating a collection

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection.truncate.truncate-collection", "collection.truncate.truncate-collection-truncate-twice", "collection.truncate.truncate-collection-parallel-deletes-all-previously-existing-documents"]

Truncating an empty collection, or the same collection twice, succeeds; truncating while documents are added in parallel deletes every document that existed before the truncate.

# `defra.collection.truncate_isolation`: Truncating preserves other collections

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection.truncate.isolation.truncate_preserves_other_collections"]

Truncating an indexed collection removes its documents and leaves the documents, collection definitions and indexes of every other collection intact, including when legacy index prefixes overlap other datastore key families.

# `go.collection.truncate.re_add`: Re-adding documents after truncate

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection.truncate.truncate-collection-add-adds-doc-with-same-doc-id-as-original", "collection.truncate.truncate-collection-add-adds-doc-with-same-cid-as-original", "collection.truncate.truncate-collection-add-adds-doc-with-blocks-at-height1"]

A document re-added after truncate gets the same docID and the same head CID as the original, with blocks starting again at height 1.

# `go.collection.truncate.filter`: Truncating selected documents

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection.truncate.truncate-collection-with-filter-is-idempotent", "collection.truncate.truncate-collection-with-large-integer-filter-removes-only-exact-match", "collection.truncate.truncate-collection-index-filter-removes-document", "collection.truncate.truncate-collection-index-filter-with-unique-index-removes-document", "collection.truncate.truncate-collection-index-filter-with-unique-index-allows-recreation-of-document"]
- **Scope:** "the large-integer test: Go, HTTP, CLI and C clients with collection save/named mutations only"

A truncate limited to certain documents removes only those (idempotently on retry), matches large integers exactly (2^53 vs 2^53+1), works through plain and unique secondary indexes, and frees a unique value so a document with it can be created again.

# `defra.collection.truncate.filter_branchable`: A filtered truncate of a branchable collection keeps its history

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection::truncate::branchable_filter::filtered_truncate_of_a_branchable_collection_removes_only_the_matches", "collection::truncate::branchable_filter::filtered_truncate_of_a_branchable_collection_keeps_its_history", "collection::truncate::branchable_filter::a_document_truncated_from_a_branchable_collection_can_be_added_again", "collection::truncate::branchable_filter::a_peer_update_restores_a_document_truncated_from_a_branchable_collection", "collection::truncate::branchable_filter::a_fresh_peer_sync_restores_a_document_truncated_from_a_branchable_collection", "db::collection::purge::purge_keeps_a_branchable_collections_history"]
- **Scope:** "all clients except go-http, which follows `.github/testo/divergences.txt`: Go's parity pin refuses it with `filtered truncate is not supported for branchable collections`"

A filtered truncate of a `@branchable` collection is not refused. It removes the matching documents as a purge does (`defra.purge.history`): with `Users @branchable { name: String @index(unique: true) }` holding John and Jane, truncating with `{name: {_eq: "John"}}` leaves only Jane. Every block of John's history stays stored, since the collection DAG links it, and no longer names John as an owner; both collection blocks stay listed. `_commits` lists six blocks before the truncate and four after: John's field and composite blocks are no longer listed, as no document holds them. Querying `_commits(cid: <John's composite CID>)` finds the kept block, while `_commits(docID: <John>)` is empty. John can be added again with the same DocID because the unique value was freed, and `_commits(docID: <John>)` lists his field and composite blocks again.

A peer that still holds John may update him after the truncate. Pulling the article back from that peer restores John with both fields and the new age rather than leaving a partial document with a null field. A fresh peer that syncs the branchable collection also gets John back whole from that history.

# `defra.collection.truncate.other_collections`: Truncate leaves other collections intact

- **Parents:** ["go.collection.truncate"]
- **Scenarios:** ["collection::truncate::other_collections::truncate_leaves_other_collections_indexes_intact"]

Truncating a collection removes only its own documents and index entries: another collection's documents stay, and a filter on its indexed field still returns them, whatever short IDs the two collections have (with `Kept { name: String @index }` holding `{name: "Kept"}`, truncating each of 125 other indexed collections leaves `Kept(filter: {name: {_eq: "Kept"}})` returning that document, while the truncated collection's own indexed filter returns nothing).

# `defra.collection_version.removed_transform_reindex`: Removing a transform reindexes an indexed collection

- **Parents:** ["go.collection_version.migrations"]
- **Scenarios:** ["collection_version::updates::remove::removed_transform_reindex::remove_source_transform_reindexes_an_indexed_collection", "collection_version::updates::remove::removed_transform_reindex::removing_transform_preserves_old_and_new_documents", "collection_version::updates::remove::removed_transform_reindex::removing_an_inactive_versions_transform_reindexes_the_active_version"]

Removing a collection's source transform by patch (`{"op": "remove", "path": "/Users/PreviousVersion/Transform"}`) reindexes its indexed fields from the stored values, so a filter on an indexed field returns the values reads return, not values the removed transform produced: with `Users { name: String @index }` holding `{name: "Shahzad"}` and a patch whose `set-default` lens sets `name` to `"Fred"`, removing the transform makes `Users(filter: {name: {_eq: "Shahzad"}})` return that document. A document created at the active version keeps its value: with `age: Int @index`, an `increment` lens adding 5, `{name: "old", age: 20}` written before the lens and `{name: "new", age: 40}` after it, removing the transform makes a filter on `age: 20` return `old` and one on `age: 40` return `new`. The transform removed may be an inactive version's that the active version's history passes through: with the `increment` lens on the version that added `level`, and a later version adding `rank` without a lens, removing the `level` version's transform by its version ID makes a filter on `age: 20` return `old`. A value already materialized through the removed transform is retained, because its stored version alone does not tell it from a document created at the active version and a lens need not be reversible.

# `defra.collection_version.migrations.query.index.unique_clash`: A migration that gives two documents one unique value

- **Parents:** ["go.collection_version.migrations.query.index"]
- **Scenarios:** ["collection_version::migrations::query::with_unique_index::migration_giving_two_documents_one_unique_value_fails_the_index", "collection_version::migrations::query::with_unique_index::switching_back_after_a_unique_clash_rebuilds_the_index", "collection_version::migrations::query::with_unique_index::migration_keeping_unique_values_distinct_keeps_the_index"]

A migration whose lens gives two documents the same value of a unique index still commits, and the index lists as a failed build: with `Users { name: String, age: Int @index(unique: true) }` holding `{name: "Andy", age: 20}` and `{name: "John", age: 30}`, a patch whose `set-default` lens sets `age` to 5 makes the listing of `Users_age_ASC` show `Action` 3 (index backfill), `Status` 2 (errored) and the `Reason` `can not index a doc's field(s) that violates unique index. DocID: <docID>, age: 5`. Reads return the migrated values (`Users(filter: {age: {_eq: 5}})` returns both documents) by scanning (0 index fetches), and the failed index is not maintained by writes, as Go abandons a failed index, so it refuses no write: `{name: "Fred", age: 5}` and then `{name: "Gina", age: 5}` can both be created, the index still lists as failed, and `age: 5` returns all four documents. Switching the active version back to the first one while the index lists as failed rebuilds the index from the old values: it lists as completed (`Status` 3, empty `Reason`) and `Users(filter: {age: {_eq: 20}})` returns `Andy` through 1 index fetch. A migration that keeps the values distinct (an `increment` lens adding 5) leaves the index completed and used (`age: 25` returns `Andy` through 1 index fetch).

# `defra.view.unique_clash`: A lensed view whose items share a unique value

- **Parents:** ["go.view"]
- **Scenarios:** ["view::simple::with_unique_index::view_materialized_transform_giving_two_items_one_unique_value", "view::simple::with_unique_index::view_transform_giving_two_items_one_unique_value"]

A view's items are not indexed, so a unique index a view declares refuses nothing: with `User { name: String }` holding `John` and `Fred`, a view `UserView { name: String, age: Int @index(unique: true) }` over `User { name }` whose `set-default` lens sets `age` to 23 returns both items for `UserView(filter: {age: {_eq: 23}})`, whether or not the view is materialized, and `RefreshViews` succeeds. A materialized view's cache holds every field the view declares, including fields only its lens produces, so the materialized view returns `age: 23` for both items as the non-materialized one does.

# `defra.wasm.collection_objects`: The wasm client returns Go-shaped collection version objects

- **Parents:** ["go.collection_version"]
- **Scenarios:** ["collection_js_tests::a_plain_collection_is_gos_js_object", "collection_js_tests::relation_kinds_are_gos_object_kinds", "collection_js_tests::every_optional_value_is_gos_encoding", "collection_js_tests::index_kinds_are_gos_kind_descriptions", "collection_js_tests::every_vector_algorithm_keeps_its_parameters", "client::tests::test_get_collections_returns_gos_js_collection_objects"]
- **Scope:** "wasm client only; this requirement is exempt from testo because no testo client runs the published browser client, and the serializer and real JavaScript object are checked by crate tests"

The browser client's `getCollections()` returns data-only `client.CollectionVersion` objects with the JavaScript shape Go produces by `json.Marshal` on the untagged struct. This is deliberately different from Go's `getCollections()`, which returns collection handles with methods. The wasm client returns the same version-object shape Go uses in `addCollection` and `version` responses. Its keys are, in this order: `Name`, `VersionID`, `CollectionID`, `CollectionSet`, `Query`, `PreviousVersion`, `Fields`, `Indexes`, `EncryptedIndexes`, `Policy`, `IsActive`, `IsMaterialized`, `IsBranchable`, `IsEmbeddedOnly`, `IsPlaceholder`, `VectorEmbeddings`, and no others. An unset `CollectionSet`, `Query`, `PreviousVersion` or `Policy` is `null`, never missing or `undefined`. Each entry of `Fields` has the keys `FieldID`, `Name`, `Kind`, `Typ`, `RelationName`, `IsPrimary`, `DefaultValue`, `Size`, in that order. `Kind` is the scalar kind's number (`Int` is `4`, `String` is `11`, `[Float32!]` is `9`) or, for a relation, `{"Array": false, "CollectionID": "<id>"}` or `{"RelativeID": "<id>", "Array": true}`. `Typ` is the CRDT type's number (`0` none, `1` lww, `4` pncounter, `5` pcounter), not its name. `RelationName` and `DefaultValue` are `null` when unset. Integer defaults use JavaScript Number with the same rounding as parsing the Go JSON response, including integers outside the safe-integer range. Each entry of `Indexes` is `{Name, ID, Fields, Kind, KindDescription, Unique}`, with `Kind` `0` and `KindDescription` `{Unique}` for an ordered index, and `Kind` `1` and `KindDescription` `{Fields, Algorithm, Metric, Dimensions}` plus the selected algorithm's `HNSW`, `IVFPQ`, `IVFFLAT`, `SSG` or `DISKANN` block for a vector index. Unselected algorithm blocks are absent, and `Fields` there is the list of field names. `Policy` is `{ID, ResourceName}`, `CollectionSet` `{CollectionSetID, RelativeID}`, `PreviousVersion` `{SourceCollectionID, Transform}`, `Query` `{Query, Transform}`, each `Transform` `null` when unset. A wasm client is not a testo client, so the shape is checked by the crate's native unit tests over the serializer and a browser test over the JavaScript object.
