# `go.collection_version`: Collection definitions, versions, patches and migrations

- **Parents:** ["go"]

Declaring collections in SDL, the GraphQL schema generated from them, reading their versions, patching them into new versions, and Lens migrations between versions.

# `go.collection_version.define`: Adding collections from SDL

- **Parents:** ["go.collection_version"]

`AddCollection` (`client collection add` / `POST /collections`) with an SDL type creates a collection version and the matching GraphQL types.

# `go.collection_version.define.basic_types`: SDL types become queryable GraphQL types

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/simple_test.go::TestColVersionSimpleAddsColGivenEmptyType", "collection_version/simple_test.go::TestCollectionVersionSimpleAddsCollectionGivenNewTypes", "collection_version/simple_test.go::TestCollectionVersionSimpleAddsCollectionWithDefaultFieldsGivenEmptyType", "collection_version/simple_test.go::TestCollectionVersionSimpleAddsCollectionGivenTypeWithStringField", "collection_version/comment_test.go::TestCollectionVersionSimpleAddsCollectionGivenLeadingComment", "collection_version/comment_test.go::TestCollectionVersionSimpleAddsCollectionGivenLeadingCommentWithMultiByteChar", "collection_version/with_inline_array_test.go::TestCollectionVersionInlineArrayAddsCollectionGivenSingleType", "collection_version/with_inline_array_test.go::TestCollectionVersionInlineArrayAddsCollectionGivenSecondType"]

Adding an SDL type (empty, with scalar fields, with inline arrays, several types in one SDL, or preceded by `#` comments including multi-byte characters) produces a GraphQL object type whose fields are the default system fields (`_docID`, `_version`, `_deleted`, aggregates, `_group`) plus the declared fields, as seen through `__type(name: ...)` introspection.

# `go.collection_version.define.scalar_kinds`: Every scalar kind is accepted and introspected

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/simple_test.go::TestCollectionVersionSimpleAddsCollectionGivenTypeWithBlobField", "collection_version/simple_test.go::TestCollectionVersionSimple_WithJSONField_AddsCollectionGivenType", "collection_version/simple_test.go::TestCollectionVersionSimple_WithFloat32Field_AddsCollectionGivenType", "collection_version/simple_test.go::TestCollectionVersionSimple_WithFloat64Field_AddsCollectionGivenType", "collection_version/simple_test.go::TestCollectionVersionSimple_WithFloatField_AddsCollectionGivenType", "collection_version/simple_test.go::TestCollectionVersionSimple_WithAllTypes_AddsCollectionGivenTypes"]
- **Go-Bug-Suspect:** "WithFloatField and WithAllTypes carry a TODO saying they document a graphql-go introspection bug (Go issue #3429); the asserted introspection output for 'Float' may not be the intended one."

Fields of kind `Blob`, `JSON`, `Float32`, `Float64`, `Float` and every other supported scalar can be declared in SDL; introspection reports `Float` and `Float64` fields as the `Float64` scalar and `Float32` as `Float32`.

# `go.collection_version.define.errors`: Invalid SDL definitions are rejected

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenDuplicateCollection", "collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenDuplicateCollectionInSameSDL", "collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenDuplicateCollectionInSameSDLMultiple", "collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenTypeWithInvalidFieldType", "collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenTypeWithInvalidFieldTypeMultiple", "collection_version/simple_test.go::TestCollectionVersionSimpleErrorsGivenNonNullManyRelationField", "collection_version/nil_type_test.go::TestCollectionVersion_WithMissingType_Errors"]

Adding a collection whose name already exists (in the store or twice in one SDL) fails with `collection already exists` (one `collection already exists. Name: Users` line per duplicate); an unknown field type fails with `no type found for given name. Field: name, Kind: NotAType` (one line per bad field); a non-null list relation such as `[Dogs!]` fails with `NonNull variants for type are not supported. Type: Dogs`; a field with no type fails with `field type not specified. Object: User, Field: name`.

# `go.collection_version.define.default_values`: `@default` field values

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/with_default_fields_test.go::TestCollectionVersion_WithDefaultFieldValues", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithInvalidDefaultFieldValueType_ReturnsError", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithIncorrectDefaultFieldValueType_ReturnsError", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithDefaultFieldValueNoArg_ReturnsError", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithUnknownDefaultFieldArg_ReturnsError", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithDefaultFieldValueOnRelation_ReturnsError", "collection_version/with_default_fields_test.go::TestCollectionVersion_WithDefaultFieldValueOnList_ReturnsError"]

`@default(<kind>: value)` on a scalar field records the default in the collection definition. A value of the wrong kind fails with `default value is invalid. Field: active, Expected: Boolean, Actual: Int, Value: 10`; no argument fails with `default value must specify one argument. Field: name`; an unknown argument fails with `Unknown argument "bool" on directive "@default"`; a default on a relation or list fails with `default value is not allowed for this field type. Name: friend, Type: User` / `Type: List`.

# `go.collection_version.define.crdt_types`: `@crdt` counter types in SDL

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPNCounterTypeWithIntKind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPNCounterTypeWithFloatKind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPNCounterTypeWithWrongKind_Error", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPNCounterWithInvalidType_Error", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPCounterTypeWithIntKind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPCounterTypeWithFloatKind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPCounterTypeWithFloat64Kind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPCounterTypeWithFloat32Kind_NoError", "collection_version/crdt_type_test.go::TestCollectionAdd_ContainsPCounterTypeWithWrongKind_Error"]
- **Scope:** "8 of 9 tests are skipped under the secondary-index multiplier (which adds @index to every field), because counter fields cannot be indexed (Go issue #4439)"

`@crdt(type: pncounter)` and `@crdt(type: pcounter)` are accepted on `Int` and float fields (`Float`, `Float32`, `Float64`); on a `String` field they fail with `CRDT type pncounter can't be assigned to field kind String` (resp. `pcounter`); an unknown type fails with `Argument "type" has invalid value "invalid"`.

# `go.collection_version.define.branchable`: `@branchable` directive

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/branchable_directive_test.go::TestColVersionBranchable_NoArguments_DefaultTrue", "collection_version/branchable_directive_test.go::TestColVersionBranchable_ArgumentIfTrue_ShouldBeTrue", "collection_version/branchable_directive_test.go::TestColVersionBranchable_ArgumentIfFalse_ShouldBeFalse"]

`@branchable` with no argument or `if: true` sets `IsBranchable: true` on the collection; `@branchable(if: false)` leaves it false.

# `go.collection_version.define.embeddings`: `@embedding` vector field definitions

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/vector_embedding_test.go::TestCollectionVersion_WithStringForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithIntForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithFloatForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithFloat64ForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithNillableFloat32ForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithFloat32ForEmbedding_ShouldSucceed", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithNonExistantFieldForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithInvalidEmbeddingGenerationFieldType_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithUnsupportedProviderForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_WithMissingModelForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_ReferenceToSelfForEmbedding_ShouldError", "collection_version/vector_embedding_test.go::TestCollectionVersion_ReferenceToAnotherEmbedding_ShouldError"]

`@embedding(fields: [...], provider: ..., model: ...)` is accepted only on a `[Float32!]` field: other kinds (String, Int, Float, Float64, nillable Float32 array) fail with `invalid type for vector embedding`. A missing source field fails with `the given field does not exist. Embedding generation field: about`; a JSON source with `invalid field type for vector embedding generation. Actual: JSON`; an unknown provider with `unknown embedding provider`; a missing model with `embedding Model cannot be empty`; referring to itself or another embedding with `embedding fields cannot refer to self or another embedding field. Field: name_v`.

# `go.collection_version.define.vector_index`: Vector indexes declared in SDL

- **Parents:** ["go.collection_version.define"]
- **Go-Tests:** ["collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexOnRawFloat32Array_ShouldSucceed", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexOnFloat32ArrayWithoutDimensionsOrEmbedding_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexOnStringField_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexOnIntArrayField_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexOnFloat64ArrayField_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithUnsupportedAlgorithm_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithEuclideanMetric_ShouldSucceed", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithDotProductMetric_ShouldSucceed", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithUnsupportedMetric_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithDescendingDirection_ShouldError", "collection_version/vector_index_test.go::TestCollectionVersion_VectorIndexWithAscendingDirection_ShouldSucceed"]

A vector `@index` on a raw `[Float32!]` field with dimensions is created and listed; metric `EUCLIDEAN` or `DOT_PRODUCT` and an explicit ascending direction are accepted. Missing dimensions with no embedding fails with `vector index dimensions must be greater than zero`; a String, Int-array or Float64-array field fails with `vector index requires a [Float32!] field`; an unknown algorithm (`IVFFlat`) fails with `In field "IVFFlat": Unknown field.`; metric `MANHATTAN` fails with `Expected type "VectorDistanceMetric", found MANHATTAN`; a descending direction fails with `vector index cannot have a direction`.

# `go.collection_version.relations`: Relation definitions

- **Parents:** ["go.collection_version"]

How relation fields between collections are declared and validated when collections are added.

# `go.collection_version.relations.one_one`: One-to-one relation declaration

- **Parents:** ["go.collection_version.relations"]
- **Go-Tests:** ["collection_version/one_one_test.go::TestCollectionVersionOneOne_NoPrimary_Errors", "collection_version/one_one_test.go::TestCollectionVersionOneOne_TwoPrimaries_Errors", "collection_version/one_one_test.go::TestCollectionVersionOneOne_SelfUsingActualName"]

A one-to-one relation needs exactly one side marked `@primary`: with neither side it fails with `relation missing field`, with both it fails with `relation can only have a single field set as primary`. A self one-to-one using the type's own name is accepted and introspects correctly.

# `go.collection_version.relations.one_many`: One-to-many relation declaration

- **Parents:** ["go.collection_version.relations"]
- **Go-Tests:** ["collection_version/one_many_test.go::TestCollectionVersionOneMany_Primary", "collection_version/one_many_test.go::TestCollectionVersionOneMany_SelfReferenceOneFieldLexographicallyFirst", "collection_version/one_many_test.go::TestCollectionVersionOneMany_SelfReferenceManyFieldLexographicallyFirst", "collection_version/one_many_test.go::TestCollectionVersionOneMany_SelfUsingActualName"]
- **Go-Bug-Suspect:** "TestCollectionVersionOneMany_SelfUsingActualName needs an explicit '@primary' only because of Go issue #2620 (comment says it should be removed once fixed)."

A one-to-many relation gets its relation ID field on the one side regardless of field declaration order, including self-referencing one-to-many relations whatever field sorts first; a self one-to-many declared with the type's own name introspects correctly.

# `go.collection_version.relations.relation_names`: Relation name uniqueness

- **Parents:** ["go.collection_version.relations"]
- **Go-Tests:** ["collection_version/duplicate_relation_name_test.go::TestCollectionVersionDuplicateRelationNameWithoutRelationDirective", "collection_version/duplicate_relation_name_test.go::TestCollectionVersionDuplicateRelationNameWithRelationDirective", "collection_version/duplicate_relation_name_test.go::TestCollectionVersionSelfRelationPrimarySecondaryPairWithSameRelationName"]

Two relation fields in one collection that infer the same relation name fail with `relation name is not unique within collection. Field: author, RelationName: book_person`; giving them distinct `@relation(name: ...)` values succeeds, and a self-relation primary/secondary pair may share one relation name.

# `go.collection_version.relations.self_ref_ids`: Collection IDs of self-referencing collection sets

- **Parents:** ["go.collection_version.relations"]
- **Go-Tests:** ["collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceSimple_HasSimpleCollectionID", "collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceTwoTypes_HasComplexCollectionID", "collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceTwoTypes_HasComplexCollectionID_SingleSidedRelations", "collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceTwoPairsOfTwoTypes_HaveDifferentComplexCollectionID", "collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceTwoPairsOfTwoTypesJoinedByThirdCircle_AllHaveSameBaseCollectionID", "collection_version/self_ref_test.go::TestCollectionVersionSelfReferenceTwoPairsOfTwoTypesJoinedByThirdCircleAcrossAll_AllHaveSameBaseCollectionID", "collection_version/one_one_many_self_ref_test.go::TestCollectionVersionWith_OneOne_OneMany_SelfRef"]

A collection with a relation to itself gets a plain self kind with no base ID; collections joined in a relation cycle share one base collection ID with distinct index suffixes, separate cycles get different base IDs, and cycles joined by a third cycle all share one base ID. Mixed one-one and one-many self relations produce correct definitions from `GetCollections`.

# `go.collection_version.introspection`: GraphQL schema generated for collections

- **Parents:** ["go.collection_version"]

The GraphQL types, inputs and arguments DefraDB generates from collection definitions, checked through introspection queries.

# `go.collection_version.introspection.aggregate_inline_array`: Aggregate arguments over inline arrays

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersCount", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersSum", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersAverage", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersNillableBooleanCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersBooleanCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersNillableIntegerCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersIntegerCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersNillableFloatCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersFloatCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersNillableStringCountFilter", "collection_version/aggregates/inline_array_test.go::TestCollectionVersionAggregateInlineArrayAddsUsersStringCountFilter"]

For inline array fields, `_count`, `_sum` and `_avg` expose arguments for the array field, and `_count` takes a filter input whose operator block matches the element kind (nillable and non-nillable Boolean, Int, Float, String).

# `go.collection_version.introspection.aggregate_fields`: Aggregate fields on collections and relations

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/aggregates/simple_test.go::TestCollectionVersionAggregateSimpleAddsUsersCount", "collection_version/aggregates/simple_test.go::TestCollectionVersionAggregateSimpleAddsUsersSum", "collection_version/aggregates/simple_test.go::TestCollectionVersionAggregateSimpleAddsUsersAverage", "collection_version/aggregates/top_level_test.go::TestCollectionVersionAggregateTopLevelAddsCountGivenCollection", "collection_version/aggregates/top_level_test.go::TestCollectionVersionAggregateTopLevelAddsSumGivenCollection", "collection_version/aggregates/top_level_test.go::TestCollectionVersionAggregateTopLevelAddsAverageGivenCollection"]

Every collection type exposes `_count`, `_sum` and `_avg` with arguments naming its related many-side fields, and the top-level `Query` exposes `_count`, `_sum` and `_avg` taking the collection as argument.

# `go.collection_version.introspection.filter_inputs`: Filter input types

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/filter_test.go::TestFilterForSimpleCollection", "collection_version/filter_test.go::TestFilterForOneToOneCollection", "collection_version/filter_test.go::TestCollectionVersionFilterInputs_WithJSONField_Succeeds", "collection_version/filter_test.go::TestCollectionVersionFilterInputs_WithNonNillableStringField_UsesStringOperatorBlock", "collection_version/filter_test.go::TestCollectionVersionFilterInputs_WithNonNillableJSONField_UsesJSONScalar"]

Each collection gets a `<Name>FilterArg` input with `_and`, `_or`, `_not`, one operator block per field and relation fields filtered through the related type's filter input; non-nillable String fields use the string operator block and JSON fields (nillable or not) use the JSON scalar.

# `go.collection_version.introspection.group_order_inputs`: Group-by and order input types

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/group_test.go::TestGroupByFieldForTheManySideInCollection", "collection_version/group_test.go::TestGroupByFieldForTheSingleSideInCollection", "collection_version/input_type_test.go::TestInputTypeOfOrderFieldWhereCollectionHasManyRelationType", "collection_version/input_type_test.go::TestInputTypeOfOrderFieldWhereCollectionHasRelationType"]

`groupBy` on a collection accepts its field enum including the one side's relation ID field, and the `order` argument's input type includes relation fields from both one and many sides as nested order inputs.

# `go.collection_version.introspection.mutation_inputs`: Mutation input and field enum types

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/add_one_one_data_test.go::TestAddOneToOne_Input_PrimaryObject", "collection_version/add_one_one_data_test.go::TestAddOneToOne_Input_SecondaryObject", "collection_version/type_explicit_fields_test.go::TestEncryptFieldsForAddMutation"]

For a one-to-one relation, the primary side's `<Name>MutationInputArg` includes the relation object and `_<rel>ID` fields while the secondary side's input lists its scalar fields; each collection gets a `<Name>Field` enum listing its fields (used e.g. for `encryptFields`).

# `go.collection_version.introspection.similarity`: `_similarity` on vector-capable fields

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/similarity_test.go::TestCollectionVersionIntrospection_SimilarityCapableFieldIntArray", "collection_version/similarity_test.go::TestCollectionVersionIntrospection_SimilarityCapableFieldFloat32Array", "collection_version/similarity_test.go::TestCollectionVersionIntrospection_SimilarityCapableFieldsIntArrayAndFloat32Array", "collection_version/similarity_test.go::TestCollectionVersionIntrospection_SimilarityIsDocumented"]

Collections with `[Int!]` or `[Float32!]` array fields expose a `_similarity` field whose argument names those fields, and the field and its arguments carry descriptions visible through introspection.

# `go.collection_version.introspection.builtin_types`: Built-in schema types

- **Parents:** ["go.collection_version.introspection"]
- **Go-Tests:** ["collection_version/client_test.go::TestIntrospectionExplainTypeDefined", "collection_version/client_introspection/simple_test.go::TestClientIntrospectionBasic", "collection_version/client_introspection/one_many_test.go::TestClientIntrospectionWithOneToManyCollection"]

With or without collections, introspection exposes the `ExplainType` enum with its description, and the full client introspection query succeeds, including with a one-to-many schema.

# `go.collection_version.get`: Reading collection versions

- **Parents:** ["go.collection_version"]
- **Go-Tests:** ["collection_version/get_collection_version_test.go::TestGetCollectionVersion_GivenNonExistantCollectionVersionID_Errors", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_GivenNoCollectionReturnsEmptySet", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_GivenNoCollectionGivenUnknownRoot", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_GivenNoCollectionGivenUnknownName", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_ReturnsAllCollections", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_ReturnsCollectionForGivenRoot", "collection_version/get_collection_version_test.go::TestGetCollectionVersion_ReturnsCollectionForGivenName", "collection_version/describe_string_kind_test.go::TestGetCollectionVersion_RendersAndRoundTripsStringifiedKinds", "collection_version/describe_string_kind_test.go::TestAddCollection_WithStringifiedKinds_RoundTripsToOriginalVersions"]

`GetCollections` (`client collection describe`) returns all versions (including inactive ones when asked) or filters by collection ID, name or version ID; unknown IDs or names return an empty set, while an unknown version ID fails with `collection not found`. Field kinds, including scalar, array, self and cross-collection relation kinds, render as readable strings and round-trip back to the same versions when passed to `AddCollection`.

# `go.collection_version.active_version`: Choosing the active collection version

- **Parents:** ["go.collection_version"]
- **Go-Tests:** ["collection_version/with_update_set_default_test.go::TestCollectionVersion_WithUpdateAndSetDefaultVersionToEmptyString_Errors", "collection_version/with_update_set_default_test.go::TestCollectionVersion_WithUpdateAndSetDefaultVersionToUnknownVersion_Errors", "collection_version/with_update_set_default_test.go::TestCollectionVersion_WithUpdateAndSetDefaultVersionToOriginal_NewFieldIsNotQueriable", "collection_version/with_update_set_default_test.go::TestCollectionVersion_WithUpdateAndSetDefaultVersionToNew_AllowsQueryingOfNewField", "collection_version/updates/replace/is_active_test.go::TestColVersionUpdateReplaceIsActive_False", "collection_version/updates/replace/is_active_test.go::TestColVersionUpdateReplaceIsActive_FalseThenTrue", "collection_version/updates/replace/is_active_test.go::TestColVersionUpdateReplaceIsActive_MultipleVersionsToTrue", "collection_version/updates/replace/is_active_txn_test.go::TestColVersionUpdateReplaceIsActive_GetCollectionsWithTxn"]

`SetActiveCollectionVersion` or a patch of `/Name/IsActive` selects which version GraphQL serves. An empty version ID fails with `collection version ID can't be empty`, an unknown one with `collection not found`; reverting to the original version hides fields added later (`Cannot query field "email" on type "Users".`), switching to the new one exposes them; setting `IsActive: false` removes the collection from `Query` and `true` restores it; activating two versions of one collection fails with `collection already exists. Name: Users`; `GetCollections` inside a transaction sees that transaction's activation change.

# `go.collection_version.version_branches`: Branching collection version history

- **Parents:** ["go.collection_version"]
- **Go-Tests:** ["collection_version/updates/with_version_branch_test.go::TestCollectionVersionUpdates_WithBranchingCollection", "collection_version/updates/with_version_branch_test.go::TestCollectionVersionUpdates_WithPatchOnBranchedCollection", "collection_version/updates/with_version_branch_test.go::TestCollectionVersionUpdates_WithBranchingCollectionAndSetActiveCollectionToOtherBranch", "collection_version/updates/with_version_branch_test.go::TestCollectionVersionUpdates_WithBranchingCollectionAndSetActiveCollectionToOtherBranchThenPatch", "collection_version/updates/with_version_branch_test.go::TestCollectionVersionUpdates_WithBranchingCollectionAndGetCollectionAtVersion"]

Patching an older (non-active) version creates a sibling branch of versions: each branch keeps its own fields (`Cannot query field "phone" on type "Users".` on the other branch), either branch can be activated and patched further, and `GetCollections` returns every version in the tree.

# `go.collection_version.patch`: Patching collections into new versions

- **Parents:** ["go.collection_version"]

`PatchCollection` (`client collection patch`, JSON Patch over the collection definitions keyed by name or version ID) creates new collection versions; these requirements cover what each patch operation may and may not change.

# `go.collection_version.patch.add_field`: Adding fields by patch

- **Parents:** ["go.collection_version.patch"]

`add` operations on `/<Collection>/Fields/-` (or `/Fields/<name>`) create a new version with the extra field.

# `go.collection_version.patch.add_field.basic`: Added fields become queryable

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimple", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdates_AddFieldSimpleInactiveFalse_Errors", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdates_AddFieldSimpleDoNotSetDefault_VersionIsQueryable", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldMultipleInPatch", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldMultiplePatches", "collection_version/updates/add/field/with_filter_test.go::TestCollectionVersionUpdatesAddFieldSimpleWithFilter", "collection_version/updates/add/field/with_filter_test.go::TestCollectionVersionUpdatesAddFieldSimpleWithFilterOnPopulatedDatabase", "collection_version/updates/add/field/with_introspection_test.go::TestCollectionVersionUpdatesAddFieldIntrospection", "collection_version/updates/add/field/with_relation_test.go::TestCollectionVersionUpdatesAddField_DoesNotAffectExistingRelation", "collection_version/updates/add/field/constraint/size_test.go::TestCollectionVersionUpdates_AddFieldSizeContraint_ShouldSucceed"]

After adding one or several fields (in one or successive patches) the new version is active and the fields can be queried, filtered and introspected, existing relations are unaffected, and `@constraints(size: 2)` on an added array field is enforced (`array size mismatch. Actual: 3, Expected: 2`). With `SetAsActiveVersion` false the new field is not queryable (`Cannot query field "email" on type "Users".`) but the version is still returned by `GetCollections`.

# `go.collection_version.patch.add_field.atomicity`: A failing patch changes nothing

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldMultipleInPatchPartialSuccess", "collection_version/updates/add/field/with_introspection_test.go::TestCollectionVersionUpdatesAddFieldIntrospectionDoesNotAmendGQLTypesGivenBadPatch"]

When one operation in a patch fails (`no type found for given name. Type: 111`), none of its operations apply: the valid field is not queryable, the original definition remains and the GraphQL types are not amended.

# `go.collection_version.patch.add_field.validation`: Added field validation

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimpleWithoutName", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimpleDuplicateOfExistingField", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimpleDuplicateOfExistingFieldMultiple", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimpleDuplicateField"]

An added field without a name fails with `Names must match /^[_a-zA-Z][_a-zA-Z0-9]*$/ but "" does not.`; a name that duplicates an existing or another added field fails with `duplicate field. Name: name`.

# `go.collection_version.patch.add_field.non_nillable`: Non-nillable fields cannot be added

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableBoolField_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableIntField_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableFloat64Field_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableFloat32Field_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableStringField_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableDateTimeField_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableBlobField_Error", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesAddNonNillableJSONField_Error"]

Adding a non-nillable field (Boolean, Int, Float64, Float32, String, DateTime, Blob or JSON) to an existing collection fails with `adding a non-nillable field to an existing collection is not supported`.

# `go.collection_version.patch.add_field.existing_docs`: Documents across a field addition

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/add_test.go::TestCollectionVersionUpdatesAddFieldWithAdd", "collection_version/updates/add/field/add_test.go::TestCollectionVersionUpdatesAddFieldWithAddAfterCollectionUpdate", "collection_version/updates/add/field/add_test.go::TestCollectionVersionUpdatesAddField_WithExistingSecondaryOneToOneRelationship", "collection_version/updates/add/field/add_update_test.go::TestCollectionVersionUpdatesAddFieldWithAddWithUpdateAfterCollectionUpdateAndVersionJoin", "collection_version/updates/add/field/add_update_test.go::TestCollectionVersionUpdatesAddFieldWithAddWithUpdateAfterCollectionUpdateAndCommitQuery"]

Documents created before a field is added return `null` for it; documents created afterwards, updates after the patch, version joins and `_commits` queries all work across the version change, including with an existing secondary one-to-one relation.

# `go.collection_version.patch.add_field.by_name_path`: Adding a field addressed by name

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/with_index_sub_test.go::TestCollectionVersionUpdatesAddFieldSimple_FieldIndexedByName", "collection_version/updates/add/field/with_index_sub_test.go::TestCollectionVersionUpdatesAddFieldSimple_FieldIndexedByNameWithSameNameDefinedInValue", "collection_version/updates/add/field/with_index_sub_test.go::TestCollectionVersionUpdatesAddFieldSimple_FieldIndexedByNameWithDifferentNameDefinedInValue", "collection_version/updates/add/field/with_index_sub_test.go::TestCollectionVersionUpdatesAddFieldSimple_FieldIndexedByNameMultipleTimes"]

A field added at `/Users/Fields/email` takes its name from the path (a matching `Name` in the value is fine, a different one fails with `the index used does not match the given name`), and later operations in the same patch can address it by that name.

# `go.collection_version.patch.add_field.with_index`: Adding fields to an indexed collection

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/field/with_index_test.go::TestCollectionVersionUpdatesAddFieldSimple_WithExistingIndexDocsAddedAfterPatch", "collection_version/updates/add/field/with_index_test.go::TestCollectionVersionUpdatesAddFieldSimple_WithExistingIndexDocsAddedBeforePatch"]

Adding a field to a collection with an existing index keeps the index in both `ListIndexes` and `GetCollections`, and indexed queries return documents added before or after the patch.

# `go.collection_version.patch.add_field.branchable`: Adding fields to a branchable collection

- **Parents:** ["go.collection_version.patch.add_field"]
- **Go-Tests:** ["collection_version/updates/add/branchable_test.go::TestBranchableCollection_AddNewField_ShouldUpdateCollectionDefinition", "collection_version/updates/add/branchable_test.go::TestBranchableCollection_AddNewFieldWithMultipleDocs_ShouldAddField"]

Adding a field to a `@branchable` collection updates its definition and documents can then be created and updated with the new field.

# `go.collection_version.patch.add_field.crdt`: CRDT type of added fields

- **Parents:** ["go.collection_version.patch.add_field"]

The `Typ` given for an added field.

# `go.collection_version.patch.add_field.crdt.valid`: Supported CRDT types

- **Parents:** ["go.collection_version.patch.add_field.crdt"]
- **Go-Tests:** ["collection_version/updates/add/field/crdt/lww_test.go::TestCollectionVersionUpdatesAddFieldCRDTLWW", "collection_version/updates/add/field/crdt/none_test.go::TestCollectionVersionUpdatesAddFieldCRDTDefault", "collection_version/updates/add/field/crdt/none_test.go::TestCollectionVersionUpdatesAddFieldCRDTNone", "collection_version/updates/add/field/crdt/pcounter_test.go::TestCollectionVersionUpdates_AddFieldCRDTPCounter_NoError", "collection_version/updates/add/field/crdt/pncounter_test.go::TestCollectionVersionUpdates_AddFieldCRDTPNCounter_NoError", "collection_version/updates/add/field/crdt/string_typ_test.go::TestCollectionVersionUpdates_AddFieldCRDTTypAsString_NoError"]

An added field with no `Typ`, `Typ` 0 (none), LWW, `pcounter` or `pncounter` on a numeric kind is accepted and queryable; `Typ` may also be given as its string name (e.g. `"lww"`).

# `go.collection_version.patch.add_field.crdt.invalid`: Rejected CRDT types

- **Parents:** ["go.collection_version.patch.add_field.crdt"]
- **Go-Tests:** ["collection_version/updates/add/field/crdt/composite_test.go::TestCollectionVersionUpdatesAddFieldCRDTCompositeErrors", "collection_version/updates/add/field/crdt/composite_test.go::TestCollectionVersionUpdatesAddFieldCRDTCompositeErrorsMultiple", "collection_version/updates/add/field/crdt/invalid_test.go::TestCollectionVersionUpdatesAddFieldCRDTInvalidErrors", "collection_version/updates/add/field/crdt/invalid_test.go::TestCollectionVersionUpdatesAddFieldCRDTInvalidErrorsMultiple", "collection_version/updates/add/field/crdt/object_bool_test.go::TestCollectionVersionUpdatesAddFieldCRDTObjectWithBoolFieldErrors", "collection_version/updates/add/field/crdt/object_bool_test.go::TestCollectionVersionUpdatesAddFieldCRDTObjectWithBoolFieldErrorsMultiple", "collection_version/updates/add/field/crdt/pcounter_test.go::TestCollectionVersionUpdates_AddFieldCRDTPCounterWithMismatchKind_Error", "collection_version/updates/add/field/crdt/pncounter_test.go::TestCollectionVersionUpdates_AddFieldCRDTPNCounterWithMismatchKind_Error", "collection_version/updates/add/field/crdt/string_typ_test.go::TestCollectionVersionUpdates_AddFieldCRDTTypAsUnknownString_Error"]

Composite, object or unknown numeric `Typ` values fail with `CRDT type not supported. Name: foo, CRDTType: composite` (`object`, `unknown`), one line per bad field; a counter on a Boolean fails with `CRDT type pcounter can't be assigned to field kind Boolean` (resp. `pncounter`); an unknown string name fails with `unknown crdt string representation`.

# `go.collection_version.patch.add_field.kind`: Kind of added fields

- **Parents:** ["go.collection_version.patch.add_field"]

Each scalar kind can be added by numeric kind ID, used in documents created afterwards, and given by its string name (kind substitution).

# `go.collection_version.patch.add_field.kind.numeric`: Numeric kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/int_test.go::TestCollectionVersionUpdatesAddFieldKindInt", "collection_version/updates/add/field/kind/int_test.go::TestCollectionVersionUpdatesAddFieldKindIntWithAdd", "collection_version/updates/add/field/kind/int_test.go::TestCollectionVersionUpdatesAddFieldKindIntSubstitutionWithAdd", "collection_version/updates/add/field/kind/float_test.go::TestCollectionVersionUpdatesAddFieldKindFloat", "collection_version/updates/add/field/kind/float_test.go::TestCollectionVersionUpdatesAddFieldKindFloatWithAdd", "collection_version/updates/add/field/kind/float_test.go::TestCollectionVersionUpdatesAddFieldKindFloatSubstitutionWithAdd", "collection_version/updates/add/field/kind/float32_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32", "collection_version/updates/add/field/kind/float32_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32WithAdd", "collection_version/updates/add/field/kind/float32_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32SubstitutionWithAdd"]

Adding an `Int`, `Float` (Float64) or `Float32` field by kind ID or by name makes it queryable, and documents created afterwards store and return values of that kind.

# `go.collection_version.patch.add_field.kind.other_scalars`: Boolean, String, DateTime and DocID kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/bool_test.go::TestCollectionVersionUpdatesAddFieldKindBool", "collection_version/updates/add/field/kind/bool_test.go::TestCollectionVersionUpdatesAddFieldKindBoolWithAdd", "collection_version/updates/add/field/kind/bool_test.go::TestCollectionVersionUpdatesAddFieldKindBoolSubstitutionWithAdd", "collection_version/updates/add/field/kind/string_test.go::TestCollectionVersionUpdatesAddFieldKindString", "collection_version/updates/add/field/kind/string_test.go::TestCollectionVersionUpdatesAddFieldKindStringWithAdd", "collection_version/updates/add/field/kind/string_test.go::TestCollectionVersionUpdatesAddFieldKindStringSubstitutionWithAdd", "collection_version/updates/add/field/kind/datetime_test.go::TestCollectionVersionUpdatesAddFieldKindDateTime", "collection_version/updates/add/field/kind/datetime_test.go::TestCollectionVersionUpdatesAddFieldKindDateTimeWithAdd", "collection_version/updates/add/field/kind/datetime_test.go::TestCollectionVersionUpdatesAddFieldKindDateTimeSubstitutionWithAdd", "collection_version/updates/add/field/kind/doc_id_test.go::TestCollectionVersionUpdatesAddFieldKindDocID", "collection_version/updates/add/field/kind/doc_id_test.go::TestCollectionVersionUpdatesAddFieldKindDocIDWithAdd", "collection_version/updates/add/field/kind/doc_id_test.go::TestCollectionVersionUpdatesAddFieldKindDocIDSubstitutionWithAdd"]

Adding a `Boolean`, `String`, `DateTime` or `ID` (DocID) field by kind ID or by name makes it queryable, and documents created afterwards store and return values of that kind.

# `go.collection_version.patch.add_field.kind.blob_json`: Blob and JSON kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/blob_test.go::TestCollectionVersionUpdatesAddFieldKindBlob", "collection_version/updates/add/field/kind/blob_test.go::TestCollectionVersionUpdatesAddFieldKindBlobWithAdd", "collection_version/updates/add/field/kind/blob_test.go::TestCollectionVersionUpdatesAddFieldKindBlobSubstitutionWithAdd", "collection_version/updates/add/field/kind/json_test.go::TestCollectionVersionUpdatesAddFieldKindJSON", "collection_version/updates/add/field/kind/json_test.go::TestCollectionVersionUpdatesAddFieldKindJSONWithAdd", "collection_version/updates/add/field/kind/json_test.go::TestCollectionVersionUpdatesAddFieldKindJSONSubstitutionWithAdd"]

Adding a `Blob` or `JSON` field by kind ID or by name makes it queryable, and documents created afterwards store and return hex blobs and JSON values.

# `go.collection_version.patch.add_field.kind.arrays`: Non-nillable-element array kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/bool_array_test.go::TestCollectionVersionUpdatesAddFieldKindBoolArray", "collection_version/updates/add/field/kind/bool_array_test.go::TestCollectionVersionUpdatesAddFieldKindBoolArrayWithAdd", "collection_version/updates/add/field/kind/bool_array_test.go::TestCollectionVersionUpdatesAddFieldKindBoolArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/int_array_test.go::TestCollectionVersionUpdatesAddFieldKindIntArray", "collection_version/updates/add/field/kind/int_array_test.go::TestCollectionVersionUpdatesAddFieldKindIntArrayWithAdd", "collection_version/updates/add/field/kind/int_array_test.go::TestCollectionVersionUpdatesAddFieldKindIntArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/float_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloatArray", "collection_version/updates/add/field/kind/float_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloatArrayWithAdd", "collection_version/updates/add/field/kind/float_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloatArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/float32_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32Array", "collection_version/updates/add/field/kind/float32_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32ArrayWithAdd", "collection_version/updates/add/field/kind/float32_array_test.go::TestCollectionVersionUpdatesAddFieldKindFloat32ArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/string_array_test.go::TestCollectionVersionUpdatesAddFieldKindStringArray", "collection_version/updates/add/field/kind/string_array_test.go::TestCollectionVersionUpdatesAddFieldKindStringArrayWithAdd", "collection_version/updates/add/field/kind/string_array_test.go::TestCollectionVersionUpdatesAddFieldKindStringArraySubstitutionWithAdd"]

Adding `[Boolean!]`, `[Int!]`, `[Float!]`, `[Float32!]` or `[String!]` fields by kind ID or by name makes them queryable and storable.

# `go.collection_version.patch.add_field.kind.nillable_arrays`: Nillable-element array kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/bool_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableBoolArray", "collection_version/updates/add/field/kind/bool_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableBoolArrayWithAdd", "collection_version/updates/add/field/kind/bool_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableBoolArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/int_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableIntArray", "collection_version/updates/add/field/kind/int_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableIntArrayWithAdd", "collection_version/updates/add/field/kind/int_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableIntArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/float_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloatArray", "collection_version/updates/add/field/kind/float_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloatArrayWithAdd", "collection_version/updates/add/field/kind/float_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloatArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/float32_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloat32Array", "collection_version/updates/add/field/kind/float32_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloat32ArrayWithAdd", "collection_version/updates/add/field/kind/float32_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableFloat32ArraySubstitutionWithAdd", "collection_version/updates/add/field/kind/string_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableStringArray", "collection_version/updates/add/field/kind/string_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableStringArrayWithAdd", "collection_version/updates/add/field/kind/string_nil_array_test.go::TestCollectionVersionUpdatesAddFieldKindNillableStringArraySubstitutionWithAdd"]

Adding `[Boolean]`, `[Int]`, `[Float]`, `[Float32]` or `[String]` fields by kind ID or by name makes them queryable and storable, `null` elements included.

# `go.collection_version.patch.add_field.kind.invalid`: Invalid kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/invalid_test.go::TestCollectionVersionUpdatesAddFieldKind32", "collection_version/updates/add/field/kind/invalid_test.go::TestCollectionVersionUpdatesAddFieldKind198", "collection_version/updates/add/field/kind/invalid_test.go::TestCollectionVersionUpdatesAddFieldKindInvalid", "collection_version/updates/add/field/kind/none_test.go::TestCollectionVersionUpdatesAddFieldKindNone"]

An added field with kind 0, an unassigned kind ID (32, 198) or an unknown kind name fails with `no type found for given name. Type: 32` / `Field: foo, Kind: InvalidKind`.

# `go.collection_version.patch.add_field.kind.foreign_object`: Relation kinds

- **Parents:** ["go.collection_version.patch.add_field.kind"]
- **Go-Tests:** ["collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_UnknownCollection", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_IDFieldMissingKind", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_IDFieldInvalidKind", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_Succeeds", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithPatchAddingOneToOneRelationInSameBatch_ShouldSucceed", "collection_version/updates/add/field/kind/foreign_object_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithPatchAddingOneToManyRelationInSameBatch_ShouldSucceed", "collection_version/updates/add/field/kind/foreign_object_array_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObjectArray_UnknownCollection", "collection_version/updates/add/field/kind/foreign_object_array_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObjectArray_NoRelationName", "collection_version/updates/add/field/kind/foreign_object_array_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObjectArray_KnownCollection"]

Adding a relation field naming an unknown collection fails with `no type found for given name. Field: foo, Kind: Unknown` (`[Unknown]` for lists); without a relation name with `relation name cannot be empty. Field: foo`; a one-sided list relation with `relation missing field. Object: Users, RelationName: users_users`; a `_fooID` field of the wrong kind with `relational id field of invalid kind. Field: _fooID, Expected: ID, Actual: Boolean`; numeric kind 16 fails with `no type found for given name. Type: 16`. A complete one-to-one or one-to-many relation added in one patch succeeds and is queryable.

# `go.collection_version.patch.add_collection`: Collections cannot be added by patch

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/add/collections_test.go::TestColVersionUpdateAddCollections_WithUndefinedID_Errors", "collection_version/updates/add/collections_test.go::TestColVersionUpdateAddCollections_WithEmptyID_Errors", "collection_version/updates/add/collections_test.go::TestColVersionUpdateAddCollections_Errors", "collection_version/updates/add/collections_test.go::TestColVersionUpdateAddCollections_WithNoIndex_Errors", "collection_version/updates/add/simple_test.go::TestCollectionVersionUpdatesAddSimpleErrorsAddingCollection", "collection_version/updates/copy/simple_test.go::TestCollectionVersionUpdatesCopyCollectionWithRemoveIDAndReplaceName", "collection_version/updates/replace/simple_test.go::TestCollectionVersionUpdatesReplaceCollectionErrors"]

A patch that would create a collection (add at an unknown top-level path, `/-`, a copy of a collection under a new name, or a replace of a collection with another) fails with `adding collections via patch is not supported`, naming the new collection (e.g. `. Name: Book`).

# `go.collection_version.patch.invalid_paths`: Patches to unknown paths or properties

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/add/simple_test.go::TestCollectionVersionUpdatesAddSimpleErrorsAddingCollectionProp", "collection_version/updates/add/simple_test.go::TestCollectionVersionUpdatesAddSimpleErrorsAddingUnsupportedCollectionProp", "collection_version/updates/add/simple_test.go::TestCollectionVersionUpdatesAddSimpleErrorsAddingUnsupportedCollectionPropAtRoot", "collection_version/updates/add/field/simple_test.go::TestCollectionVersionUpdatesAddFieldSimpleErrorsAddingToUnknownCollection"]

Adding an unknown property to a collection fails with `json: unknown field "-"`; adding under a path that does not exist (unknown collection, unsupported property, at the root) fails with `add operation does not apply: doc is missing path`.

# `go.collection_version.patch.field_reorder`: Fields cannot be moved

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/copy/field/simple_test.go::TestCollectionVersionUpdatesCopyFieldErrors", "collection_version/updates/copy/field/simple_test.go::TestCollectionVersionUpdatesCopyFieldErrorsMultiple", "collection_version/updates/move/field/simple_test.go::TestCollectionVersionUpdatesMoveFieldErrors", "collection_version/updates/move/field/simple_test.go::TestCollectionVersionUpdatesMoveFieldErrorsMultiple"]

Copying or moving a field onto another position fails with `moving fields is not currently supported. Name: name, ProposedIndex: 1, ExistingIndex: 2`, one line per moved field.

# `go.collection_version.patch.copy_field`: Copying a field as a new field

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/copy/field/simple_test.go::TestCollectionVersionUpdatesCopyFieldWithAndReplaceName", "collection_version/updates/copy/field/simple_test.go::TestCollectionVersionUpdatesCopyFieldWithReplaceNameAndKindSubstitution", "collection_version/updates/copy/field/simple_test.go::TestCollectionVersionUpdatesCopyFieldAndReplaceNameAndInvalidKindSubstitution", "collection_version/updates/copy/field/with_introspection_test.go::TestCollectionVersionUpdatesCopyFieldIntrospectionWithRemoveIDAndReplaceName"]

Copying a field to the end of the field list and renaming it (optionally replacing its kind by name) adds a new queryable field with no values for existing documents; an invalid substituted kind fails with `no type found for given name. Field: Age, Kind: NotAValidKind`.

# `go.collection_version.patch.collection_name`: Collection names are immutable

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/replace/name_test.go::TestColVersionUpdateReplaceName_GivenExistingName", "collection_version/updates/replace/name_test.go::TestColVersionUpdateReplaceName_GivenInactiveCollection_Errors", "collection_version/updates/replace/name_one_many_test.go::TestColVersionUpdateReplaceNameOneToMany", "collection_version/updates/remove/name_test.go::TestColVersionUpdateRemoveNameByVersionID", "collection_version/updates/remove/name_test.go::TestColVersionUpdateRemoveName", "collection_version/updates/move/name_test.go::TestColVersionUpdateMoveName", "collection_version/updates/copy/name_test.go::TestColVersionUpdateCopyName", "collection_version/updates/move/simple_test.go::TestCollectionVersionUpdatesMoveCollectionDoesNothing"]

Replacing a collection's `Name` (active or inactive version, including one in a one-to-many relation) fails with `collection name cannot be mutated.`; removing or moving it away fails with `collection name can't be empty`. Copying a name between versions in a patch that also adds fields and deactivates the source is accepted, and moving a whole collection key to another key is a no-op that leaves documents and commits intact.

# `go.collection_version.patch.immutable_identity`: IDs and sources are immutable

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/replace/id_test.go::TestColVersionUpdateReplaceID_WithEmpty_Errors", "collection_version/updates/replace/id_test.go::TestColVersionUpdateReplaceID_WithExisting_Errors", "collection_version/updates/replace/id_test.go::TestColVersionUpdateReplaceID_WithExistingSameRoot_Errors", "collection_version/updates/replace/id_test.go::TestColVersionUpdateReplaceID_WithExistingDifferentRoot_Errors", "collection_version/updates/replace/id_test.go::TestColVersionUpdateReplaceID_WithNew_Errors", "collection_version/updates/replace/root_id_test.go::TestColVersionUpdateReplaceCollectionID_Errors", "collection_version/updates/remove/version_id_test.go::TestCollectionVersionUpdatesRemoveVersionIDErrors", "collection_version/updates/replace/sources_test.go::TestColVersionUpdateReplaceSources_Errors", "collection_version/updates/replace/sources_test.go::TestColVersionUpdateReplaceSourcesWithQuerySource_Errors", "collection_version/updates/replace/col_source_source_id_test.go::TestColVersionUpdateReplaceCollectionSourceSourceCollectionID_Errors"]

Patching identity properties fails: empty `CollectionID` with `collection ID cannot be empty`, changing it with `collection ID cannot be mutated.`, an invalid version ID with `invalid cid: selected encoding not supported. VersionID: invalid cid`, removing it with `invalid cid: cid too short. VersionID:`, an unknown CID with `unknown CID, collection ids cannot be manually defined`, adding or removing sources with `collection sources cannot be added or removed.`, a source from another collection with `collection source must belong to host collection.`, and a changed source collection ID with `collection source ID cannot be mutated.`.

# `go.collection_version.patch.immutable_settings`: Branchability, indexes, policy and materialization are immutable

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/replace/branchable_test.go::TestColVersionUpdateReplaceIsBranchable_UpdatingFromTrueToFalse_Errors", "collection_version/updates/replace/branchable_test.go::TestColVersionUpdateReplaceIsBranchable_UpdatingFromFalseToTrue_Errors", "collection_version/updates/replace/indexes_test.go::TestColVersionUpdateReplaceIndexes_Errors", "collection_version/updates/replace/policy_test.go::TestColVersionUpdateReplacePolicy_Errors", "collection_version/updates/replace/policy_test.go::TestColVersionUpdateReplacePolicyID_Errors", "collection_version/updates/replace/policy_test.go::TestColVersionUpdateReplacePolicyResource_Errors", "collection_version/updates/remove/policy_test.go::TestColVersionUpdateRemovePolicy_Errors", "collection_version/updates/replace/materialized_test.go::TestColVersionUpdateReplaceIsMaterialized_GivenFalseAndCollection_Errors"]

Changing `IsBranchable` either way fails with `mutating IsBranchable is not supported. Collection: User`; changing `Indexes` with `collection indexes cannot be mutated.`; replacing or removing the policy, its ID or resource with `collection policy cannot be mutated.`; setting `IsMaterialized` false on a plain collection with `non-materialized collections are not supported. Collection: User`.

# `go.collection_version.patch.remove_field`: Removing and replacing fields

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/remove/fields/simple_test.go::TestCollectionVersionUpdatesRemoveField", "collection_version/updates/remove/fields/simple_test.go::TestCollectionVersionUpdatesRemoveAllFields", "collection_version/updates/replace/field/simple_test.go::TestCollectionVersionUpdatesReplaceField"]
- **Scope:** "skipped under the secondary-index multiplier (which adds @index to every field); removing or replacing an indexed field is unsupported in Go (issue #4722)"

Removing a field (or all fields) or replacing it by a different field creates a version in which the old field is no longer queryable (`Cannot query field "name" on type "Users".`) while other fields still are.

# `go.collection_version.patch.field_mutation_errors`: Existing field properties are immutable

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/remove/fields/simple_test.go::TestCollectionVersionUpdatesRemoveFieldNameErrors", "collection_version/updates/remove/fields/simple_test.go::TestCollectionVersionUpdatesRemoveFieldKindErrors", "collection_version/updates/remove/fields/simple_test.go::TestCollectionVersionUpdatesRemoveFieldTypErrors", "collection_version/updates/replace/fields_test.go::TestColVersionUpdateReplaceFields_Errors", "collection_version/updates/replace/fields_test.go::TestColVersionUpdateReplaceDefaultValue_Errors", "collection_version/updates/remove/contraints_test.go::TestColVersionUpdate_FieldFieldSizeContraint_ShouldError", "collection_version/updates/replace/contraints_test.go::TestColVersionUpdate_ReplaceFieldSizeContraint_ShouldError"]

Removing or replacing a property of an existing field (name, kind, CRDT type, default value, size constraint) fails with `mutating an existing field is not supported` (with `ProposedName: ...`); replacing the whole fields list with an invalid entry fails with `no type found for given name. Type: 0`.

# `go.collection_version.patch.embeddings`: Patching vector embeddings

- **Parents:** ["go.collection_version.patch"]

Adding, replacing and removing `VectorEmbeddings` entries of a collection by patch; invalid entries fail with one error line per bad entry.

# `go.collection_version.patch.embeddings.valid`: Valid embedding patches apply

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Go-Tests:** ["collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbedding_ShouldSucceed", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingURL_ShouldSucceed", "collection_version/updates/replace/embeddings_test.go::TestColVersionUpdate_ReplaceVectorEmbeddingParams_ShouldSucceed", "collection_version/updates/remove/embeddings_test.go::TestColVersionUpdate_RemoveVectorEmbedding_ShouldSucceed"]

Adding an embedding (a missing URL is allowed), replacing its parameters, or removing it by patch succeeds and is reflected in the `VectorEmbeddings` returned by `GetCollections`.

# `go.collection_version.patch.embeddings.missing_settings`: Embedding settings are required

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Go-Tests:** ["collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingFieldName_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingFieldName_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingFields_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingFields_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingProvider_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingProvider_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnsupportedProvider_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnsupportedProvider_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingModel_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithMissingModel_ShouldErrorMultiple"]

An added embedding without a field name fails with `embedding FieldName cannot be empty`, without source fields with `embedding Fields cannot be empty`, without a provider with `embedding Provider cannot be empty` (followed by `unknown embedding provider. Provider: `), with an unknown provider with `unknown embedding provider. Provider: deepseek`, and without a model with `embedding Model cannot be empty`.

# `go.collection_version.patch.embeddings.field_references`: Embedding field references are validated

- **Parents:** ["go.collection_version.patch.embeddings"]
- **Go-Tests:** ["collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnknownFieldName_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnknownFieldName_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnknownEmbeddingGenerationField_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithUnknownEmbeddingGenerationField_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingWithInvalidEmbeddingGenerationFieldKind_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToSelf_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToSelf_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToAnotherEmbedding_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToAnotherEmbedding_ShouldErrorMultiple", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToAnotherEmbeddingInPatch_ShouldError", "collection_version/updates/add/embeddings_test.go::TestColVersionUpdate_AddVectorEmbeddingReferenceToAnotherEmbeddingInPatch_ShouldErrorMultiple", "collection_version/updates/replace/embeddings_test.go::TestColVersionUpdate_ReplaceVectorEmbeddingWithUnknownFieldName_ShouldError", "collection_version/updates/replace/embeddings_test.go::TestColVersionUpdate_ReplaceVectorEmbeddingWithUnknownEmbeddingGenerationField_ShouldError", "collection_version/updates/replace/embeddings_test.go::TestColVersionUpdate_ReplaceVectorEmbeddingWithInvalidEmbeddingGenerationFieldKind_ShouldError"]

An added or replaced embedding naming an unknown vector field fails with `the given field does not exist. Vector field: foo`, an unknown source field with `the given field does not exist. Embedding generation field: foo`, a JSON source with `invalid field type for vector embedding generation. Actual: JSON`, and a source that is itself or another embedding field (existing or added in the same patch) with `embedding fields cannot refer to self or another embedding field. Field: name_v`.

# `go.collection_version.patch.test_op`: JSON Patch `test` operations

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesTestAddField", "collection_version/updates/test/add_field_test.go::TestCollectionVersionUpdatesTestAddFieldBlockedByTest", "collection_version/updates/test/name_test.go::TestColVersionUpdateTestNameByVersionID", "collection_version/updates/test/name_test.go::TestColVersionUpdateTestNameByVersionID_Fails", "collection_version/updates/test/name_test.go::TestColVersionUpdateTestName", "collection_version/updates/test/name_test.go::TestColVersionUpdateTestName_Fails", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldNameErrors", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldNamePasses", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldErrors", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldPasses", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldPasses_UsingFieldNameAsIndex", "collection_version/updates/test/field/simple_test.go::TestCollectionVersionUpdatesTestFieldPasses_TargettingKindUsingFieldNameAsIndex"]

A `test` op on a collection name, field, field name or kind (addressed by version ID, collection name, field index or field name) lets the patch apply when it matches; when it does not, the whole patch fails with `test failed` (e.g. `testing value /<versionID>/Name failed: test failed`) and no field is added.

# `go.collection_version.patch.remove_collection`: Removing collection versions by patch

- **Parents:** ["go.collection_version.patch"]

`remove` of a whole collection (by name or version ID) deletes that collection version.

# `go.collection_version.patch.remove_collection.basic`: Removed collections disappear from the API

- **Parents:** ["go.collection_version.patch.remove_collection"]
- **Go-Tests:** ["collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollections_ByID", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollections_ByName", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollections_AddDocToRemovedCollection", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollection_ThenAddSameName_IsFreshCollection", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollection_ThenAddSameShape_IsFreshCollection"]

After removing an empty collection its query, mutations and subscription are gone (`Cannot query field "Users" on type "Query".`, `Cannot query field "add_Users" on type "Mutation".`), adding a document fails with `collection not found`, and re-adding a collection with the same name or shape yields a fresh collection without the old fields or documents.

# `go.collection_version.patch.remove_collection.with_documents`: Collections holding documents cannot be removed

- **Parents:** ["go.collection_version.patch.remove_collection"]
- **Go-Tests:** ["collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollectionWithData", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollectionWithSoftDeletedData", "collection_version/updates/remove/collections_txn_test.go::TestColVersionUpdateRemoveCollection_WithDataAddedInSameTxn"]

Removing a collection that has documents, soft-deleted ones included or ones added earlier in the same transaction, fails with `cannot delete a collection that has documents, first delete the documents and then delete the version`.

# `go.collection_version.patch.remove_collection.version_chain`: Removing versions within a history

- **Parents:** ["go.collection_version.patch.remove_collection"]
- **Go-Tests:** ["collection_version/updates/remove/collections_test.go::TestColVersionUpdateCopyCollectionAddFieldRemoveOriginalCollection", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveOriginalCollection_SamePatch", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveOriginalCollection_DifferentPatches", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveNewCollection_DifferentPatches", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveNewCollectionAndActivateOriginal", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveMultipleNewCollection_FirstAndLast", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveMultipleNewCollection_FirstAndMiddle", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateAddFieldRemoveMultipleNewCollection_MiddleAndLast"]

A version may be removed only if no newer version derives from it: removing the original in the same patch that adds a field succeeds, but from a later patch (or removing first-and-last, first-and-middle) fails with `cannot delete a version that is used by a newer version, first delete the new version`. Removing the newest version(s) succeeds; the previous version can then be reactivated and still serves its documents.

# `go.collection_version.patch.remove_collection.relations`: Removing related collections

- **Parents:** ["go.collection_version.patch.remove_collection"]
- **Go-Tests:** ["collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollection_ReferencedByRelation_ReturnsError", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollection_ReferencedByRelation_OtherSide_ReturnsError", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveBothRelatedCollections_Succeeds"]

Removing one side of a relation fails with `cannot remove a collection while another field references it`, from either side; removing both related collections in one patch succeeds.

# `go.collection_version.patch.remove_collection.concurrency`: Removal under concurrent transactions

- **Parents:** ["go.collection_version.patch.remove_collection"]
- **Go-Tests:** ["collection_version/updates/remove/collections_txn_test.go::TestColVersionUpdateRemoveCollection_DeadlocksIfOtherTxnWriting", "collection_version/updates/remove/collections_txn_test.go::TestColVersionUpdateRemoveCollection_DeadlockIfDeletingVersionWithNewFieldWhilstOtherTxnWriting", "collection_version/updates/remove/collections_txn_test.go::TestColVersionUpdateRemoveCollection_GetCollectionsShouldNotReturnCollectionDeletedWhilstTxnWasOpen", "collection_version/updates/remove/collections_txn_test.go::TestColVersionUpdateRemoveCollection_CollectionMayBeRedeclaredAndUsedByTxn", "collection_version/updates/remove/collections_test.go::TestColVersionUpdateRemoveCollections_ConcurrentWrite", "collection_version/updates/remove/view_txn_test.go::TestColVersionUpdateRemoveView_DoesNotDeadlockIfDeletingVersionWithNoNewFieldsWhilstOtherTxnReading"]
- **Scope:** "database types: Badger in-memory, Badger file and Defra in-memory only (not LevelDB); ConcurrentWrite also Go client only and skipped when the create finishes first"
- **Go-Bug-Suspect:** "Two tests assert that removal deadlocks ('t.Fatal('Test should deadlock but did not')') when another transaction is writing; the deadlock is pinned as expected behaviour, and ConcurrentWrite notes explicit-transaction coverage is missing (Go issue #4476)."

Removing a collection while another transaction writes to it blocks (the Go test asserts a deadlock until the other transaction ends), with or without a new field; a collection removed while a transaction was open is not returned by that transaction's later `GetCollections`, a removed collection may be redeclared and used by the transaction, a concurrent create either wins or the removal reports `cannot delete a collection that has documents`, and removing a view while another transaction reads it does not deadlock.

# `go.collection_version.patch.remove_view`: Removing views by patch

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveView", "collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveNonMaterializedViewWithData", "collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveMaterializedViewWithUnrefreshedData", "collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveMaterializedViewWithRefreshedData", "collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveCollectionBackingUnmaterializedView", "collection_version/updates/remove/view_test.go::TestColVersionUpdateRemoveCollectionBackingMaterializedView"]
- **Scope:** "the refreshed-data and backing-materialized-view tests run only with materialized views"

A view's collection can be removed by patch, including a cacheless view over existing data and a materialized view not yet refreshed; a refreshed materialized view fails with `cannot delete a collection that has documents, first delete the documents and then delete the version`. Removing the collection backing a view makes querying the view fail with `collection not found` (cacheless) or its refresh/query fail with `collection not found` (materialized).

# `go.collection_version.patch.view_sources`: Patching a view's query source

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/replace/query_source_query_test.go::TestColVersionUpdateReplaceQuerySourceQuery", "collection_version/updates/replace/query_source_query_test.go::TestColVersionUpdateReplaceQuerySourceQueryName", "collection_version/updates/replace/query_source_transform_test.go::TestColVersionUpdateReplaceQuerySourceTransform"]

Replacing a view's source query, the queried collection name or its Lens transform by patch changes what the view returns on the next query.

# `go.collection_version.patch.dematerialize`: Changing view materialization and policy

- **Parents:** ["go.collection_version.patch"]
- **Go-Tests:** ["collection_version/updates/replace/materialized_test.go::TestColVersionUpdateReplaceIsMaterialized_GivenFalseAndView_ErrorsIfNotTruncated", "collection_version/updates/replace/materialized_test.go::TestColVersionUpdateReplaceIsMaterialized_GivenFalseAndView", "collection_version/updates/replace/view_policy_test.go::TestColVersionUpdateReplaceIsMaterialized_GivenPolicyOnNonMAterializedView_Errors"]
- **Scope:** "ErrorsIfNotTruncated runs only with materialized views; the policy test only with cacheless views"
- **Go-Bug-Suspect:** "The policy error says 'materialized views do not support ACP' although the test sets the policy on a non-materialized (cacheless) view; the inverse case is untestable pending Go issue #2983."

Setting `IsMaterialized` false on a materialized view with data fails with `cannot dematerialize a materialized view that has data, first truncate it and then try again.`; after truncating it succeeds and the view queries its source directly. Adding a policy to a cacheless view fails with `materialized views do not support ACP. Collection: UserView`.

# `go.collection_version.migrations`: Lens migrations between collection versions

- **Parents:** ["go.collection_version"]

Registering Lens WASM transforms and applying them so documents written at one version are read at another.

# `go.collection_version.migrations.lens_registry`: Lens registry

- **Parents:** ["go.collection_version.migrations"]
- **Go-Tests:** ["collection_version/migrations/add_lens_test.go::TestAddLens_WithSimpleLens_CanBeListedBack", "collection_version/migrations/add_lens_test.go::TestAddLens_WithMultipleLenses_ReturnsUniqueLensIDs", "collection_version/migrations/add_lens_test.go::TestAddLens_WithIdenticalLenses_ReturnsSameCID", "collection_version/patch_migration_numeric_fidelity_test.go::TestCollectionVersionPatch_LargeIntegerLensArguments_AreNotDeduplicated"]
- **Scope:** "numeric fidelity test: Go, HTTP, CLI and C clients only (not JS), because JS numbers are float64 (Go issue #5176)"

`AddLens` stores a lens config and returns its content ID: `ListLenses` lists it back, different lenses get distinct IDs, identical lenses get the same CID, and lenses whose arguments differ only beyond float64 precision (2^53 and 2^53+1) stay distinct.

# `go.collection_version.migrations.patch_transform`: Transforms attached by patch

- **Parents:** ["go.collection_version.migrations"]
- **Go-Tests:** ["collection_version/migrations/add_lens_test.go::TestAddLens_WithPatchCollection_TransformsDocuments", "collection_version/patch_migration_presence_test.go::TestCollectionVersionPatch_NoneMigration_NoTransformRecorded", "collection_version/patch_migration_presence_test.go::TestCollectionVersionPatch_SomeEmptyMigration_TransformRecorded", "collection_version/updates/replace/col_source_transform_test.go::TestColVersionUpdateReplaceCollectionSourceTransform", "collection_version/updates/remove/col_source_transform_test.go::TestColVersionUpdateRemoveCollectionSourceTransform"]
- **Scope:** "the remove-transform test is skipped under the secondary-index multiplier (which adds @index to every field) (Go issue #4722)"

A patch may carry a Lens transform for the new version, which then transforms documents on read; a patch with no migration records no transform, while a present-but-empty lens records the empty lens's ID. A collection source's transform may be replaced or removed by patch, changing or dropping the migration applied on read.

# `go.collection_version.migrations.configure`: Configuring migrations

- **Parents:** ["go.collection_version.migrations"]
- **Go-Tests:** ["collection_version/migrations/simple_test.go::TestCollectionMigrationDoesNotErrorGivenUnknownCollectionRoots", "collection_version/migrations/simple_test.go::TestCollectionMigrationGetMigrationsReturnsMultiple", "collection_version/migrations/simple_test.go::TestCollectionMigrationReplacesExistingMigationBasedOnSourceID", "collection_version/migrations/simple_test.go::TestCollectionMigration_ConfigureMigrationSkippingVersion_Errors", "collection_version/migrations/with_txn_test.go::TestCollectionMigrationGetMigrationsWithTxn"]

`ConfigureMigration` (`client schema migration set`) records a transform between two versions, even for unknown versions; several are returned by `GetCollections`, a second one with the same source replaces the first, a migration set in a transaction is visible inside it, and one between non-adjacent versions fails with `cannot migrate between non-adjacent collection versions`.

# `go.collection_version.migrations.query`: Migrations applied on query

- **Parents:** ["go.collection_version.migrations"]

Querying documents written at another version returns them transformed to the active version.

# `go.collection_version.migrations.query.forward`: Forward migrations across versions

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithSingleMigration_AppliesMigration", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMultipleDocs_AppliesMigrationToAll", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationRegisteredBeforePatch_AppliesMigration", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationOnlyOnFirstStep_AppliesOnlyFirstStep", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationOnlyOnLastStep_AppliesOnlyLastStep", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationsRegisteredBeforePatches_AppliesAllMigrations", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationsRegisteredBeforePatchesInReverseOrder_AppliesAllMigrations", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithUnknownCollectionVersionIDs_IgnoresMigration"]

A migration from version 1 to 2 is applied to every document written at version 1, whether registered before or after the patch; with several versions each registered step is applied in order (registered in any order), a missing step is skipped (no transform for that edge), and migrations naming unknown versions are ignored.

# `go.collection_version.migrations.query.field_mapping`: Migrated field values

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationMutatingScalarField_AppliesMutation", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationMutatingInlineArrayField_AppliesMutation", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationRemovingField_ReturnsNilForRemovedField", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationMutatingFieldNotInFirstRequest_PreservesUnmutatedFieldInSecondRequest", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationCopyingFieldWhenSrcFieldNotRequested_AppliesCopy", "collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationCopyingFieldWhenSrcAndDstFieldNotRequested_AppliesCopy"]

Migrations that set a scalar, mutate an inline array, remove a field (returned as `null`) or copy a field produce those values even when the source or destination field is not requested, and a later request for a different field set still sees unmutated fields.

# `go.collection_version.migrations.query.incremental`: Migrations registered one edge at a time

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/simple_test.go::TestCollectionMigrationQuery_WithMigrationsAcrossMultipleVersions_AppliesAllMigrations", "collection_version/migrations/query/with_incremental_migrations_test.go::TestCollectionMigrationQuery_SparseChainWithIndexAndForwardMigration_ShouldMigrateAndReindexCorrectly", "collection_version/migrations/query/with_incremental_migrations_test.go::TestCollectionMigrationQuery_ThreeIncrementalMigrations_ShouldMigrateAcrossAllThree", "collection_version/migrations/query/with_incremental_migrations_test.go::TestCollectionMigrationQuery_IntermediateMigrationRegisteredLast_ShouldProduceFullyMigratedResults", "collection_version/migrations/query/with_incremental_migrations_test.go::TestCollectionMigrationQuery_IntermediateStampSurvivesRestart_ShouldRemigrateAfterRestart", "collection_version/migrations/query/with_set_default_test.go::TestCollectionMigrationQuery_WithSetDefaultToOriginalVersionThatDocWasAddedAt_ClearsMigrations"]
- **Scope:** "all but the restart test are skipped under the secondary-index multiplier (which adds @index to every field)"
- **Go-Bug-Suspect:** "Go issue #4736: the reindex doc-version stamp is not invalidated when a migration is registered on a previously nil edge, so under the secondary-index multiplier Go returns stale values; the tests assert the correct result but are skipped there."

When migrations for successive edges are registered one by one (sparse chains with an index, three or more steps, an intermediate edge registered last, across a restart) or the active version is reset to the one a document was written at, queries return the fully migrated (or un-migrated) values for the current active version.

# `go.collection_version.migrations.query.active_version`: Migrations follow the active version

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_set_default_test.go::TestCollectionMigrationQuery_WithSetDefaultToLatest_AppliesForwardMigration", "collection_version/migrations/query/with_set_default_test.go::TestCollectionMigrationQuery_WithSetDefaultToOriginal_AppliesInverseMigration", "collection_version/migrations/query/with_inverse_test.go::TestCollectionMigrationQueryInversesAcrossMultipleVersions", "collection_version/migrations/query/with_collection_branch_test.go::TestCollectionMigrationQuery_WithBranchingCollection"]

Activating the latest version applies forward migrations; activating an older version applies inverse migrations, including across several versions; querying a branch of the version tree returns documents migrated along that branch.

# `go.collection_version.migrations.query.filters`: Filtering and docID lookup over migrated documents

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_filter_test.go::TestCollectionMigrationQuery_WithFilter_ShouldFilterFMigration", "collection_version/migrations/query/with_filter_test.go::TestCollectionMigrationQuery_WithFilterAndMigrationBetweenOldVersions_ShouldApplyMigration", "collection_version/migrations/query/with_filter_test.go::TestCollectionMigrationQuery_WithFilterAndMigrationInOldPatch_ShouldApplyMigration2", "collection_version/migrations/query/with_doc_id_test.go::TestCollectionMigrationQueryByDocID", "collection_version/migrations/query/with_doc_id_test.go::TestCollectionMigrationQueryMultipleQueriesByDocID"]

Filters are evaluated against migrated values (also when the migration sits between older versions or in an older patch), and querying by one or several `docID`s returns migrated documents without hanging.

# `go.collection_version.migrations.query.index`: Secondary indexes over migrated documents

- **Parents:** ["go.collection_version.migrations.query"]

How secondary indexes interact with migrations.

# `go.collection_version.migrations.query.index.basic`: Indexes hold migrated values

- **Parents:** ["go.collection_version.migrations.query.index"]
- **Go-Tests:** ["collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithIndexOnNotMigratedDocs_ShouldNotHinder", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithIndexOnMigratedField_ShouldUseIndexWithMigratedValues", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithIndexOnMigratedFieldAndSettingOldVersionAsActive_ShouldUseIndexWithOldValues", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithIndexAppliedAfterMigration_ShouldIndexDocsOnLatestVersion", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithIndexAppliedAfterSetActiveVersion_ShouldIndexDocsOnActiveVersion"]

Index lookups on migrated fields use the migrated values for the active version (old values if an older version is activated), an index created after a migration or version switch indexes the active version's values, and un-migrated documents do not hinder index queries.

# `go.collection_version.migrations.query.index.reindex`: Reindexing on version switch or migration change

- **Parents:** ["go.collection_version.migrations.query.index"]
- **Go-Tests:** ["collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_SwitchToOldDistantVersionWithNoMigrations_ShouldNotReindex", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_SwitchToNewDistantVersionWithNoMigrations_ShouldNotReindex", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_SwitchToOldDistantVersionWithMigrationInBetween_ShouldReindexWithOldValues", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_SwitchToNewDistantVersionWithMigrationInBetween_ShouldReindexWithMigratedValues", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_ApplyingMigrationBetweenOldVersions_ShouldReindex", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_ApplyingMigrationBetweenNewVersions_ShouldNotReindex", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_ApplyingMigrationToUnknownVersionsThenPatch_ShouldReindex", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_ApplyingMigrationWithPatching_ShouldReindex"]

Switching to a distant version reindexes only if a migration lies in between (old or migrated values accordingly); registering a migration between versions reindexes when it affects the active version (also when the version is created later by patch, or when the migration comes with the patch) and not when it lies between newer inactive versions.

# `go.collection_version.migrations.query.index.branches`: Indexes over branched versions

- **Parents:** ["go.collection_version.migrations.query.index"]
- **Go-Tests:** ["collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithBranchedVersionsAndMigration_ShouldApplyMigrationCorrectly", "collection_version/migrations/query/with_index_test.go::TestCollectionMigrationQuery_WithThreeBranchedVersions_ShouldApplyCorrectMigrationPerBranch"]

With branched version histories each carrying its own migration, switching between branches reindexes so indexed queries return the values migrated along the active branch (e.g. age 20, 25 or 30 for three branches).

# `go.collection_version.migrations.query.p2p`: Migrations on replicated documents

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_p2p_test.go::TestCollectionMigrationQueryWithP2PReplicatedDocAtOlderSchemaVersion", "collection_version/migrations/query/with_p2p_test.go::TestCollectionMigrationQueryWithP2PReplicatedDocAtMuchOlderSchemaVersion", "collection_version/migrations/query/with_p2p_test.go::TestCollectionMigrationQueryWithP2PReplicatedDocAtNewerSchemaVersion", "collection_version/migrations/query/with_p2p_test.go::TestCollectionMigrationQueryWithP2PReplicatedDocAtMuchNewerSchemaVersionWithSchemaHistoryGap", "collection_version/migrations/query/with_p2p_collection_branch_test.go::TestCollectionMigrationQueryWithP2PReplicatedDocOnOtherCollectionBranch"]

A document replicated from a peer at an older, much older or newer collection version (also across a version-history gap or on another branch) is returned by the receiving node migrated up or down to its own active version, while the sender returns it as written.

# `go.collection_version.migrations.query.restart`: Migrations survive restart

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_restart_test.go::TestCollectionMigrationQueryWithRestart", "collection_version/migrations/query/with_restart_test.go::TestCollectionMigrationQueryWithRestartAndMigrationBeforePatchCollection"]

Migrations configured before or after the patch are still applied to queries after the node restarts.

# `go.collection_version.migrations.query.txn`: Migrations inside transactions

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_txn_test.go::TestCollectionMigrationQueryWithTxn", "collection_version/migrations/query/with_txn_test.go::TestCollectionMigrationQueryWithTxnAndCommit"]

A migration configured in a transaction applies to queries in that transaction, and after commit to queries outside it.

# `go.collection_version.migrations.query.update`: Migrations and updates

- **Parents:** ["go.collection_version.migrations.query"]
- **Go-Tests:** ["collection_version/migrations/query/with_update_test.go::TestCollectionMigrationQueryWithUpdateRequest", "collection_version/migrations/query/with_update_test.go::TestCollectionMigrationQueryWithMigrationRegisteredAfterUpdate"]

An `update_` mutation on a document written at an older version returns migrated values, and a migration registered after the document was updated still applies.

# `go.collection`: Collection lifecycle operations

- **Parents:** ["go"]

Operations on whole collections: relating new collections to existing ones, deleting collections, truncating them, and listing background refresh actions.

# `go.collection.relations_to_existing`: New collections relating to existing ones

- **Parents:** ["go.collection"]
- **Go-Tests:** ["collection/add_relation_with_collection_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithAddCollectionCreatingOneToManyRelationToExistingCollection_ShouldSucceed", "collection/add_relation_with_collection_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithAddCollectionCreatingOneToManyRelationsToMultipleExistingCollections_ShouldSucceed", "collection/add_relation_with_collection_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithPatchAddingOneToManyRelationAfterSeparateAddCollections_ShouldSucceed", "collection/add_relation_with_collection_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithMixedBatchHavingRelationToExistingAndNewCollections_ShouldSucceed", "collection/add_relation_with_collection_test.go::TestCollectionVersionUpdatesAddFieldKindForeignObject_WithChainedOneToManyRelationsAcrossSeparateCollections_ShouldSucceed"]

`AddCollection` with a type that declares a one-to-many relation to one or several already-existing collections (or a mix of existing and new ones, or a chain across separate adds), or a later patch adding such a relation, succeeds and documents can be created and queried across the relation.

# `go.collection.delete`: Deleting collections

- **Parents:** ["go.collection"]

`DeleteCollection` (`client collection delete`) removes collections by name.

# `go.collection.delete.basic`: Deleting empty collections

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_Simple_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_Simple_QueriesNoLongerWork", "collection/delete/delete_collection_test.go::TestDeleteCollection_NonExistentCollection_ReturnsError", "collection/delete/delete_collection_test.go::TestDeleteCollection_MultipleCollections_DeleteOne", "collection/delete/delete_collection_test.go::TestDeleteCollection_DeleteBothCollections_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_MultipleCollections_AtomicallyInOneCall_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_MixedValidAndInvalidName_RollsBack", "collection/delete/delete_collection_test.go::TestDeleteCollection_NameContainsEmptyString_ReturnsError", "collection/delete/delete_collection_test.go::TestDeleteCollection_DuplicateNames_DedupesAndSucceeds"]

Deleting one or several empty collections removes them from `GetCollections` and from GraphQL (`Cannot query field "Users" on type "Query".`, `Cannot query field "add_Users" on type "Mutation".`), atomically for a multi-name call. Unknown names fail with `collection not found` and roll back the whole call; an empty name fails with `collection name can't be empty`; repeated names are deduplicated.

# `go.collection.delete.with_documents`: Collections with documents cannot be deleted

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_WithDocuments_ReturnsError", "collection/delete/delete_collection_test.go::TestDeleteCollection_WithSoftDeletedDocuments_ReturnsError"]

Deleting a collection that has documents, soft-deleted ones included, fails with `cannot delete a collection that has documents`.

# `go.collection.delete.transactions`: Deleting collections in a transaction

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_WithTransaction_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_MultipleCollections_WithTransactionWithoutCommit_StillExist"]
- **Scope:** "the uncommitted-transaction test runs only on Badger file, Badger in-memory and Defra in-memory (LevelDB lacks concurrent transactions, Go issue #4442)"

A delete in a committed transaction removes the collections; before commit, other transactions still see them in `GetCollections`.

# `go.collection.delete.relations`: Deleting related collections

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_ReferencedByRelation_ReturnsError", "collection/delete/delete_collection_test.go::TestDeleteCollection_ReferencedByRelation_OtherSide_ReturnsError", "collection/delete/delete_collection_test.go::TestDeleteCollection_BothRelatedCollections_ActiveOnly_InOneCall_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_BothRelatedCollections_InOneCall_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_RelatedPair_QueriesAgainstSurvivor", "collection/delete/delete_collection_test.go::TestDeleteCollection_SelfReferencingOneToOne_Default_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_SelfReferencingOneToMany_Default_Succeeds", "collection/delete/delete_collection_test.go::TestDeleteCollection_SelfReferencing_ActiveOnly_LeavesEarlierVersion"]

Deleting one side of a relation fails with `cannot remove a collection while another field references it` from either side; deleting both sides in one call succeeds (default or active-only), the survivor of a pair remains queryable, and self-referencing one-to-one and one-to-many collections can be deleted (active-only leaves the earlier version).

# `go.collection.delete.versions`: Which versions a delete removes

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_Default_RemovesAllVersions", "collection/delete/delete_collection_test.go::TestDeleteCollection_BackToBackPatchAndDelete_AllVersionsRemoved", "collection/delete/delete_collection_test.go::TestDeleteCollection_ActiveOnly_LeavesEarlierVersion", "collection/delete/delete_collection_test.go::TestDeleteCollection_Default_MultipleCollectionsWithMultipleVersions_RemovesAllVersions", "collection/delete/delete_collection_test.go::TestDeleteCollection_ActiveOnly_FollowedBySetActive_RestoresQueries", "collection/delete/delete_collection_test.go::TestDeleteCollection_UnrelatedCollectionWithMultipleVersions_PreservedAfterDelete"]

By default a delete removes every version of the collection (also right after a patch and activation, and for several multi-version collections at once); with the active-only option it removes only the active version, leaving earlier ones that can be reactivated with `SetActiveCollectionVersion`. Unrelated collections and their versions are untouched.

# `go.collection.delete.recreate`: Re-adding a deleted collection

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_ThenAddSameName_IsFreshCollection", "collection/delete/delete_collection_test.go::TestDeleteCollection_MultiName_ThenAddSameNames_AreFreshCollections", "collection/delete/delete_collection_test.go::TestDeleteCollection_ActiveOnly_SingleVersion_ThenAddSameName_IsFreshCollection", "collection/delete/delete_collection_test.go::TestDeleteCollection_PatchedThenDefaultDeleted_ThenAddSameName_IsFreshCollection", "collection/delete/delete_collection_test.go::TestDeleteCollection_ThenAddSameShape_IsFreshCollection"]

Adding a collection with the name (or names, or the same shape) of a deleted one creates a fresh collection: none of the old fields (`Cannot query field "name" on type "Users".`), versions or documents carry over.

# `go.collection.delete.dependents`: Indexes and views of deleted collections

- **Parents:** ["go.collection.delete"]
- **Go-Tests:** ["collection/delete/delete_collection_test.go::TestDeleteCollection_WithSecondaryIndex_RemovesIndex", "collection/delete/delete_collection_test.go::TestDeleteCollection_WithViewDepending_ViewBecomesStale", "collection/delete/delete_collection_test.go::TestDeleteCollection_WithMaterializedViewDepending_RefreshReturnsError"]
- **Scope:** "the view tests run only with cacheless views and materialized views respectively"

Deleting a collection removes its secondary indexes. A view over it does not block the delete: querying a cacheless view afterwards fails with `collection not found`, and refreshing a materialized view (whose source was emptied first) fails with `collection not found`.

# `go.collection.truncate`: Truncating collections

- **Parents:** ["go.collection"]

`Truncate` removes documents and their blocks from a collection while keeping its definition.

# `go.collection.truncate.basic`: Truncating a collection

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/simple_test.go::TestTruncateCollection", "collection/truncate/simple_test.go::TestTruncateCollection_TruncateTwice", "collection/truncate/parallel_test.go::TestTruncateCollectionParallel_DeletesAllPreviouslyExistingDocuments"]

Truncating an empty collection, or the same collection twice, succeeds; truncating while documents are added in parallel deletes every document that existed before the truncate.

# `go.collection.truncate.removes_data`: Truncate removes documents and blocks

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/add_test.go::TestTruncateCollectionAdd_RemovesDocument", "collection/truncate/add_test.go::TestTruncateCollectionAdd_RemovesSignedDocument", "collection/truncate/add_test.go::TestTruncateCollectionAdd_RemovesEncryptedDocument", "collection/truncate/add_test.go::TestTruncateCollectionAdd_RemovesBlocks", "collection/truncate/add_test.go::TestTruncateCollectionAdd_RemovesSharedFieldBlock", "collection/truncate/branchable_add_test.go::TestTruncateCollectionBranchableAdd_RemovesDocument", "collection/truncate/branchable_add_test.go::TestTruncateCollectionBranchableAdd_RemovesBlocks"]

After truncating, plain, signed, encrypted and branchable documents are no longer returned by queries, and their blocks (including shared field blocks and the collection's branchable head) are gone from `_commits`.

# `go.collection.truncate.re_add`: Re-adding documents after truncate

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/add_test.go::TestTruncateCollectionAdd_AddsDocWithSameDocIDAsOriginal", "collection/truncate/add_test.go::TestTruncateCollectionAdd_AddsDocWithSameCIDAsOriginal", "collection/truncate/add_test.go::TestTruncateCollectionAdd_AddsDocWithBlocksAtHeight1"]

A document re-added after truncate gets the same docID and the same head CID as the original, with blocks starting again at height 1.

# `go.collection.truncate.filter`: Truncating selected documents

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/filter_test.go::TestTruncateCollection_WithFilterIsIdempotent", "collection/truncate/filter_test.go::TestTruncateCollection_WithLargeIntegerFilter_RemovesOnlyExactMatch", "collection/truncate/index_filter_test.go::TestTruncateCollectionIndexFilter_RemovesDocument", "collection/truncate/index_filter_test.go::TestTruncateCollectionIndexFilter_WithUniqueIndex_RemovesDocument", "collection/truncate/index_filter_test.go::TestTruncateCollectionIndexFilter_WithUniqueIndex_AllowsRecreationOfDocument"]
- **Scope:** "the large-integer test: Go, HTTP, CLI and C clients with collection save/named mutations only"

A truncate limited to certain documents removes only those (idempotently on retry), matches large integers exactly (2^53 vs 2^53+1), works through plain and unique secondary indexes, and frees a unique value so a document with it can be created again.

# `go.collection.truncate.acp`: Truncate keeps document ACP

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/dac_test.go::TestTruncateCollectionDAC_RemovedPrivateDocumentRetainsPermissions", "collection/truncate/dac_test.go::TestTruncateCollectionDAC_RemovedPublicDocumentRetainsPermissions"]
- **Scope:** "one test is skipped under the signed-documents multiplier, since differently signed re-adds get different docIDs"

Truncating a DAC-protected collection removes the documents but keeps their ACP registrations: a re-added private document stays private to its owner and a public one stays public.

# `go.collection.truncate.views`: Truncate and materialized views

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/view_test.go::TestTruncateCollectionViewAdd_RemovesDocument", "collection/truncate/view_test.go::TestTruncateCollectionViewAdd_TruncatingSourceDoesNotTruncateView"]
- **Scope:** "materialized views only"

Truncating a materialized view empties its cache until the next `RefreshViews` rebuilds it from the source; truncating the source collection leaves the view's cached documents in place.

# `go.collection.truncate.actions`: Truncate in the actions list

- **Parents:** ["go.collection.truncate"]
- **Go-Tests:** ["collection/truncate/list_actions_test.go::TestTruncateCollectionListAction_ListsUncompletedTruncates", "collection/truncate/list_actions_test.go::TestTruncateCollectionListAction_DoesNotListCompletedTruncates"]

`ListActions` shows a truncate while it is running and no longer lists it once complete.

# `go.collection.refresh_actions`: View refresh in the actions list

- **Parents:** ["go.collection"]
- **Go-Tests:** ["collection/refresh/list_actions_test.go::TestRefreshCollectionListAction_ListsUncompletedRefresh", "collection/refresh/list_actions_test.go::TestRefreshCollectionListAction_RecordsErrorAgainstRefreshAction", "collection/refresh/list_actions_test.go::TestRefreshCollectionListAction_DoesNotListCompletedRefreshes"]
- **Scope:** "the uncompleted and failed refresh tests run only with materialized views"

`ListActions` shows an uncompleted view refresh, records a failed refresh with its error (`collection not found`) and no longer lists completed refreshes.

# `go.view`: Views

- **Parents:** ["go"]

`AddView` defines a collection-like view from a GraphQL query plus an SDL type, optionally with a Lens transform; views are queried like collections.

# `go.view.simple`: Querying simple views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/simple_test.go::TestView_Simple", "view/simple/simple_test.go::TestView_SimpleMultipleDocs", "view/simple/simple_test.go::TestView_SimpleWithFieldSubset_ErrorsSelectingExcludedField", "view/simple/simple_test.go::TestView_SimpleWithExtraFieldInViewSDL", "view/simple/simple_test.go::TestView_SimpleWithExtraFieldInViewQuery", "view/simple/simple_test.go::TestView_SimpleViewOfView", "view/simple/with_alias_test.go::TestView_SimpleWithAlias"]

A view over a collection returns the selected fields for every source document; the view SDL may declare fields the query lacks (returned as `null`) and the query may select fields the SDL omits (not exposed), selecting a field outside the SDL fails with `Cannot query field "age" on type "UserView"`, aliases in the view query map onto SDL fields, and a view may be defined over another view.

# `go.view.filter`: Filters in and on views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_filter_test.go::TestView_SimpleWithFilter", "view/simple/with_filter_test.go::TestView_SimpleWithFilterOnViewAndQuery"]

A filter in the view query limits the documents the view returns, and a filter in the request further narrows them.

# `go.view.introspection`: View GraphQL types

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_introspection_test.go::TestView_Simple_GQLIntrospectionTest", "view/one_to_many/with_introspection_test.go::TestView_OneToMany_GQLIntrospectionTest"]

A view's GraphQL type has its SDL fields, aggregates and `_group` but no `_docID`, `_version` or `_deleted`, and a one-to-many view exposes only the declared nested field, not the relation's ID field.

# `go.view.transforms`: Lens transforms on views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_transform_test.go::TestView_SimpleWithTransform", "view/simple/with_transform_test.go::TestView_SimpleWithMultipleTransforms", "view/simple/with_transform_test.go::TestView_SimpleWithTransformReturningMoreDocsThanInput", "view/simple/with_transform_test.go::TestView_SimpleWithTransformReturningFewerDocsThanInput", "view/simple/with_transform_aggregate_test.go::TestView_SimpleWithTransformAggregate", "view/simple/transform_restart_test.go::TestView_SimpleWithTransformAndRestart", "view/one_to_many/with_transform_test.go::TestView_OneToManyWithTransformOnOuter", "view/one_to_many/with_transform_test.go::TestView_OneToManyWithTransformAddingInnerDocs", "view/one_to_one/with_transform_test.go::TestView_OneToOneWithTransformOnOuter"]

A view with one or more Lens transforms returns transformed documents, including transforms that return more or fewer documents than input, aggregate across documents, apply to the outer side of one-to-one and one-to-many views or add inner documents; transforms keep working after a restart.

# `go.view.transform_cid`: Referencing transforms by CID

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_transform_cid_test.go::TestView_WithTransformCID_CanReuseExistingLens", "view/simple/with_transform_cid_test.go::TestView_WithInvalidTransformCID_ReturnsError", "view/simple/transform_cid_presence_test.go::TestAddView_BlankTransformCID_FailsAndCreatesNoView"]

A view can reuse a lens already added by passing its CID; an unknown CID fails with `lens CID not found`, and a blank CID fails with `cid too short` and creates no view.

# `go.view.materialized`: Materialized views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/materialized_test.go::TestView_SimpleMaterialized_AutoUpdatesOnViewAdd", "view/simple/materialized_test.go::TestView_SimpleMaterialized_RefreshesAfterEarlierRefresh", "view/simple/materialized_test.go::TestView_SimpleMaterialized_DoesNotAutoUpdate"]
- **Scope:** "the refresh tests run only with materialized views"

A materialized view is populated when added, does not reflect later source changes until `RefreshViews` is called, and refreshes correctly after an earlier refresh with data.

# `go.view.constraints`: Size constraints are not enforced on views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_contraints_test.go::TestView_SimpleWithSizeConstraint_CacheLessView_DoesNotErrorOnSizeViolation", "view/simple/with_contraints_test.go::TestView_SimpleWithSizeConstraint_MaterializedView_DoesNotErrorOnSizeViolation"]
- **Scope:** "one test per view type: cacheless, materialized"
- **Go-Bug-Suspect:** "The materialized test documents a potential bug: array results come back as interface slices of float64 rather than typed Int/Float32/Float64 arrays (Go issue #3428)."

`@constraints(size: 2)` on view array fields is not enforced: a source array of 3 elements is returned in full by cacheless and materialized views.

# `go.view.source_only_features`: Collection-only features do not run on views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/simple/with_default_value_test.go::TestView_SimpleWithDefaultValue_DoesNotSetFieldValue", "view/simple/with_embeddings_test.go::TestView_SimpleWithEmbeddings_DoesNotGenerateEmbedding"]

A `@default` value in a view SDL is not applied to view results, and an `@embedding` field in a view SDL is not generated.

# `go.view.one_to_many`: One-to-many views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/one_to_many/simple_test.go::TestView_OneToMany", "view/one_to_many/simple_test.go::TestView_OneToManyWithMixedSDL", "view/one_to_many/simple_test.go::TestView_OneToManyFromInnerSide_Errors", "view/one_to_many/simple_test.go::TestView_OneToManyOuterToInnerToOuter_Errors", "view/one_to_many/simple_test.go::TestView_OneToManyWithRelationInQueryButNotInSDL", "view/one_to_many/simple_test.go::TestView_OneToManyMultipleViewsWithEmbeddedSchema", "view/one_to_many/simple_test.go::TestView_OneToManyWithDoubleSidedRelation_Errors", "view/one_to_many/with_alias_test.go::TestView_OneToManyWithAliasOnOuter", "view/one_to_many/with_alias_test.go::TestView_OneToManyWithAliasOnInner"]
- **Go-Bug-Suspect:** "TestView_OneToManyWithDoubleSidedRelation_Errors is named as an error case but asserts a successful view-of-view query."

A view over a one-to-many query returns nested related documents (with mixed SDL, aliases on either side, relations in the query but not the SDL, several views sharing an embedded type, and views over such views). The inner embedded type is not a top-level query (`Cannot query field "BookView" on type "Query".`) and cannot navigate back (`Cannot query field "author" on type "BookView".`).

# `go.view.one_to_many_count`: Counts in one-to-many views

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/one_to_many/with_count_test.go::TestView_OneToManyWithCount_Errors", "view/one_to_many/with_count_test.go::TestView_OneToManyWithAliasedCount", "view/one_to_many/with_count_test.go::TestView_OneToManyWithCountInQueryButNotSDL"]
- **Go-Bug-Suspect:** "The unaliased-count error test partially documents Go issue #2113."

An aliased `_count` in a view query maps to an SDL field and returns the count; a count in the query but not the SDL is dropped; an unaliased `COUNT(books: {})` mapped to SDL field `COUNT` fails at query time with `aggregate must be provided with a property to aggregate`.

# `go.view.one_to_one`: One-to-one views and embedded types

- **Parents:** ["go.view"]
- **Go-Tests:** ["view/one_to_one/identical_schema_test.go::TestView_OneToOneSameSchema", "view/one_to_one/identical_schema_test.go::TestView_OneToOneEmbeddedSchemaIsNotLostOnNextUpdate", "view/one_to_one/simple_test.go::TestView_OneToOneDuplicateEmbeddedSchema_Errors", "view/one_to_one/with_restart_test.go::TestView_OneToOneEmbeddedSchemaIsNotLostORestart"]
- **Go-Bug-Suspect:** "TestView_OneToOneSameSchema carries a TODO: the setup works but prevents querying the right-hand type as the primary return object."

A one-to-one view whose SDL reuses one type for both sides returns nested documents; an embedded type survives a later view update and a restart; declaring an embedded type a second time in another view fails with `collection already exists. Name: BookView`.

