# `go.mutation`: Document mutations

- **Parents:** ["go"]

Creating, updating, upserting, deleting and truncating documents through GraphQL mutations and the collection API.

# `go.mutation.add`: Document creation

- **Parents:** ["go.mutation"]

`add_<Collection>` mutations and the collection create/save API.

# `go.mutation.add.variables`: GraphQL variables in add mutations

- **Parents:** ["go.mutation.add"]
- **Go-Tests:** ["mutation/add/with_variables_test.go::TestMutationAddWithNonNullVariable", "mutation/add/with_variables_test.go::TestMutationAddWithDefaultVariable", "mutation/add/with_variables_test.go::TestMutationAdd_WithVariableInJSONObject_Succeeds", "mutation/add/with_variables_test.go::TestMutationAdd_WithJSONVariable_Succeeds"]

`add_Users(input: $user)` accepts a `[UsersMutationInputArg!]!` variable and a variable with a default value. A variable can be used inside a JSON object literal (`{embed: {message: $message}}`) or as the whole value of a `JSON` field, and the stored JSON is returned with the substituted values.

# `go.mutation.deleted_doc`: A deleted docID cannot be re-created or updated

- **Parents:** ["go.mutation"]
- **Go-Tests:** ["mutation/add/with_delete_test.go::TestMutationAdd_GivenPreviouslyDeletedDoc_Errors", "mutation/add/with_delete_test.go::TestMutationAdd_GivenPreviouslyDeletedDoc_DocRemainsDeleted", "mutation/update/with_delete_test.go::TestUpdateSave_DeletedDoc_DoesNothing"]
- **Scope:** "TestUpdateSave_DeletedDoc_DoesNothing runs only on the Collection.Save mutation type"

Re-adding the same content after a delete, or saving an update to a deleted document, fails with `a document with the given ID has been deleted`, and the document stays deleted (a later `Users` query returns nothing).

# `go.mutation.embeddings`: `@embedding` vector fields

- **Parents:** ["go.mutation"]

Vector fields generated from other fields through an embedding provider.

# `go.mutation.embeddings.generate`: Embeddings are generated on create and update

- **Parents:** ["go.mutation.embeddings"]
- **Go-Tests:** ["mutation/add/embeddings/embedding_test.go::TestMutationAdd_WithMultipleEmbeddingFields_ShouldSucceed", "mutation/update/embeddings/embedding_test.go::TestMutationUpdate_WithMultipleEmbeddingFields_ShouldSucceed"]
- **Scope:** "Go client only; needs a local ollama 'nomic-embed-text' server"

With `name_v: [Float32!] @embedding(fields: ["name", "about"], provider: "ollama", model: "nomic-embed-text", ...)`, creating or updating a document with one or both source fields fills `name_v` with a 768-element float32 vector.

# `go.mutation.embeddings.skip`: User-supplied vectors and unrelated updates skip generation

- **Parents:** ["go.mutation.embeddings"]
- **Go-Tests:** ["mutation/add/embeddings/embedding_test.go::TestMutationAdd_UserDefinedVectorEmbeddingDoesNotTriggerGeneration_ShouldSucceed", "mutation/update/embeddings/embedding_test.go::TestMutationUpdate_UserDefinedVectorEmbeddingDoesNotTriggerGeneration_ShouldSucceed", "mutation/update/embeddings/embedding_test.go::TestMutationUpdate_FieldsForEmbeddingNotUpdatedDoesNotTriggerGeneration_ShouldSucceed"]
- **Scope:** "the two update tests run on the Go client only"

A `name_v` value given explicitly (`[1, 2, 3]`) on create or update is stored as-is instead of being generated, and an update that touches none of the embedding's source fields leaves the existing vector unchanged.

# `go.mutation.field_kinds`: Field kinds on create

- **Parents:** ["go.mutation"]

Values of each scalar and array kind written with `add_` or the collection API and read back by a query.

# `go.mutation.field_kinds.numeric`: Int and float scalars round-trip

- **Parents:** ["go.mutation.field_kinds"]
- **Go-Tests:** ["mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithInt", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNonNillableInt", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithFloat", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithFloat32", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithFloat64", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat32", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat64"]

`Int`/`Int!` store and return 42 as int64; `Float`, `Float64` (nillable and `!`) return 1.2 as float64, and `Float32`/`Float32!` return 1.2 as float32.

# `go.mutation.field_kinds.nillable_arrays`: Nillable-element arrays round-trip, including null elements

- **Parents:** ["go.mutation.field_kinds"]
- **Go-Tests:** ["mutation/add/field_kinds/field_kind_bool_test.go::TestMutationAddFieldKinds_WithNillableBoolArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_bool_test.go::TestMutationAddFieldKinds_WithNillableBoolArray_WithNilElement", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNillableIntArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNillableIntArray_WithNilElement", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNillableIntArray_FromIntSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNillableFloatArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNillableFloat64Array_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNillableFloat64Array_WithNilElement", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNillableFloat32Array_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNillableFloat32Array_WithNilElement", "mutation/add/field_kinds/field_kind_string_test.go::TestMutationAddFieldKinds_WithNillableStringArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_string_test.go::TestMutationAddFieldKinds_WithNillableStringArray_FromAnySlice", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNillableDateTimeArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNillableDateTimeArray_WithNilElement", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNillableDateTimeArray_FromStringSlice"]

`[Boolean]`, `[Int]`, `[Float]`, `[Float32]`, `[Float64]`, `[String]` and `[DateTime]` fields return the given elements in order, with a null element kept as null in its position (e.g. `[1, null, 3]`). Element values given as a typed list, a Go `[]int`, or RFC 3339 strings for DateTime are all accepted, never silently dropped.

# `go.mutation.field_kinds.non_nillable_arrays`: Non-nillable-element arrays round-trip

- **Parents:** ["go.mutation.field_kinds"]
- **Go-Tests:** ["mutation/add/field_kinds/field_kind_bool_test.go::TestMutationAddFieldKinds_WithNonNillableBoolArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_bool_test.go::TestMutationAddFieldKinds_WithNonNillableBoolArray_FromAnySlice", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNonNillableIntArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNonNillableIntArray_FromAnySlice", "mutation/add/field_kinds/field_kind_int_test.go::TestMutationAddFieldKinds_WithNonNillableIntArray_FromIntSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloatArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloatArray_FromAnySlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat64Array_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat32Array_FromTypedSlice", "mutation/add/field_kinds/field_kind_float_test.go::TestMutationAddFieldKinds_WithNonNillableFloat32Array_FromAnySlice", "mutation/add/field_kinds/field_kind_string_test.go::TestMutationAddFieldKinds_WithNonNillableStringArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_string_test.go::TestMutationAddFieldKinds_WithNonNillableStringArray_FromAnySlice", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNonNillableDateTimeArray_FromTypedSlice", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNonNillableDateTimeArray_FromAnySlice", "mutation/add/field_kinds/field_kind_date_time_test.go::TestMutationAddFieldKinds_WithNonNillableDateTimeArray_FromStringSlice"]

`[Boolean!]`, `[Int!]`, `[Float!]`, `[Float32!]`, `[Float64!]`, `[String!]` and `[DateTime!]` fields return exactly the given elements in order, whether the input was a typed list, an untyped list, a Go `[]int`, or RFC 3339 strings for DateTime.

# `go.mutation.crdt`: Counter CRDT fields under mutation

- **Parents:** ["go.mutation"]

`@crdt(type: pcounter)` and `@crdt(type: pncounter)` fields, where each update adds its value to the current total.

# `go.mutation.crdt.create`: Counters store their initial value

- **Parents:** ["go.mutation.crdt"]
- **Go-Tests:** ["mutation/add/crdt/pcounter_test.go::TestPCounterAdd_IntKindWithPositiveValue_NoError", "mutation/add/crdt/pcounter_test.go::TestPCounterAdd_Float32KindWithPositiveValue_NoError", "mutation/add/crdt/pcounter_test.go::TestPCounterAdd_Float64KindWithPositiveValue_NoError", "mutation/add/crdt/pncounter_test.go::TestPNCounterAdd_IntKindWithPositiveValue_NoError", "mutation/add/crdt/pncounter_test.go::TestPNCounterAdd_Float32KindWithPositiveValue_NoError", "mutation/add/crdt/pncounter_test.go::TestPNCounterAdd_Float64KindWithPositiveValue_NoError"]
- **Scope:** "excluded from the secondary-index multiplier (counter fields cannot be indexed, Go issue #4439)"

Creating a document with an Int, Float32 or Float64 pcounter or pncounter field set to 10 / 10.1 returns that value.

# `go.mutation.crdt.increment`: Counter updates accumulate

- **Parents:** ["go.mutation.crdt"]
- **Go-Tests:** ["mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_IntKindWithNegativeIncrement_ShouldError", "mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_IntKindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_FloatKindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_Float32KindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_Float64KindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_IntKindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_FloatKindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_Float32KindWithPositiveIncrement_ShouldIncrement", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_Float64KindWithPositiveIncrement_ShouldIncrement"]
- **Scope:** "excluded from the secondary-index multiplier (Go issue #4439)"

Starting from 0, two updates of 10 give 20, and updates of 10.1 then 10.2 give 20.299999999999997 for Float/Float64 and float32 20.3 for Float32. A pcounter update with a negative value fails with `value cannot be negative` and leaves the total unchanged.

# `go.mutation.crdt.overflow`: Counter overflow and precision limits

- **Parents:** ["go.mutation.crdt"]
- **Go-Tests:** ["mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_IntKindWithPositiveIncrementOverflow_RollsOverToMinInt64", "mutation/update/crdt/pcounter_test.go::TestPCounterUpdate_FloatKindWithPositiveIncrementOverflow_NoOp", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_IntKindWithPositiveIncrementOverflow_RollsOverToMinInt64", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_FloatKindWithPositiveIncrementOverflow_PositiveInf", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_FloatKindWithDecrementOverflow_NegativeInf", "mutation/update/crdt/pncounter_test.go::TestPNCounterUpdate_FloatKindWithPositiveIncrementInsignificantValue_DoesNothing"]
- **Scope:** "Int overflow tests: Go, CLI and HTTP clients, collection-named and Collection.Save mutation types only; float ±Inf tests: Go client only (Go issue #2569); all excluded from the secondary-index multiplier"
- **Go-Bug-Suspect:** "tests document rather than endorse the behaviour: an Int counter at MaxInt64 plus 1 silently wraps to MinInt64 (a pcounter goes negative), and a float pncounter overflows to +Inf/-Inf instead of erroring"

An Int counter at `math.MaxInt64` incremented by 1 reads `math.MinInt64`. A Float pncounter at MaxFloat64 plus MaxFloat64/10 reads +Inf, and the negative case reads -Inf. Adding 1000 to a pcounter at MaxFloat64, or 1 to a pncounter at MaxFloat64/10, leaves the value unchanged.

# `go.mutation.relation`: Linking related documents through mutations

- **Parents:** ["go.mutation"]

Setting relation fields by `_<field>ID` or by the relation field name (alias) on create and update.

# `go.mutation.relation.one_to_one_link`: Linking a one-to-one from the primary side

- **Parents:** ["go.mutation.relation"]
- **Go-Tests:** ["mutation/add/field_kinds/one_to_one/with_simple_test.go::TestMutationAddOneToOne", "mutation/add/field_kinds/one_to_one/with_alias_test.go::TestMutationAddOneToOne_UseAliasedRelationNameToLink_QueryFromPrimarySide", "mutation/add/field_kinds/one_to_one/with_null_value_test.go::TestMutationAddOneToOne_WithExplicitNullOnPrimarySide", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOne", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOne_WithGQLRequest_ReturnsResults", "mutation/update/field_kinds/one_to_one/with_self_ref_test.go::TestMutationUpdateOneToOne_SelfReferencingFromPrimary"]

Setting `_publishedID` or `published` (the `@primary` side) on create or update links the documents, and the link is visible from both sides (`Book { author { name } }` and `Author { published { name } }`), including in the `update_Author` response selection. An explicit `"author": null` on the primary side leaves the relation null; a self-referencing `boss: User @primary` / `underling: User` link resolves both ways.

# `go.mutation.relation.one_to_one_secondary`: Setting a one-to-one from the secondary side is rejected

- **Parents:** ["go.mutation.relation"]
- **Go-Tests:** ["mutation/add/field_kinds/one_to_one/with_simple_test.go::TestMutationAddOneToOneSecondarySide_CollectionApi", "mutation/add/field_kinds/one_to_one/with_simple_test.go::TestMutationAddOneToOneSecondarySide_GQL", "mutation/add/field_kinds/one_to_one/with_alias_test.go::TestMutationAddOneToOne_UseAliasedRelationNameToLink_CollectionAPI_Errors", "mutation/add/field_kinds/one_to_one/with_alias_test.go::TestMutationAddOneToOne_UseAliasedRelationNameToLink_GQL_Errors", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOneSecondarySide_CollectionApi", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOneSecondarySide_GQL", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOne_RelationIDToLinkFromSecondarySide_CollectionApi", "mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOne_RelationIDToLinkFromSecondarySide_GQL", "mutation/update/field_kinds/one_to_one/with_alias_test.go::TestMutationUpdateOneToOne_AliasRelationNameToLinkFromSecondarySide_CollectionApi", "mutation/update/field_kinds/one_to_one/with_alias_test.go::TestMutationUpdateOneToOne_AliasRelationNameToLinkFromSecondarySide_GQL"]
- **Scope:** "'_CollectionApi'/'CollectionAPI' tests run on the collection-named and Collection.Save mutation types; '_GQL' tests run on the GQL mutation type only"

Setting `author` or `_authorID` on `Book` (the non-primary side) on create or update fails with `cannot set relation from secondary side` through the collection API, and with `Argument "input" has invalid value` through GraphQL, because the GQL input type has no such field.

# `go.mutation.relation.one_to_many_single_side`: Setting a one-to-many from the one side is rejected

- **Parents:** ["go.mutation.relation"]
- **Go-Tests:** ["mutation/update/field_kinds/one_to_many/simple_test.go::TestMutationUpdateOneToMany_RelationIDToLinkFromSingleSide_Error", "mutation/update/field_kinds/one_to_many/with_alias_test.go::TestMutationUpdateOneToMany_AliasRelationNameToLinkFromSingleSide_CollectionApi", "mutation/update/field_kinds/one_to_many/with_alias_test.go::TestMutationUpdateOneToMany_AliasRelationNameToLinkFromSingleSide_GQL"]
- **Scope:** "the '_Error' and '_CollectionApi' tests run on collection mutation types, '_GQL' on the GQL mutation type only"

Updating an `Author` with `_publishedID` fails with `the given field does not exist. Name: _publishedID`; with `published` it fails with `cannot set relation from secondary side` (collection API) or `Argument "input" has invalid value` (GraphQL).

# `go.mutation.relation.invalid_id`: Malformed relation IDs are rejected

- **Parents:** ["go.mutation.relation"]
- **Go-Tests:** ["mutation/update/field_kinds/one_to_one/with_simple_test.go::TestMutationUpdateOneToOne_InvalidLengthRelationIDToLink_Error", "mutation/update/field_kinds/one_to_one/with_alias_test.go::TestMutationUpdateOneToOne_AliasWithInvalidLengthRelationIDToLink_Error", "mutation/update/field_kinds/one_to_many/simple_test.go::TestMutationUpdateOneToMany_InvalidRelationIDToLinkFromManySide", "mutation/update/field_kinds/one_to_many/with_alias_test.go::TestMutationUpdateOneToMany_InvalidAliasRelationNameToLinkFromManySide_GQL", "mutation/update/field_kinds/one_to_many/with_alias_test.go::TestMutationUpdateOneToMany_InvalidAliasRelationNameToLinkFromManySide_Collection"]
- **Go-Bug-Suspect:** "the asserted message is the Go uuid library's text, not a DefraDB error; the '_GQL' test also carries a stale 'should probably not pass' comment although it expects an error"

Updating a relation field with `bae-35953ca-518d-9e6b-9ce6cd00eff5` (a 30-character UUID part) fails with `uuid: incorrect UUID length 30 in string "35953ca-518d-9e6b-9ce6cd00eff5"`.

# `go.mutation.update`: Document updates

- **Parents:** ["go.mutation"]

`update_<Collection>` mutations, the collection update API and update-with-filter.

# `go.mutation.update.basic`: Updates change only the given fields

- **Parents:** ["go.mutation.update"]
- **Go-Tests:** ["mutation/update/underscored_collection_test.go::TestMutationUpdateUnderscoredCollection", "mutation/update/with_default_values_test.go::TestMutationUpdate_WithDefaultValues_DoesNotOverwrite", "mutation/update/field_kinds/blob_test.go::TestMutationUpdate_WithBlobField", "mutation/update/field_kinds/date_time_test.go::TestMutationUpdate_WithDateTimeField", "mutation/update/field_kinds/date_time_test.go::TestMutationUpdate_WithDateTimeField_MultipleDocs"]

An update replaces the given fields and leaves others alone: on `My_User` (underscored name) `name` becomes "Fred"; a stored `score: 0` with `@default(value: 100)` stays 0 after updating `name`; Blob and DateTime values are replaced. `update_Users(input: {created_at: ...})` with no filter updates every document.

# `go.mutation.update.by_filter`: Update by GraphQL filter

- **Parents:** ["go.mutation.update"]
- **Go-Tests:** ["mutation/update/with_filter_test.go::TestMutationUpdate_WithBooleanFilter_ResultNotFilteredOut", "mutation/update/with_filter_test.go::TestMutationUpdate_WithBooleanFilter", "mutation/update/with_filter_test.go::TestMutationUpdate_WithFilterOnUpdatedField_ReturnsResult", "mutation/update/with_filter_test.go::TestMutationUpdate_WithRelationSelectInResponse_ReturnsRelation", "mutation/update/with_filter_test.go::TestMutationUpdate_WithRelationFilter_CorrectlyFilters", "mutation/update/with_filter_test.go::TestMutationUpdate_WithRelationFilterAndRelationSelect_ReturnsBoth"]
- **Scope:** "the three relation tests run on the GQL mutation type only"

`update_Users(filter: ..., input: ...)` updates only matching documents and returns them with their new values, even when the update makes them no longer match (filter `verified: true`, input `verified: false`; filter `name: "John"`, input `name: "Jane"`). A filter on a related field (`published: {rating: {_gt: 3}}`) selects by the relation, and the response can select related fields.

# `go.mutation.update.collection_filter_errors`: Invalid update-with-filter input

- **Parents:** ["go.mutation.update"]
- **Go-Tests:** ["mutation/update/with_filter_action_test.go::TestUpdateWithInvalidFilterType_ReturnsError", "mutation/update/with_filter_action_test.go::TestUpdateWithInvalidFilterType_WithGoClient_ReturnsError", "mutation/update/with_filter_action_test.go::TestUpdateWithEmptyFilter_ReturnsError", "mutation/update/with_filter_action_test.go::TestUpdateWithInvalidJSON_ReturnsError", "mutation/update/with_filter_action_test.go::TestUpdateWithInvalidUpdater_ReturnsError", "mutation/update/with_filter_action_test.go::TestUpdateWithPatch_DoesNothing"]
- **Scope:** "the first invalid-filter-type test runs on HTTP and CLI clients only, the second on the Go client only"
- **Go-Bug-Suspect:** "an unsupported filter type over HTTP/CLI reports 'collection not found', which says nothing about the filter; a JSON-array (patch) updater is accepted and silently does nothing instead of erroring"

A filter of an unsupported type fails with `unsupported filter type` (Go client) or `collection not found` (HTTP, CLI); an empty filter string fails with `filter cannot be empty`; an updater that is not JSON fails with `cannot parse JSON: cannot parse object`; a JSON string updater fails with `the updater of a document is of invalid type`; a JSON array updater leaves the document unchanged.

# `go.mutation.upsert`: Upsert

- **Parents:** ["go.mutation"]

`upsert_<Collection>(filter:, add:, update:)`.

# `go.mutation.delete`: Document deletion

- **Parents:** ["go.mutation"]

`delete_<Collection>` mutations and the collection delete API.

# `go.mutation.truncate`: Truncate

- **Parents:** ["go.mutation"]

`truncate_<Collection>` removes documents in bulk and returns `true`.

# `go.mutation.truncate.apply`: Truncate removes all or selected documents

- **Parents:** ["go.mutation.truncate"]
- **Go-Tests:** ["mutation/truncate/simple_test.go::TestMutationTruncate", "mutation/truncate/filter_test.go::TestMutationTruncateWithFilter", "mutation/truncate/docid_test.go::TestMutationTruncateWithDocID", "mutation/truncate/docid_test.go::TestMutationTruncateWithScalarDocID", "mutation/truncate/docid_test.go::TestMutationTruncateWithMultipleDocIDs", "mutation/truncate/docid_test.go::TestMutationTruncateWithUnknownDocID", "mutation/truncate/docid_test.go::TestMutationTruncateWithDocIDAndFilter"]

`mutation { truncate_User }` returns `true` and empties the collection. With `filter:` or `docID:` (scalar or list) it removes only the matching documents; an unknown docID removes nothing; `docID` and `filter` together remove only documents matching both.

# `go.mutation.truncate.restrictions`: Truncate argument and context restrictions

- **Parents:** ["go.mutation.truncate"]
- **Go-Tests:** ["mutation/truncate/simple_test.go::TestMutationTruncateMustBeStandalone", "mutation/truncate/simple_test.go::TestMutationTruncateCannotRunInTransaction", "mutation/truncate/filter_test.go::TestMutationTruncateRejectsNullFilter", "mutation/truncate/docid_test.go::TestMutationTruncateRejectsNullDocID"]

Truncate alongside another mutation field fails with `truncate mutation must be the only field in an operation`; inside a transaction it fails with `truncate mutation cannot run in a transaction`; `filter: null` fails with `truncate filter cannot be null` and `docID: null` with `truncate docID cannot be null`, leaving documents intact.

# `go.mutation.txn`: Mutations in explicit transactions

- **Parents:** ["go.mutation"]

Requests carrying a transaction ID, committed or discarded later.

# `go.mutation.txn.conflict`: Concurrent transactions and commit conflicts

- **Parents:** ["go.mutation.txn"]
- **Go-Tests:** ["mutation/mix/with_txn_test.go::TestMutationWithTxnDoesNotAllowUpdateInSecondTransactionUser", "mutation/update/txn_test.go::TestMutationUpdate_InterleavedTxn", "mutation/add/field_kinds/one_to_one_to_one/with_txn_test.go::TestTransactionalCreationAndLinkingOfRelationalDocumentsForward", "mutation/add/field_kinds/one_to_one_to_one/with_txn_test.go::TestTransactionalCreationAndLinkingOfRelationalDocumentsBackward"]
- **Scope:** "Badger file, Badger in-memory and Defra in-memory databases only (Go issue #4442)"
- **Go-Bug-Suspect:** "in the relational creation tests two transactions each add a different book linked to a different publisher, yet the second commit must fail with 'transaction conflict' only because each txn ran a read query over the other's publisher (SSI anti-dependency); defradb.rs validates query snapshots never, so it is expected to commit both"

Two transactions updating the same document: the first commit wins and the second fails with `transaction conflict. Please retry`; the stored value is the first's. Two transactions updating different documents both commit (regression for Go issue #5257). Two transactions that create and link books to different publishers and then query publishers: Go fails the second commit with `transaction conflict` and only the first book is stored.

# `go.net`: P2P networking and replication

- **Parents:** ["go"]

Peer connections, pubsub subscriptions, replicators, on-demand sync, and the CRDT merge behaviour seen across nodes.

# `go.net.peers`: Peer connection management

- **Parents:** ["go.net"]

Connecting, listing and disconnecting peers.

# `go.net.peers.connect`: Connecting peers and listing active peers

- **Parents:** ["go.net.peers"]
- **Go-Tests:** ["net/info/connect_peers_test.go::TestNetInfoPeers_NoP2PConfigured", "net/info/connect_peers_test.go::TestNetInfoPeers", "net/info/connect_peers_test.go::TestNetInfoConnectPeers_SourceP2PDisabled", "net/info/connect_peers_test.go::TestNetInfoConnectPeers", "net/info/connect_peers_test.go::TestNetInfoConnectMultiplePeers"]
- **Scope:** "active-peers tests exclude the JS client (no 'ActivePeers'); the P2P-disabled connect test runs only against v1.1.0 and later"

Listing active peers on a node without P2P, or connecting from one, fails with `no p2p system configured`. A fresh node lists no peers; after nodes 1 and 2 connect to node 0, node 0 lists both addresses, and nodes 1 and 2 also list each other because node 0 joined them into one network.

# `go.net.peer_events`: Peer join and leave events per pubsub topic

- **Parents:** ["go.net"]

Events a node emits when peers join or leave the topics it is on.

# `go.net.peer_events.doc_sync`: Connecting joins the `doc-sync` topic

- **Parents:** ["go.net.peer_events"]
- **Go-Tests:** ["net/peer_events/simple_test.go::TestPeerEvents_OnConnect_ShouldReceiveJoinEventOnDocSyncTopic", "net/peer_events/simple_test.go::TestPeerEvents_OnConnectMultiplePeers_ShouldReceiveAllJoinEvents", "net/peer_events/simple_test.go::TestPeerEvents_OnConnectBidirectional_BothNodesShouldReceiveJoinEvents"]

When peers connect, each node receives a join event naming the other peer on the `doc-sync` topic; with two peers connected to node 0 it receives both, and both sides of a connection receive one.

# `go.net.peer_events.collection`: Collection topic join and leave events

- **Parents:** ["go.net.peer_events"]
- **Go-Tests:** ["net/peer_events/collection_test.go::TestPeerEvents_OnSubscribeToCollection_ShouldReceiveJoinEventOnCollectionTopic", "net/peer_events/collection_test.go::TestPeerEvents_OnSubscribeToMultipleCollections_ShouldReceiveJoinEventsOnAllTopics", "net/peer_events/collection_test.go::TestPeerEvents_MultipleNodesSubscribedToCollection_ShouldReceiveAllJoinEvents", "net/peer_events/collection_test.go::TestPeerEvents_OnUnsubscribeFromCollection_ShouldReceiveLeftEvent", "net/peer_events/collection_test.go::TestPeerEvents_OnUnsubscribeFromMultipleCollections_ShouldReceiveLeftEvents"]

When connected peers subscribe to the same collection(s), node 0 receives a join event per peer on each collection topic; when a peer removes its subscription, node 0 receives a left event (`PeerEventTypeLeft`) on each removed collection topic.

# `go.net.peer_events.document`: Document topic join and leave events

- **Parents:** ["go.net.peer_events"]
- **Go-Tests:** ["net/peer_events/document_test.go::TestPeerEvents_OnSubscribeToDocument_ShouldReceiveJoinEventOnDocumentTopic", "net/peer_events/document_test.go::TestPeerEvents_OnSubscribeToMultipleDocuments_ShouldReceiveJoinEventsOnAllTopics", "net/peer_events/document_test.go::TestPeerEvents_DocumentAndDocSyncTopics_ShouldReceiveJoinEventsOnBoth", "net/peer_events/document_test.go::TestPeerEvents_AllTopicTypes_ShouldReceiveJoinEventsOnAll", "net/peer_events/document_test.go::TestPeerEvents_OnUnsubscribeFromDocument_ShouldReceiveLeftEvent", "net/peer_events/document_test.go::TestPeerEvents_OnUnsubscribeFromMultipleDocuments_ShouldReceiveLeftEvents"]

When connected peers subscribe to the same document(s), node 0 receives a join event on each document topic, alongside the `doc-sync` and collection topic joins when those apply; unsubscribing a document produces a left event on that document topic.

# `go.net.subscribe`: P2P collection and document subscriptions

- **Parents:** ["go.net"]

Adding, listing and removing the collections and documents a node follows over pubsub.

# `go.net.subscribe.collection_sync`: Collection subscriptions control which new documents arrive

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddSingle", "net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddMultiple", "net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddNone", "net/simple/peer/subscribe/collection/with_add_remove_test.go::TestP2PCollectionAddAndRemoveSingle", "net/simple/peer/subscribe/collection/with_add_remove_test.go::TestP2PCollectionAddAndRemoveMultiple", "net/simple/peer/subscribe/collection/with_add_remove_test.go::TestP2PCollectionAddSingleAndRemoveNone"]

A node subscribed to a collection receives documents created on its peer in that collection; documents in unsubscribed collections, or in a collection whose subscription was removed, are not received, and documents created on the subscriber are not pushed back. Adding or removing an empty list changes nothing.

# `go.net.subscribe.collection_errors`: Invalid collection subscriptions are atomic

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddSingleErroneousCollectionID", "net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddValidAndErroneousCollectionID", "net/simple/peer/subscribe/collection/with_add_test.go::TestP2PCollectionAddValidThenErroneousCollectionID", "net/simple/peer/subscribe/collection/with_add_remove_test.go::TestP2PCollectionAddSingleAndRemoveErroneous"]

Adding or removing an unknown collection ID fails with `collection not found`. A call mixing a valid and an unknown ID adds nothing (no documents sync); a failed call leaves earlier subscriptions working.

# `go.net.subscribe.collection_inactive`: Subscribing to a synced but inactive collection

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/peer_events/collection_inactive_test.go::TestPeerEvents_SubscribeToCollectionWithNoActiveVersion_ShouldSucceed", "net/peer_events/collection_inactive_test.go::TestPeerEvents_DeleteSubscriptionOnCollectionWithNoActiveVersion_ShouldSucceed", "net/peer_events/collection_inactive_test.go::TestPeerEvents_ListCollectionsWithNoActiveVersion_ShouldSucceed"]

On a node that received a collection version through `SyncCollectionVersions` but has no active version of it, adding, removing and listing a P2P subscription to that collection succeed, and the list returns it.

# `go.net.subscribe.document_list`: Listing document subscriptions

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/simple/peer/subscribe/document/with_get_test.go::TestP2PDocument_GetAllWithNoneConfigured_ShouldSucceed", "net/simple/peer/subscribe/document/with_add_get_test.go::TestP2PDocumentAddGetSingle", "net/simple/peer/subscribe/document/with_add_get_test.go::TestP2PDocumentAddGetMultiple", "net/simple/peer/subscribe/document/with_add_get_remove_test.go::TestP2PDocumentAddRemoveGetSingle", "net/simple/peer/subscribe/document/with_add_get_remove_test.go::TestP2PDocumentAddRemoveGetMultiple"]

Listing P2P documents returns nothing until some are added, then exactly the added docIDs; removing one leaves only the rest.

# `go.net.subscribe.document_sync`: Document subscriptions control which updates arrive

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/simple/peer/subscribe/document/with_add_test.go::TestP2PDocument_AddSingle_ShouldSync", "net/simple/peer/subscribe/document/with_add_remove_test.go::TestP2PDocumentAddAndRemoveSingle", "net/simple/peer/subscribe/document/with_add_remove_test.go::TestP2PDocumentAddAndRemoveMultiple", "net/simple/peer/subscribe/document/with_add_remove_test.go::TestP2PDocumentAddSingleAndRemoveNone"]

A node subscribed to a docID receives updates made to it on a peer (and so the document itself, even if it never had it), but not other documents; after removing the subscription, updates to that doc no longer arrive while other subscribed docs still sync. Removing an empty list changes nothing.

# `go.net.subscribe.document_errors`: Invalid document subscriptions

- **Parents:** ["go.net.subscribe"]
- **Go-Tests:** ["net/simple/peer/subscribe/document/with_add_test.go::TestP2PDocument_AddSingleErroneousDocID_ShouldNotSync", "net/simple/peer/subscribe/document/with_add_remove_test.go::TestP2PDocumentAddSingleAndRemoveErroneous"]

Adding or removing a malformed docID fails with `malformed document ID, missing either version or cid`; a failed add syncs nothing, and a failed remove leaves the existing subscription working.

# `go.net.peer`: Pubsub peer sync

- **Parents:** ["go.net"]

Documents and updates propagated between connected peers by pubsub.

# `go.net.peer.isolation`: Unsubscribed or unconnected nodes get nothing

- **Parents:** ["go.net.peer"]
- **Go-Tests:** ["net/simple/peer/with_add_test.go::TestP2PAddDoesNotSync", "net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentSingleUpdateDoesNotSyncToNonPeerNode", "net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentSingleUpdateDoesNotSyncFromUnmappedNode"]

Merely connected peers do not receive each other's new documents. An update on a node is not received by a third node that is not connected, and an update made on an unconnected node does not reach the others.

# `go.net.peer.collection_topic`: New documents propagate to collection subscribers

- **Parents:** ["go.net.peer"]
- **Go-Tests:** ["net/simple/peer/with_add_test.go::TestP2PAddWithP2PCollection", "net/simple/peer/with_add_test.go::TestP2PAdd_WithP2PCollectionWithNodeChain_ShouldSucceed", "net/simple/peer/with_add_test.go::TestP2PAdd_WithP2PCollectionOnLastNodeInNodeChain_ShouldPropagateUpdate", "net/simple/peer/with_add_test.go::TestP2PAdd_WithP2PCollectionAndSubscription_ShouldSucceed", "net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentSingleUpdateFromChildWithP2PCollection", "net/simple/peer/with_update_test.go::TestP2PWithMultipleDocumentUpdatesPerNodeWithP2PCollection"]

Documents created on node 0 reach a node subscribed to the collection, and documents created on the subscriber are not pushed back. In a five-node chain where nodes subscribe, a document created on node 0 reaches all of them, also when only the last node subscribes; a GraphQL subscription on the receiving node fires for the synced document. Later updates to synced docs propagate, converging to one of the concurrent final values (47 or 62).

# `go.net.peer.doc_updates`: Document updates propagate to document subscribers

- **Parents:** ["go.net.peer"]
- **Go-Tests:** ["net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentSingleUpdateFromChild", "net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentSingleUpdateFromParent", "net/simple/peer/with_update_test.go::TestP2PWithSingleDocumentUpdatePerNode", "net/simple/peer/with_update_test.go::TestP2PWithMultipleDocumentUpdatesPerNode", "net/simple/peer/with_update_restart_test.go::TestP2PWithSingleDocumentSingleUpdateFromChildAndRestart"]

With a doc present on both nodes and a doc subscription on the receiving node, an update in either direction (`Age` 21 to 60) arrives on the other node, also after both nodes restart. With concurrent updates on both nodes, both converge to the same value, one of the two last writes.

# `go.net.peer.deletes`: Deletes propagate to document subscribers

- **Parents:** ["go.net.peer"]
- **Go-Tests:** ["net/simple/peer/with_delete_test.go::TestP2PWithSingleDocumentConcurrentDeleteAndUpdate", "net/simple/peer/with_delete_test.go::TestP2PWithMultipleDocumentsSingleDelete", "net/simple/peer/with_delete_test.go::TestP2PWithMultipleDocumentsSingleDeleteWithShowDeleted", "net/simple/peer/with_delete_test.go::TestP2PWithMultipleDocumentsWithSingleUpdateBeforeConnectSingleDeleteWithShowDeleted", "net/simple/peer/with_delete_test.go::TestP2PWithMultipleDocumentsWithMultipleUpdatesBeforeConnectSingleDeleteWithShowDeleted"]

Deleting a subscribed doc on one node deletes it on the peer: a normal query returns only the other doc, and `Users(showDeleted: true)` shows it with `_deleted: true` and its last values, including updates made before the nodes connected. A delete concurrent with an update ends deleted on both nodes, with `Name` either value.

# `go.net.peer.no_head_exchange`: Pre-connection history is not exchanged

- **Parents:** ["go.net.peer"]
- **Go-Tests:** ["net/simple/peer/with_delete_test.go::TestP2PWithMultipleDocumentsWithUpdateAndDeleteBeforeConnectSingleDeleteWithShowDeleted"]
- **Go-Bug-Suspect:** "the test comments 'The target node currently won't receive the pre-connection updates from the source. We should look into adding a head exchange mechanic on connect.' The two nodes stay diverged"

When node 0 updates and deletes a doc before connecting, and node 1 updates it after connecting, node 0 shows the doc deleted at age 62 while node 1 shows it live at age 66.

# `go.net.peer_replicator`: Peer sync and replicator together

- **Parents:** ["go.net"]
- **Go-Tests:** ["net/simple/peer_replicator/with_add_test.go::TestP2PPeerReplicatorWithAdd", "net/simple/peer_replicator/with_update_test.go::TestP2PPeerReplicatorWithUpdate", "net/simple/peer_replicator/with_update_restart_test.go::TestP2PPeerReplicatorWithUpdateAndRestart", "net/simple/peer_replicator/with_delete_test.go::TestP2PPeerReplicatorWithDeleteShowDeleted"]

With node 0 peered to node 1 and replicating to node 2, a new doc on node 0 reaches node 2 but not node 1; an update or delete on node 0 reaches both node 1 (doc subscription) and node 2 (replicator), also across a restart.

# `go.net.replicator`: Replicators push documents to target nodes

- **Parents:** ["go.net"]

`AddReplicator` from a source node to a target node.

# `go.net.replicator.push`: New and existing documents are pushed to targets

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/with_add_test.go::TestP2POneToOneReplicator", "net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorDoesNotSyncExisting", "net/simple/replicator/with_add_test.go::TestP2POneToManyReplicator", "net/simple/replicator/with_add_test.go::TestP2POneToOneOfManyReplicator", "net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorManyDocs", "net/simple/replicator/with_add_test.go::TestP2POneToManyReplicatorManyDocs", "net/simple/replicator/with_add_restart_test.go::TestP2POneToOneReplicatorWithRestart", "net/simple/replicator/with_backfill_test.go::TestP2POneToOneReplicator_BackfillSyncsAllExistingDocs"]
- **Scope:** "the backfill test is excluded from cross-version multipliers"

Docs created on the source after `AddReplicator` reach every target (one or two), not an unconfigured third node, also after a restart. Docs created before the replicator was added are pushed when it is configured (despite the test name `DoesNotSyncExisting`), including a 200-doc backfill (`COUNT(Users: {})` is 200 on the target).

# `go.net.replicator.direction`: Replication is one-way and stops when removed

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorDoesNotSyncFromTargetToSource", "net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorDoesNotSyncFromDeletedReplicator", "net/simple/replicator/with_add_update_test.go::TestP2POneToOneReplicatorDoesNotUpdateDocExistingOnlyOnTarget"]

Docs created or updated only on the target are not pushed to the source, and after `DeleteReplicator` new source docs no longer reach the target.

# `go.net.replicator.updates`: Updates and deletes reach targets

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/with_add_update_test.go::TestP2POneToOneReplicatorWithAddWithUpdate", "net/simple/replicator/with_add_update_test.go::TestP2POneToOneReplicatorWithAddWithUpdateOnRecipientNode", "net/simple/replicator/with_update_test.go::TestP2POneToOneReplicatorUpdatesDocAddedBeforeReplicatorConfig", "net/simple/replicator/with_update_test.go::TestP2POneToOneReplicatorUpdatesDocAddedBeforeReplicatorConfigWithNodesInversed", "net/simple/replicator/with_delete_test.go::TestP2POneToOneReplicatorDeletesDocAddedBeforeReplicatorConfig", "net/simple/replicator/with_delete_test.go::TestP2POneToOneReplicatorDeletesDocAddedBeforeReplicatorConfigWithNodesInversed"]

Updates and deletes on the source reach the target, including for docs created before the replicator was added (either node as source); deleted docs show `_deleted: true` with their values under `showDeleted: true`. An update made on the target reaches the source when the source holds a doc subscription for it.

# `go.net.replicator.offline_target`: A temporarily offline target catches up

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/with_add_test.go::TestP2POneToOneReplicator_ManyDocsWithTargetNodeTemporarilyOffline_ShouldSucceed", "net/simple/replicator/with_update_test.go::TestP2POneToOneReplicator_ManyDocsUpdateWithTargetNodeTemporarilyOffline_ShouldSucceed", "net/simple/replicator/with_update_test.go::TestP2POneToOneReplicator_ManyDocsUpdateWithTargetNodeTemporarilyOfflineAfterAdd_ShouldSucceed", "net/simple/replicator/with_update_add_field_test.go::TestP2PReplicatorUpdateWithNewFieldWithTargetNodeTemporarilyOffline_SyncsUpdate"]
- **Scope:** "Badger file databases only (needs a stop/start without data loss); the new-field test excludes the cross-version old-source multiplier"

Creates and updates made on the source while the target is closed reach it once it starts again, with the final values. An update whose push failed is retried from the doc's heads even after the source patched the collection to a new version.

# `go.net.replicator.determinism`: Replicated docs keep their IDs and shared blocks

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorOrderIndependent", "net/simple/replicator/with_add_test.go::TestP2POneToOneReplicatorOrderIndependentDirectAdd", "net/simple/replicator/with_shared_field_test.go::TestP2PReplicator_SharedFieldValue_SyncsForBothDocuments"]

Two nodes declaring the same collection with fields in a different order get the same collection version ID, and the same doc has the same `_docID` and `_version { collectionVersionId }` whether replicated or created on each node. Two docs sharing a field value (`Name: "Shared"`) both return it on the target after replication.

# `go.net.replicator.cross_version`: Replication between the current build and v1.0.0

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/cross_version_test.go::TestP2PCrossVersion_HeadToV1_DocSyncs", "net/simple/replicator/cross_version_test.go::TestP2PCrossVersion_V1ToHead_DocSyncs"]
- **Scope:** "skipped when no v1.0.0 release asset exists for the platform"

A doc created on the current build replicates to a v1.0.0 node, and one created on v1.0.0 replicates to the current build.

# `go.net.replicator.branchable`: Replicating a branchable collection's commit

- **Parents:** ["go.net.replicator"]
- **Go-Tests:** ["net/simple/replicator/branchable_collection_test.go::TestP2POneToOneReplicator_BranchableCollection", "net/simple/replicator/branchable_collection_patch_test.go::TestP2POneToOneReplicator_BranchableCollection_WithPatch"]
- **Scope:** "Badger file databases only"

For an `@branchable` collection, a doc created while the target is offline arrives once it restarts, and the target's `_commits(filter: {fieldName: {_eq: null}})` holds the collection-level commit (`docID` and `fieldName` null) with the source's collection version ID, also after a patch added a field.

# `go.net.relations`: Related documents across nodes

- **Parents:** ["go.net"]
- **Go-Tests:** ["net/one_to_many/replicator/with_add_test.go::TestP2POneToManyReplicator", "net/one_to_many/peer/with_add_update_test.go::TestP2POneToManyPeerWithAddUpdateLinkingSyncedDocToUnsyncedDoc"]

A replicated `Author` and its `Book` (linked by `_AuthorID`) arrive linked on the target. A peer-synced update that points a book at an author the receiver does not have syncs without error; the receiver returns the `_AuthorID` but `Author: null`.

# `go.net.schema_drift`: Sync across different collection versions

- **Parents:** ["go.net"]

Nodes holding different versions of a collection after `PatchCollection` adds a field.

# `go.net.schema_drift.sync`: Docs sync between older and newer collection versions

- **Parents:** ["go.net.schema_drift"]
- **Go-Tests:** ["net/simple/peer/with_add_add_field_test.go::TestP2PPeerAddWithNewFieldSyncsDocsToOlderCollectionVersion", "net/simple/peer/with_add_add_field_test.go::TestP2PPeerAddWithNewFieldSyncsDocsToNewerCollectionVersion", "net/simple/peer/with_add_add_field_test.go::TestP2PPeerAddWithNewFieldSyncsDocsToUpdatedCollectionVersion", "net/simple/peer/with_update_add_field_test.go::TestP2PPeerUpdateWithNewFieldSyncsDocsToOlderCollectionVersionMultistep", "net/simple/peer/with_update_add_field_test.go::TestP2PPeerUpdateWithNewFieldSyncsDocsToOlderCollectionVersion", "net/simple/replicator/with_add_add_field_test.go::TestP2POneToOneReplicatorAddWithNewFieldSyncsDocsToOlderCollectionVersion", "net/simple/replicator/with_add_add_field_test.go::TestP2POneToOneReplicatorAddWithNewFieldSyncsDocsToNewerCollectionVersion", "net/simple/replicator/with_add_add_field_test.go::TestP2POneToOneReplicatorAddWithNewFieldSyncsDocsToUpdatedCollectionVersion", "net/simple/replicator/with_update_add_field_test.go::TestP2PReplicatorUpdateWithNewFieldSyncsDocsToOlderCollectionVersionMultistep", "net/simple/replicator/with_update_add_field_test.go::TestP2PReplicatorUpdateWithNewFieldSyncsDocsToOlderCollectionVersion"]

Over peer sync and replicators, a doc created or updated on a node with an extra `Email` field still syncs to a node without it (which returns the other fields), a doc from the older version syncs to a node with `Email`, and when both have `Email` its value syncs. An update setting only the new field does not stop a later update to `Name` from arriving.

# `go.net.schema_drift.late_patch`: A field added after sync does not surface synced values

- **Parents:** ["go.net.schema_drift"]
- **Go-Tests:** ["net/simple/peer/with_add_add_field_test.go::TestP2PPeerAddWithNewFieldDocSyncedBeforeReceivingNodeSchemaUpdatedDoesNotReturnNewField"]
- **Go-Bug-Suspect:** "documents unwanted behaviour until Go issue #2255 is fixed; the test comment says 'The email should be returned but it is not'"

When a doc with `Email` syncs to a node that only later patches in `Email`, that node returns `Email: null` instead of the synced value.

# `go.net.crdt`: CRDT convergence across nodes

- **Parents:** ["go.net"]

Merging concurrent field updates from several nodes.

# `go.net.crdt.lww`: LWW fields converge

- **Parents:** ["go.net.crdt"]
- **Go-Tests:** ["net/simple/peer/crdt/lww_test.go::TestP2PUpdate_WithLWWConcurrentDifferentFields_BothFieldsPreserved", "net/simple/peer/crdt/lww_test.go::TestP2PUpdate_WithLWWConcurrentSameField_ConvergesToSameValue"]

Across three fully subscribed nodes, concurrent updates to different fields (`name`, `age`, `score`) all survive the merge; concurrent updates to the same field converge to one identical value on every node (the test expects 60).

# `go.net.crdt.counters_peer`: Counters merge by summing across peers

- **Parents:** ["go.net.crdt"]
- **Go-Tests:** ["net/simple/peer/crdt/pcounter_test.go::TestP2PUpdate_WithPCounter_NoError", "net/simple/peer/crdt/pcounter_test.go::TestP2PUpdate_WithPCounterThreeNodeSimultaneousUpdate_NoError", "net/simple/peer/crdt/pcounter_test.go::TestP2PUpdate_WithPCounterSimultaneousUpdate_NoError", "net/simple/peer/crdt/pncounter_test.go::TestP2PUpdate_WithPNCounter_NoError", "net/simple/peer/crdt/pncounter_test.go::TestP2PUpdate_WithPNCounterThreeNodeSimultaneousUpdate_NoError", "net/simple/peer/crdt/pncounter_test.go::TestP2PUpdate_WithPNCounterSimultaneousUpdate_NoError", "net/simple/peer/crdt/pncounter_convergence_test.go::TestP2PUpdate_WithPNCounterRepeatedSimultaneousUpdates_Converges", "net/simple/peer/crdt/pncounter_convergence_test.go::TestP2PUpdate_WithPNCounterFiveNodesRepeatedUpdates_AllConverge"]
- **Scope:** "excluded from the secondary-index and signed-docs multipliers; the convergence tests also exclude cross-version multipliers (v1.0.0 still reports an inflated total)"

Increments made on different nodes to a pcounter or pncounter sum on every node: 10 + 10 = 20; three nodes adding 10, 20, 30 give 60; a pncounter at 100 with 10, -30, 50 gives 130; two nodes each adding 45 give 90. Twenty interleaved increments of 1 on each of two nodes give 40 on both, and ten on each of five fully connected nodes give 50 on all.

# `go.net.crdt.counters_replicator`: Counters through replicators

- **Parents:** ["go.net.crdt"]
- **Go-Tests:** ["net/simple/peer_replicator/crdt/pcounter_test.go::TestP2PPeerReplicatorWithAdd_PCounter_NoError", "net/simple/peer_replicator/crdt/pcounter_test.go::TestP2PPeerReplicatorWithUpdate_PCounter_NoError", "net/simple/peer_replicator/crdt/pncounter_test.go::TestP2PPeerReplicatorWithAdd_PNCounter_NoError", "net/simple/peer_replicator/crdt/pncounter_test.go::TestP2PPeerReplicatorWithUpdate_PNCounter_NoError", "net/simple/replicator/crdt/pcounter_test.go::TestP2POneToOneReplicatorUpdate_PCounter_NoError", "net/simple/replicator/crdt/pncounter_test.go::TestP2POneToOneReplicatorUpdate_PNCounter_NoError"]
- **Scope:** "excluded from the secondary-index multiplier (Go issue #4439)"

A counter doc created on the source reaches the replicator target with its value (3000); an increment of 10 on a counter at 10 reads 20 on the source, the replicator target and the doc-subscribed peer.

# `go.net.sync`: On-demand sync

- **Parents:** ["go.net"]

Explicit requests to fetch documents, branchable collections or collection versions from peers.

# `go.net.sync.documents`: `SyncDocuments` fetches docs from peers

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/documents_test.go::TestDocSync_WithDocsAvailableOnSingleNode_ShouldSync", "net/sync/documents_test.go::TestDocSync_WithDocsAvailableOnMultipleNode_ShouldSync", "net/sync/documents_test.go::TestDocSync_WithSingleDocAvailableOnMultipleNode_ShouldSync", "net/sync/documents_test.go::TestDocSync_WithDifferentVersionsOnPeers_ShouldSyncLatest", "net/sync/documents_test.go::TestDocSync_AfterSync_ShouldNotSubscribeToDocUpdates", "net/sync/documents_test.go::TestDocSync_ConcurrentSameDocRequests_AllShouldSucceed"]
- **Scope:** "the concurrent-requests test excludes the cross-version new-source multiplier"

Syncing docIDs fetches them from whichever connected peers have them (one peer, several, or the same doc on several), and with diverged versions across peers the node ends with the merged latest value (`Age` 27). Sync does not subscribe: a later update on the source is not received. Four concurrent sync calls for the same docID all succeed.

# `go.net.sync.branchable_copy`: Branchable collection sync copies every document

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/branchable_collection/simple_test.go::TestBranchableCollectionSync_OneNodeEmptyAnotherWithDocs_ShouldCopyAll", "net/sync/branchable_collection/simple_test.go::TestBranchableCollectionSync_WithDifferentDocsOnBothNodes_ShouldSync", "net/sync/branchable_collection/simple_test.go::TestBranchableCollectionSync_ShouldNotSubscribe", "net/sync/branchable_collection/complex_network_test.go::TestBranchableCollectionSync_WithMultipleDocsInComplexLinkedNetwork_ShouldSyncAll", "net/sync/branchable_collection/complex_network_test.go::TestBranchableCollectionSync_WithMultipleDocumentHeadsReceivedFromPeers_ShouldSyncAll", "net/sync/branchable_collection/versions_test.go::TestBranchableCollectionSync_WithBranchedVersionsAndDocs_ShouldSync"]
- **Scope:** "the branched-versions test excludes the encrypted-docs (Go issue #4789) and signed-docs multipliers"

`SyncBranchableCollection` on an `@branchable` collection fetches every doc known to reachable peers, through multi-hop topologies, so both nodes end with the union. It does not subscribe: docs created later arrive only on the next sync. With each node having patched a different new field (`email`, `score`), each node returns all five docs, with values only for its own field.

# `go.net.sync.branchable_dag`: Branchable sync yields identical collection DAGs

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/branchable_collection/complex_network_test.go::TestBranchableCollectionSync_WithDocumentsFromPeers_ShouldHaveIdenticalDAG", "net/sync/branchable_collection/complex_network_test.go::TestBranchableCollectionSync_WithDocumentsFromPeersAndNewHeadAfterSync_ShouldHaveIdenticalDAG"]
- **Scope:** "the new-head test excludes the signed-docs multiplier"

After every node syncs, `_commits(filter: {fieldName: {_eq: null}})` returns the same collection commit CIDs on all nodes. A doc created on every node afterwards gets a height-2 collection commit whose `heads` link all 4 previous heads.

# `go.net.sync.branchable_errors`: Branchable sync on an invalid collection

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/branchable_collection/simple_test.go::TestBranchableCollectionSync_WithNonBranchableCollection_ShouldError", "net/sync/branchable_collection/simple_test.go::TestBranchableCollectionSync_WithNonExistentCollection_ShouldError"]
- **Go-Bug-Suspect:** "the non-existent collection case asserts 'index out of range', a Go runtime/test-harness message rather than a proper not-found error"

Syncing a collection without `@branchable` fails with `collection is not branchable`; a non-existent collection fails (Go reports `index out of range`).

# `go.net.sync.branchable_deadline`: Branchable sync with and without a context deadline

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/branchable_collection/timeout_test.go::TestBranchableCollectionSync_NoDeadlineContext_ShouldSucceed", "net/sync/branchable_collection/timeout_test.go::TestBranchableCollectionSync_FiniteDeadlineContext_ShouldSucceed"]
- **Scope:** "C client only (tests deadline serialization across the C bindings)"

A branchable collection sync called with no deadline, or with a finite one, completes and the doc arrives.

# `go.net.sync.collection_version`: Syncing collection versions

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/collection_version/simple_test.go::TestSyncColVersion_WithInitialColVersion", "net/sync/collection_version/simple_test.go::TestSyncColVersion_WithInitialColVersion_CanBeActivatedAndQueried", "net/sync/collection_version/patch_test.go::TestSyncColVersion_WithPatchVersionOfUnknownCollection", "net/sync/collection_version/patch_test.go::TestSyncColVersion_WithPatchVersionOfKnownCollection"]

`SyncCollectionVersions` with a version ID fetches it from a peer, and it arrives inactive (listed only with `SetGetInactive(true)`). Syncing a patched version also fetches its unknown predecessor (both inactive, with `PreviousVersion` set); a locally active version stays active. Once activated with `SetActiveCollectionVersion`, the collection can receive docs and be queried.

# `go.net.sync.collection_version_view`: Syncing view versions

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/collection_version/view_test.go::TestSyncColVersion_WithView", "net/sync/collection_version/view_test.go::TestSyncColVersion_WithView_CanBeActivatedAndQueried"]

A synced view (`UserView` with a lens transform) arrives inactive and non-materialized; once activated, `UserView { fullName }` returns values computed by the synced transform.

# `go.net.sync.collection_version_kinds`: Synced versions keep non-nillable field kinds

- **Parents:** ["go.net.sync"]
- **Go-Tests:** ["net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableBool", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableInt", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableFloat64", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableFloat32", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableString", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableDateTime", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableBlob", "net/sync/collection_version/with_nonnillable_scalar_fields_test.go::TestSyncColVersion_WithNonNillableJSON", "net/sync/collection_version/with_nonnillable_array_fields_test.go::TestSyncColVersion_WithNonNillableBoolArray", "net/sync/collection_version/with_nonnillable_array_fields_test.go::TestSyncColVersion_WithNonNillableIntArray", "net/sync/collection_version/with_nonnillable_array_fields_test.go::TestSyncColVersion_WithNonNillableFloat64Array", "net/sync/collection_version/with_nonnillable_array_fields_test.go::TestSyncColVersion_WithNonNillableFloat32Array", "net/sync/collection_version/with_nonnillable_array_fields_test.go::TestSyncColVersion_WithNonNillableStringArray"]

A synced collection version with a `Boolean!`, `Int!`, `Float64!`, `Float32!`, `String!`, `DateTime!`, `Blob!` or `JSON!` field, or a `[Boolean!]`, `[Int!]`, `[Float64!]`, `[Float32!]` or `[String!]` field, keeps the same non-nillable kind (e.g. `FieldKind_BOOL`, not the nillable kind) with an LWW register on the receiving node.
