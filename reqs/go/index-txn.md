# `go.index`: Secondary indexes

- **Parents:** ["go"]

Secondary indexes declared with `@index` or the index API: their definition, build, use by the query planner and maintenance on writes, merges and deletes.

# `go.index.definition`: Index definition and lifecycle

- **Parents:** ["go.index"]

Creating, listing, deleting and inspecting index definitions.

# `go.index.definition.create`: Create an index through the index API

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/new_test.go::TestIndexNew_IfGivenSameIndexName_ShouldReturnError", "index/new_test.go::TestIndexNew_IfInvalidIndexName_ReturnError", "index/new_test.go::TestIndexNew_ShouldNotHinderQuerying", "index/new_test.go::TestIndexNewWithCollection_ShouldNotHinderQuerying"]

`NewIndex` on an existing collection adds the index without changing query results. A duplicate name fails with `index with name already exists` (Name: `age_index`) and an invalid name such as `!` fails with `index with invalid name`.

# `go.index.definition.list`: List indexes

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/get_test.go::TestIndexList_IfThereAreNoIndexes_ReturnEmptyList", "index/new_get_test.go::TestIndexList_GetIndexesForACollection_ReturnCollectionSpecificList", "index/new_get_test.go::TestIndexList_ShouldReturnListOfExistingIndexes", "index/list_collection_name_test.go::TestIndexList_WithCollectionScope_ResultsNameTheirCollection", "index/list_collection_name_test.go::TestIndexList_WithMultipleCollections_EachResultNamesItsOwnCollection"]

`ListIndexes` returns an empty list when no index exists, every existing index otherwise, and only the given collection's indexes when scoped to one. Each listed index names the collection it belongs to, also in a multi-collection listing.

# `go.index.definition.delete`: Delete an index

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/delete_test.go::TestIndexDelete_IfIndexDoesNotExist_ReturnError", "index/delete_test.go::TestIndexDelete_ShouldNotHinderQuerying", "index/delete_test.go::TestIndexDelete_ShouldRemoveIndexFromCollection"]

`DeleteIndex` removes the index from the collection's listing and queries keep returning the same results. Deleting an unknown name fails with `index with name doesn't exists. Name: non_existing_index`.

# `go.index.definition.composite-directive`: Composite index directive and directions

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/new_composite_test.go::TestCompositeIndexNew_UsingFieldDirective_ImplicitlyAddsField", "index/new_composite_test.go::TestCompositeIndexNew_UsingFieldDirective_OverridesDefaultDirection", "index/new_composite_test.go::TestCompositeIndexNew_UsingFieldDirective_SetsDefaultDirection", "index/new_composite_test.go::TestCompositeIndexNew_UsingFieldDirective_WithExplicitIncludes_RespectsOrder", "index/new_composite_test.go::TestCompositeIndexNew_UsingObjectDirective_OverridesDefaultDirection", "index/new_composite_test.go::TestCompositeIndexNew_UsingObjectDirective_SetsDefaultDirection", "index/new_composite_test.go::TestCompositeIndexNew_WhenAdded_CanRetrieve"]
- **Go-Bug-Suspect:** "Go names the index 'User_name_ASC' even when its first field is descending; the tests carry the comment 'this should be User_name_DESC'."

A type-level `@index(includes: [...])`, or a field-level `@index(includes: [...])` that implicitly adds its own field first, creates one composite index whose field list and per-field `Descending` flags follow `direction` (default and per-field overrides) and the `includes` order, as reported by `ListIndexes`.

# `go.index.definition.request-config`: Index request kind configs and deprecated spellings

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/new_unique_test.go::TestUniqueIndexNew_SetThroughOrderedConfig_ShouldSucceed", "index/new_unique_test.go::TestUniqueIndexNew_BothSpellingsAgree_ShouldSucceed", "index/new_unique_test.go::TestUniqueIndexNew_SpellingsDisagree_ShouldError", "index/new_unique_test.go::TestIndexNew_BothOrderedAndVectorConfig_ShouldError", "index/vector_params_test.go::TestIndexNew_OrderedFieldsDisagreeWithDeprecated_IsRejected", "index/vector_params_test.go::TestIndexNew_FieldsDisagreeBetweenSpellings_IsRejected"]
- **Scope:** "client types: go, http for all but TestIndexNew_FieldsDisagreeBetweenSpellings_IsRejected (the CLI and C clients cannot express both spellings)"

An index request may set the deprecated `Unique`/`Fields` or the kind configs (`Ordered`, `Vector`). `Ordered{Unique: true}` alone or agreeing with `Unique: true` creates an ordered unique index (`Kind: ordered`). Disagreement fails: `index request sets both the deprecated unique field and an ordered config that disagrees with it`, `index request has more than one kind config`, or `index request sets both the deprecated fields and a kind config naming different fields`.

# `go.index.definition.accumulated-crdt`: Counter CRDT fields cannot be indexed

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/crdt_test.go::TestIndexNew_WithCollectionLevelIndexOnPNCounter_ShouldError", "index/crdt_test.go::TestIndexNew_WithCompositeIndexIncludingPNCounter_ShouldError", "index/crdt_test.go::TestIndexNew_WithLWWField_ShouldSucceed", "index/crdt_test.go::TestIndexNew_WithPCounterField_ShouldError", "index/crdt_test.go::TestIndexNew_WithPCounterFieldViaDirective_ShouldError", "index/crdt_test.go::TestIndexNew_WithPNCounterField_ShouldError", "index/crdt_test.go::TestIndexNew_WithPNCounterFieldViaDirective_ShouldError", "index/crdt_test.go::TestIndexNew_WithPNCounterFloatField_ShouldError", "index/crdt_test.go::TestIndexNew_WithUniqueIndexOnPNCounter_ShouldError"]

Indexing a `pcounter` or `pncounter` field (Int or Float), through the index API or `@index`, alone, in a composite, unique or collection-level index, fails with `indexing accumulated CRDT fields is not yet supported` (Field, CRDTType). An LWW field can be indexed.

# `go.index.definition.patch-immutable`: Indexes cannot be changed through PatchCollection

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/patch_test.go::TestPatchCollection_AddSecondaryIndex_ShouldError", "index/patch_test.go::TestPatchCollection_ModifySecondaryIndex_ShouldError", "index/patch_test.go::TestPatchCollection_ModifyVectorIndexMetric_ShouldError", "index/patch_test.go::TestPatchCollection_RemoveSecondaryIndex_ShouldError"]

A collection patch that adds, removes or modifies a secondary index, or changes a vector index's metric, fails with `collection indexes cannot be mutated`.

# `go.index.definition.status`: Index build status

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/status_test.go::TestIndexStatus_NewIndex_IsReady", "index/status_test.go::TestIndexStatus_ReadyAndFailedInOneCollection_EachReportsOwnStatus", "index/status_test.go::TestIndexStatus_SDLIndex_IsReady", "index/list_actions_test.go::TestIndexBackfill_AfterSuccessfulBuild_ListsNoAction"]

`ListIndexes` reports a status per index: a new index built by `NewIndex` or declared in SDL is ready, a failed build reports its own error status next to a ready index in the same collection, and a successful build leaves no entry in `ListActions`.

# `go.index.definition.one-to-one`: One-to-one relations get a unique index

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_CompositeIndexRelationNotFirst_AutoIndexStillAdded", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_MultipleNullRelations_Allowed", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_OneToMany_ShouldNotMakeNewIndex", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_OnPrimarySide_AutoAdded", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_ReferenceSameRelatedDoc_RejectsDuplicateLink", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_TypeLevelCompositeUniqueIndex_Succeeds", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_TypeLevelNonUniqueIndex_ReturnsError", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_UserDefinedNonUniqueIndex_ReturnsError", "index/new_unique_composite_one_to_one_test.go::TestOneToOneUniqueIndex_UserDefinedUniqueIndexWithName_Succeeds"]

The primary side of a one-to-one relation automatically gets a unique index on its relation ID field (e.g. `User__addressID_ASC`), also when the field is not first in a composite index; one-to-many relations do not. A user-declared non-unique index on it fails with `one-to-one relation must have a unique index`, a named or composite unique index is accepted, several null links are allowed, and linking two documents to the same related document fails with `can not index a doc's field(s) that violates unique index`.

# `go.index.definition.one-to-one-patch`: Relations added by patch get their unique index

- **Parents:** ["go.index.definition"]
- **Go-Tests:** ["index/patch_relation_test.go::TestPatchRelation_MultipleOneToOne_AddsUniqueIndexesWithCorrectIDs", "index/patch_relation_test.go::TestPatchRelation_OneToMany_DoesNotAddUniqueIndex", "index/patch_relation_test.go::TestPatchRelation_OneToOne_AddsUniqueIndex", "index/patch_relation_test.go::TestPatchRelation_OneToOneWithVersionSwitching_IndexOnlyOnActiveVersion"]

Adding a one-to-one relation with `PatchCollection` adds a unique index on the new relation ID field with correct index IDs, also for several relations at once; a one-to-many relation adds none; and the index exists only on the active collection version when versions are switched.

# `go.index.backfill`: Index backfill

- **Parents:** ["go.index"]

Building an index over documents that already exist.

# `go.index.backfill.async`: Asynchronous backfill

- **Parents:** ["go.index.backfill"]
- **Go-Tests:** ["index/async_backfill_test.go::TestAsyncBackfill_QueryWhileBuilding_FullScans", "index/async_backfill_test.go::TestAsyncBackfill_RestartMidBuild_ResumesToReady", "index/async_backfill_test.go::TestAsyncBackfill_WhileGated_ReportsBuildingThenReady", "index/async_backfill_test.go::TestAsyncBackfill_WriteWhileBuilding_ReflectedInIndex"]
- **Scope:** "client types: go only (uses an in-process build gate); database: badger in-memory, badger file for the restart test"

An index created on a populated collection lists as building until its backfill finishes, then ready, and is then used by filtered queries. While building, queries full-scan (0 index fetches) with correct results, documents written meanwhile end up in the index, and a build interrupted by a restart resumes to ready.

# `go.index.backfill.migration`: Reindexing after a migration

- **Parents:** ["go.index.backfill"]
- **Go-Tests:** ["index/async_backfill_migration_test.go::TestAsyncBackfill_EpochAdvancedMidBuild_LiveEpochComplete", "index/async_backfill_migration_test.go::TestAsyncBackfill_RestartAfterNonZeroWatermark_ResumesToReady", "index/async_backfill_migration_test.go::TestAsyncBackfill_TwoCollectionsMigratedTogether_BothReindexConcurrentlyAndConverge"]
- **Scope:** "client types: go only; database: badger file"

A patch with a migration that reindexes two collections rebuilds both concurrently (both listed in progress together) and both converge to the migrated values; a migration that advances the epoch mid-build leaves every document in the live epoch; a restart after some batches resumes from the stored watermark with no duplicated or skipped documents.

# `go.index.backfill.txn`: Index creation in an explicit transaction

- **Parents:** ["go.index.backfill"]
- **Go-Tests:** ["index/backfill_txn_test.go::TestIndexCreate_WithExplicitTxn_BackfillRunsOnCommit", "index/backfill_txn_test.go::TestIndexCreate_WithExplicitTxnDiscard_NoIndexNoBackfill"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb)"

`NewIndex` inside an explicit transaction is invisible outside it before commit; after commit the index exists and indexes all pre-existing documents. Discarding the transaction leaves no index and filtered queries full-scan with correct results.

# `go.index.backfill.large-docs`: Backfill over large documents

- **Parents:** ["go.index.backfill"]
- **Go-Tests:** ["index/backfill_large_test.go::TestIndex_OverManyLargeExistingDocs_CreatesAndDeletes"]

Creating and deleting an index over many existing documents with ~50 KB indexed values succeeds without exceeding the transaction size limit, and a filtered query on one value returns it through the index.

# `go.index.unique`: Unique indexes

- **Parents:** ["go.index"]

Indexes created with `unique: true`, single-field and composite.

# `go.index.unique.backfill`: Unique index over existing documents

- **Parents:** ["go.index.unique"]
- **Go-Tests:** ["index/new_unique_test.go::TestAddUniqueIndex_IfFieldValuesAreNotUnique_ReturnError", "index/new_unique_test.go::TestUniqueIndexNew_IfFieldValuesAreUnique_Succeed", "index/new_unique_composite_test.go::TestUniqueCompositeIndexNew_IfFieldValuesAreNotUnique_ReturnError", "index/new_unique_composite_test.go::TestUniqueCompositeIndexNew_IfFieldValuesAreOrdered_Succeed", "index/new_unique_composite_test.go::TestUniqueCompositeIndexNew_IfFieldValuesAreUnique_Succeed", "index/backfill_unique_test.go::TestUniqueIndexBackfill_WithDuplicateValues_FailsAndPersistsDefinition"]

Creating a unique index (single or composite, any direction) over unique existing values succeeds. Over duplicate values the backfill fails without an error from `NewIndex`: the definition stays listed with an errored backfill status whose reason contains `can not index`, queries ignore it, it can be deleted, and once the duplicate is fixed a new unique index builds.

# `go.index.unique.write-conflict`: Writes that violate a unique index

- **Parents:** ["go.index.unique"]
- **Go-Tests:** ["index/new_unique_test.go::TestUniqueIndexNew_UponAddingDocWithExistingFieldValue_ReturnError", "index/new_unique_test.go::TestUniqueQueryWithIndex_UponAddingDocWithSameDateTime_Error", "index/new_unique_composite_test.go::TestUniqueCompositeIndexNew_UponAddingDocWithExistingFieldValue_ReturnError", "index/update_unique_test.go::TestUniqueIndexUpdate_UponUpdatingDocNonIndexedField_ShouldSucceed", "index/update_unique_composite_test.go::TestUniqueCompositeIndexUpdate_UponUpdatingDocWithExistingFieldValue_ShouldSucceed"]

Adding a document whose indexed value (single field, DateTime, or composite combination) already exists fails with `can not index a doc's field(s) that violates unique index.`. Updating a non-indexed field of an indexed document succeeds.

# `go.index.unique.nil`: Null values in unique indexes

- **Parents:** ["go.index.unique"]
- **Go-Tests:** ["index/new_unique_test.go::TestUniqueIndexNew_AddingDocWithNilValue_ShouldSucceed", "index/new_unique_test.go::TestUniqueIndexNew_UponAddingDocWithExistingNilValue_ShouldSucceed", "index/new_unique_test.go::TestUniqueIndexNew_WithMultipleNilFields_ShouldSucceed"]

Null indexed values do not collide: several documents may have a null (or absent) value in a unique-indexed field, before or after the index is created.

# `go.index.unique.array`: Unique indexes on array fields

- **Parents:** ["go.index.unique"]
- **Go-Tests:** ["index/array_unique_test.go::TestArrayUniqueIndex_UponDeletingDoc_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_UponDocCreationWithUniqueElement_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_UponDocCreationWithUniqueElements_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_UponDocUpdateWithArrayElementThatExists_Error", "index/array_unique_test.go::TestArrayUniqueIndex_UponDocUpdateWithUniqueElements_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_WithNilElementsAndAllOp_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_WithNilElementsAndAnyOp_Succeed", "index/array_unique_test.go::TestArrayUniqueIndex_WithNilElementsAndNoneOp_Succeed", "index/array_unique_composite_test.go::TestArrayUniqueCompositeIndex_IfDocIsAddedThatViolatesUniqueness_Error", "index/array_unique_composite_test.go::TestArrayUniqueCompositeIndex_IfDocIsUpdatedThatViolatesUniqueness_Error", "index/array_unique_composite_test.go::TestArrayUniqueCompositeIndex_IfDocsHaveNilValues_Succeed", "index/array_unique_composite_test.go::TestArrayUniqueCompositeIndex_WithUniqueCombinations_Succeed"]

A unique index on an array field is unique per element: creating or updating a document so that an element (or, for a composite, an element plus scalar combination) already exists in another document fails with `can not index a doc's field(s) that violates unique index.`. Unique combinations, null values and nil elements are allowed, deleting frees the values, and `_any`/`_all`/`_none` filters return correct results.

# `go.index.unique.json`: Unique indexes on JSON fields

- **Parents:** ["go.index.unique"]
- **Go-Tests:** ["index/json_unique_test.go::TestJSONUniqueIndex_UponUpdate_ShouldUseNewIndexValues", "index/json_unique_test.go::TestJSONUniqueIndex_WithRandomValues_ShouldGuaranteeUniquenessAndBeAbelToUseIndexForFetching", "index/json_unique_array_test.go::TestJSONArrayUniqueIndex_ShouldAllowOnlyUniqueValuesAndUseThemForFetching"]

A unique index on a JSON field is unique per leaf value path (and per element of JSON arrays): a create or update that repeats a value at the same path fails with `can not index a doc's field(s) that violates unique index.`, and equality filters on the new values are served by the index after updates.

# `go.index.query`: Filtering through an index

- **Parents:** ["go.index"]

Filters on indexed scalar fields return the same documents as a full scan, and `@explain(type: execute)` reports index fetches when the index is used.

# `go.index.query.single-operators`: Single-field index with comparison operators

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_index_only_filter_test.go::TestQueryWithIndex_EmptyFilterOnIndexedField_ShouldSucceed", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_IfSeveralDocsWithEqFilter_ShouldFetchAll", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_IfSeveralDocsWithInFilter_ShouldFetchAll", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithEqualFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithGreaterOrEqualFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithGreaterThanFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithInFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithInFilterOnFloat_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLessOrEqualFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLessThanFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLikeFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNonIndexedFields_ShouldFetchAllOfThem", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNotEqualFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNotInFilter_ShouldFetch", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNotLikeFilter_ShouldFetch"]

With `@index` on one field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like` and `_nlike` (Int, Float, String) return the matching documents, several matches are all returned, an empty filter on the field returns all, and filters on non-indexed fields still work; explain reports the expected index fetch count.

# `go.index.query.single-null`: Null semantics of indexed filters

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNeFilterAgainstBooleanField_ShouldFetchNilValues", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNeFilterAgainstDateTimeField_ShouldFetchNilValues", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNeFilterAgainstFloatField_ShouldFetchNilValues", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNeFilterAgainstIntField_ShouldFetchNilValues", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithNeFilterAgainstStringField_ShouldFetchNilValues", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithGeqNullFilterOnDateTimeField_ShouldNotUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithGeqNullFilterOnFloatField_ShouldNotUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithGeqNullFilterOnIntField_ShouldNotUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLeqNullFilterOnDateTimeField_ShouldUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLeqNullFilterOnFloatField_ShouldUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithLeqNullFilterOnIntField_ShouldUseIndex"]

On an indexed Int, Float, String, Boolean or DateTime field, `_ne: <value>` also returns documents whose value is null and `_ne: null` returns only non-null ones. `_le: null` returns only null documents and uses the index; `_ge: null` returns every document and does not use the index (0 index fetches).

# `go.index.query.datetime`: Indexed DateTime filters

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithEqFilterOnDateTimeField_ShouldIndex", "index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithGeFilterOnDateTimeField_ShouldIndex", "index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithGtFilterOnDateTimeField_ShouldIndex", "index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithLeFilterOnDateTimeField_ShouldIndex", "index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithLtFilterOnDateTimeField_ShouldIndex", "index/query_with_index_on_datetime_test.go::TestQueryWithIndex_WithNeFilterOnDateTimeField_ShouldIndex"]

On an indexed DateTime field, `_eq`, `_ne`, `_gt`, `_ge`, `_lt` and `_le` with an RFC 3339 value return the matching documents through the index.

# `go.index.query.unique-single`: Unique single-field index filters

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_IfNoMatch_ReturnEmptyResult", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithDateTimeField_ShouldIndex", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithEqualFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithEqualFilterOnNilValue_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithEqualFilterOnZero_ShouldNotFetchNil", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithGreaterOrEqualFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithGreaterThanFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithInFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithLessOrEqualFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithLessThanFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithLikeFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithMultipleNilValuesAndEqualFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithNotCaseInsensitiveLikeFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithNotEqualFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithNotEqualFilterOnNilValue_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithNotInFilter_ShouldFetch", "index/query_with_unique_index_only_filter_test.go::TestQueryWithUniqueIndex_WithNotLikeFilter_ShouldFetch"]

With a unique index on one field (Int, String or DateTime), every comparison operator including `_ilike`/`_nilike` returns the matching documents; no match returns an empty list, `_eq: 0` does not return null documents, `_eq: null` returns every null document and `_ne: null` every non-null one.

# `go.index.query.composite-prefix`: Composite index field-prefix use

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_IfConsecutiveEqOps_ShouldUseAllToOptimizeQuery", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_IfFirstFieldIsNotInFilter_ShouldNotUseIndex", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_IfMiddleFieldIsNotInFilter_ShouldIgnoreValue", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_IfFirstFieldIsNotInFilter_ShouldNotUseIndex", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_IfMiddleFieldIsNotInFilter_ShouldIgnoreValue", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithInForFirstAndEqForRest_ShouldFetchEfficiently"]

A composite index (unique or not) is used only when its first field is in the filter; consecutive `_eq` on leading fields all narrow the scan, a field missing in the middle is skipped, and `_in` on the first field with `_eq` on the rest is served efficiently.

# `go.index.query.composite-operators`: Composite index with comparison operators

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithEqualFilter_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithEqualFilterOnNilValueOnFirst_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithEqualFilterOnNilValueOnSecond_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithGreaterOrEqualFilterOnFirstField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithGreaterOrEqualFilterOnSecondField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithGreaterThanFilterOnFirstField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithGreaterThanFilterOnSecondField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithInFilter_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithLessOrEqualFilterOnFirstField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithLessOrEqualFilterOnSecondField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithLessThanFilterOnFirstField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithLessThanFilterOnSecondField_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithLikeFilter_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithNotEqualFilter_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithNotInFilter_ShouldFetch", "index/query_with_composite_index_only_filter_test.go::TestQueryWithCompositeIndex_WithNotLikeFilter_ShouldFetch"]

With a composite index on two fields, `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in`, `_nin`, `_like`, `_nlike` on the first or second field return the matching documents, including `_eq: null` on either field.

# `go.index.query.unique-composite-operators`: Unique composite index with comparison operators

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithEqualFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithGreaterOrEqualFilterOnFirstField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithGreaterOrEqualFilterOnSecondField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithGreaterThanFilterOnFirstField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithGreaterThanFilterOnSecondField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithInFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithLessOrEqualFilterOnFirstField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithLessOrEqualFilterOnSecondField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithLessThanFilterOnFirstField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithLessThanFilterOnSecondField_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithLikeFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithNotCaseInsensitiveLikeFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithNotEqualFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithNotInFilter_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithNotLikeFilter_ShouldFetch"]

With a unique composite index on two fields, every comparison operator on the first or second field, including `_nilike`, returns the matching documents.

# `go.index.query.unique-composite-nil`: Nulls in a unique composite index

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_AfterUpdateOnNilFields_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithEqualFilterOnNilValueOnFirst_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithEqualFilterOnNilValueOnSecond_ShouldFetch", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithMultipleNilOnBothFieldsAndNilFilter_ShouldFetchAll", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithMultipleNilOnFirstFieldAndNilFilter_ShouldFetchAll", "index/query_with_unique_composite_index_filter_test.go::TestQueryWithUniqueCompositeIndex_WithMultipleNilOnSecondFieldsAndNilFilter_ShouldFetchAll"]

A unique composite index allows several documents with null in either or both fields; `_eq: null` on either field returns all of them, and documents updated from null values are found by their new values.

# `go.index.query.combined`: Indexed and non-indexed conditions together

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_index_combined_filter_test.go::TestQueryWithIndex_FilterOnNonIndexedField_ShouldIgnoreIndex", "index/query_with_index_combined_filter_test.go::TestQueryWithIndex_IfIndexFilterWithRegular_ShouldFilter", "index/query_with_index_combined_filter_test.go::TestQueryWithIndex_IfMultipleIndexFiltersWithRegular_ShouldFilter", "index/query_with_index_combined_filter_test.go::TestQueryWithIndex_IfMultipleIndexFiltersWithRegularCaseInsensitive_ShouldFilter"]

A filter combining one or several indexed fields with non-indexed fields (field names case-insensitive in the test schema) returns only documents matching all conditions; a filter on only non-indexed fields ignores the index.

# `go.index.query.or`: `_or` filters and indexes

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilter_ShouldFetchCorrectDocs", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterOnIndexedAndNonIndexedField_ShouldFallbackToFullScan", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterOnOnlyNonIndexedFields_ShouldNotUseIndex", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterOnTwoDifferentIndexedFields_ShouldNotUseIndex", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterOnTwoDifferentIndexedFields_WithOverlap_ShouldReturnCorrectResults", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterOnTwoDifferentIndexedFields_WithRangeConditions_ShouldNotUseIndex", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterWithAnyAndNoneOnSameArrayField_ShouldFallbackToFullScan", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterWithOverlappingConditions_ShouldDeduplicateResults", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterWithRangeConditions_ShouldUseIndex", "index/query_with_index_filter_compound_test.go::TestQueryWithIndex_WithOrFilterWithThreeBranches_ShouldUseIndex"]

`_or` branches on the same indexed field (equality or ranges, two or three branches) are served by the index and overlapping branches return each document once. `_or` mixing indexed and non-indexed fields, two different indexed fields, or `_any` and `_none` on one array field falls back to a full scan, with correct results.

# `go.index.query.performance`: Indexed query speed-up

- **Parents:** ["go.index.query"]
- **Go-Tests:** ["index/query_performance_test.go::TestQueryPerformance_Simple", "index/query_performance_test.go::TestQueryPerformance_WithFloat32"]
- **Scope:** "benchmark measured on the go client only"

An `_eq` filter on an indexed Int or Float32 field over 500 generated documents runs at least twice as fast as the same filter on an identical non-indexed collection.

# `go.index.order`: Ordering through an index

- **Parents:** ["go.index"]

Result order when an index serves `order` or when an unordered query reads an index.

# `go.index.order.single-field`: `order` on a single indexed field

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/order_query_test.go::TestOrderQueryWithIndex_WithAscendingOrder_ShouldUseIndex", "index/order_query_test.go::TestOrderQueryWithIndex_WithAscendingQueryOnDescendingIndexedField_ShouldReturnInReverseOrder", "index/order_query_test.go::TestOrderQueryWithIndex_WithFilterOnIndexedFieldAscending_ShouldUseIndex", "index/order_query_test.go::TestOrderQueryWithIndex_WithFilterOnIndexedFieldDescending_ShouldUseIndex", "index/order_query_test.go::TestOrderQueryWithIndex_WithFilterOnNonIndexedFieldAscending_ShouldUseIndexForOrdering", "index/order_query_test.go::TestOrderQueryWithIndex_WithFilterOnNonIndexedFieldDescending_ShouldUseIndexForOrdering", "index/order_query_test.go::TestOrderQueryWithIndex_WithLimitAscending_ShouldUseIndex", "index/order_query_test.go::TestOrderQueryWithIndex_WithLimitDescending_ShouldUseIndex"]

`order: {age: ASC|DESC}` on an indexed field returns documents in that order through the index, with or without `limit` and with filters on the same or another field; an ascending order on a `direction: DESC` index still returns ascending results.

# `go.index.order.composite`: `order` against a composite index

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/order_query_test.go::TestOrderQueryWithCompositeIndex_OrderMismatchASCAndASC_ShouldNotUseIndex", "index/order_query_test.go::TestOrderQueryWithCompositeIndex_OrderMismatchASCAndDESC_ShouldNotUseIndex", "index/order_query_test.go::TestOrderQueryWithCompositeIndex_OrderMismatchDESCAndASC_ShouldNotUseIndex", "index/order_query_test.go::TestOrderQueryWithCompositeIndex_OrderMismatchDESCAndDESC_ShouldNotUseIndex", "index/order_query_test.go::TestOrderQueryWithCompositeIndex_WithOrderOnNonIndexInEnd_ShouldNotUseIndex", "index/order_query_test.go::TestOrderQueryWithCompositeIndex_WithOrderOnNonIndexInMiddle_ShouldNotUseIndex"]

A multi-field `order` whose directions match neither the composite index nor its exact reverse, or that puts a non-indexed field in the middle or end, does not use the index (0 index fetches) and still returns correctly ordered results.

# `go.index.order.relation`: `order` on related or relation ID fields

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/order_query_test.go::TestOrderQueryWithIndex_WithOrderOnNestedField_ShouldUseIndexForOrdering", "index/order_query_test.go::TestOrderQueryWithIndex_WithOrderOnRelationIDField_ShouldUseIndexForOrdering", "index/order_query_test.go::TestOrderQueryWithIndexOnRelation_OrderByPrimaryDoc_ShouldOrderWithIndex", "index/order_query_test.go::TestOrderQueryWithIndexOnRelation_OrderBySecondaryDoc_ShouldOrderWithIndex"]

Ordering by an indexed field of a related document (`order: {device: {model: ASC}}`), by an indexed relation ID (`order: {_ownerID: ASC}`), or from either side of a relation returns correctly ordered results using the index.

# `go.index.order.descending-index`: Unordered results follow a descending index

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfFloat32FieldInDescOrder_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfFloatFieldInDescOrderWithGe_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfFloatFieldInDescOrderWithGt_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfFloatFieldInDescOrderWithLe_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfFloatFieldInDescOrderWithLt_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfIntFieldInDescOrderWithGe_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfIntFieldInDescOrderWithGt_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfIntFieldInDescOrderWithLe_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfIntFieldInDescOrderWithLt_ShouldFetchInRevertedOrder", "index/query_with_index_only_field_order_test.go::TestQueryWithIndex_IfStringFieldInDescOrder_ShouldFetchInRevertedOrder"]

A filtered query without `order` on a `@index(direction: DESC)` Int, Float, Float32 or String field returns documents in descending value order, for `_gt`, `_ge`, `_lt`, `_le` filters.

# `go.index.order.composite-direction`: Unordered results follow composite index directions

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_IfExactMatchWithRevertedOrderOnFirstField_ShouldFetch", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_IfExactMatchWithRevertedOrderOnSecondField_ShouldFetch", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithDefaultOrder_ShouldFetchInDefaultOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithDefaultOrderCaseInsensitive_ShouldFetchInDefaultOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRevertedOrderOnFirstField_ShouldFetchInRevertedOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRevertedOrderOnFirstFieldAndNoFilter_ShouldFetchInRevertedOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRevertedOrderOnFirstFieldCaseInsensitive_ShouldFetchInRevertedOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRevertedOrderOnSecondField_ShouldFetchInRevertedOrder", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRevertedOrderOnSecondFieldCaseInsensitive_ShouldFetchInRevertedOrder"]

A query without `order` filtered through a composite index returns documents in the index's per-field direction order (default ASC, or DESC on the first or second field), with direction keywords case-insensitive, also without a filter and for exact matches.

# `go.index.order.composite-in`: `_in` through a composite index with a reversed field

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithInFilterOnFirstFieldWithRevertedOrder_ShouldFetch", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithInFilterOnSecondFieldWithRevertedOrder_ShouldFetch"]
- **Go-Bug-Suspect:** "TestQueryWithCompositeIndex_WithInFilterOnSecondFieldWithRevertedOrder_ShouldFetch is marked 'TODO: This test documents incorrect behaviour' (Go issue #3780): the expected order Andy, Fred, Shahzad is replaced by the actual Shahzad, Andy, Fred, and the check is relaxed to non-ordered."

`_in` on the first or second field of a composite index with one DESC field returns the matching documents.

# `go.index.order.composite-range`: Range filters on a composite index's first field

- **Parents:** ["go.index.order"]
- **Go-Tests:** ["index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithDescendingFirstFieldAndRangeQuery_ShouldUseRangeOptimization", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRangeQueryOnFirstField_ShouldUseRangeOptimization", "index/query_with_composite_index_field_order_test.go::TestQueryWithCompositeIndex_WithRangeQueryOnFirstFieldWithMultipleFilters_ShouldUseRangeOptimization"]

A range filter (`_gt` etc.) on the first field of a composite index, ASC or DESC, alone or with further filters, is served by an index range scan and returns the matching documents.

# `go.index.relation`: Indexes across relations

- **Parents:** ["go.index"]

Filters and ordering on related documents through indexes on relation or related fields.

# `go.index.relation.one-to-many-filter`: Filters through one-to-many and many-to-one indexes

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_filter_test.go::TestQueryWithIndexOnManyToOne_IfFilterOnIndexedField_ShouldFilterWithExplain", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnManyToOne_IfFilterOnIndexedRelation_ShouldFilterWithExplain", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnManyToOne_MultipleViaOneToMany", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToMany_IfFilterOnIndexedPrimaryDoc_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToMany_IfFilterOnIndexedPrimaryDocAndSubFilter_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToMany_IfFilterOnIndexedRelation_ShouldFilterWithExplain", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToMany_IfIndexedRelationIsNil_EqNilFilterShouldUseIndex", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToMany_IfIndexedRelationIsNil_NeNilFilterShouldUseIndex", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToManyRelation_IfFilterOnIndexedRelation_ShouldFilter"]

Filtering parents by an indexed field of their children (`User(filter: {devices: {model: {_eq: ...}}})`), children by an indexed parent field or relation ID, including `_eq: null`/`_ne: null` on an indexed relation, and many-to-one chains, returns the right documents and reports index fetches at the expected explain levels.

# `go.index.relation.one-to-one-filter`: Filters through one-to-one indexes

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToOne_IfFilterOnIndexedRelation_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToOnePrimaryRelation_IfFilterOnIndexedFieldOfRelation_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToOnePrimaryRelation_IfFilterOnIndexedFieldOfRelationAndRelation_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToOnePrimaryRelation_IfFilterOnIndexedRelationWhileIndexedForeignField_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithIndexOnOneToOnesSecondaryRelation_IfFilterOnIndexedRelation_ShouldFilter", "index/query_with_relation_filter_test.go::TestQueryWithUniqueIndex_WithFilterOnChildIndexedField_ShouldFetch", "index/query_with_unique_index_on_relation_filter_test.go::TestQueryWithUniqueCompositeIndex_WithFilterOnIndexedRelation_ShouldFilter"]

Filtering on an indexed field of a one-to-one related document, from the primary or secondary side, on the relation and a related field together, or through a unique (composite) index on the related type, returns the matching documents.

# `go.index.relation.combined-filter`: Relation filters combined with other conditions

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_filter_test.go::TestQueryWithIndex_WithMultipleScalarsAndRelationFilter_ShouldApplyAllAsAnd", "index/query_with_relation_filter_test.go::TestQueryWithIndex_WithScalarAndRelationFilterAtTopLevel_ShouldApplyBothAsAnd", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithFilterOn2Relations_ShouldFilter", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationAndFilter_Data", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationAndFilter_NoData", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationNotFilter_Data", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationNotFilter_NoData", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationOrFilter_Data", "index/query_with_compound_filter_relation_test.go::TestIndex_QueryWithIndexOnOneToManyRelationOrFilter_NoData", "index/query_with_group_test.go::TestIndex_GroupByWithFilterOnIndexedRelation_ReturnsGroupByFieldValue", "index/query_with_composite_inxed_on_relation_test.go::TestQueryWithCompositeIndexOnManyToOne_WithMultipleIndexedChildNodes_ShouldSucceed"]

Scalar and relation conditions at the top level apply as an AND; `_and`, `_or` and `_not` over an indexed one-to-many relation return the right documents or none; filters on two relations, `groupBy` with a filter on an indexed relation, and a composite index on several indexed child fields all return correct results.

# `go.index.relation.sub-filter`: Child sub-filters through indexes

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_IfSubFilterAndOrderOnIndexedField_ShouldUseIndexForFilter", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_IfSubFilterOnIndexedField_ShouldFilter", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_IfSubFilterOnNonIndexedField_ShouldNotUseIndex", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithOrderOnParentAndSubFilter_ShouldFilterBothWithIndexes", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithOrderOnParentAndSubFilter_ShouldFilterPerParent", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithParentFilterOnOwnFieldAndRelationAndSubFilter_ShouldCombineAllFilters", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithParentFilterOnRelationAndSubFilterOnDifferentIndexedField_ShouldUseBothIndexes", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithParentFilterOnRelationAndSubFilterOnNonIndexedField_ShouldUseParentIndex", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithSameFilterOnParentAndSubType_ShouldFilterBothWithIndexes", "index/query_with_relation_sub_filter_test.go::TestQueryWithIndexOnOneToMany_WithSameFilterValueOnParentAndSubType_ShouldReturnMatchingDocs"]

A filter on a one-to-many child selection (`devices(filter: ...)`), alone or with a parent filter, parent ordering, or the same condition at both levels, returns each parent's matching children, using the child's index when the field is indexed and not otherwise.

# `go.index.relation.sub-order`: Child ordering through indexes

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithIndexOnOrderFieldAscending_ShouldOrder", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithIndexOnOrderFieldAscendingWithLimit_ShouldOrderAndLimit", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithIndexOnOrderFieldDescending_ShouldOrder", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithMultipleAuthors_ShouldOrderEachAuthorsBooks", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithMultipleAuthorsAndIndexOnRelation_ShouldOrderEachAuthorsBooks", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithParentFilterOnRelationAndSubOrder_ShouldOrderChildren", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithSubFilterAndOrderAndRelationIndex_ShouldFilterThenOrder", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithSubFilterAndOrderOnSameIndexedField_ShouldFilterThenOrderASC", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderOnOneToMany_WithSubFilterAndOrderOnSameIndexedField_ShouldFilterThenOrderDESC"]

`published(order: {rating: ASC|DESC})` on a one-to-many child with an indexed order field orders each parent's children independently, with `limit`, with a sub-filter on the same or another field (filter then order), and with a parent filter on the relation.

# `go.index.relation.order-orphans-excluded`: Ordering by a related field drops orphans without `@exhaustive`

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_WithParentPrimaryASC_ExcludesOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_WithParentSecondaryASC_ExcludesOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_WithASCAndLimit_ExcludesOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_WithDESCAndLimit_ExcludesOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_WithASCAndLimit_RecursiveExplain", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_WithDESCAndLimit_RecursiveExplain"]
- **Go-Bug-Suspect:** "Adding 'order' silently removes documents with no related document from the result, so the result set depends on ordering and indexing; the tests say orphans are excluded 'for performance'."

Without `@exhaustive`, ordering by an indexed field of a related document (`Book(order: {publisher: {establishedYear: ASC}})`, at the root or in a nested selection, with or without `limit`) returns only documents that have the related document; explain shows the index-based inverted join at each level.

# `go.index.relation.order-orphans-exhaustive`: `@exhaustive` keeps orphans when ordering by a related field

- **Parents:** ["go.index.relation"]
- **Go-Tests:** ["index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_ExhaustiveWithParentPrimaryASC_ShouldIncludeOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_ExhaustiveWithParentPrimaryDESC_ShouldIncludeOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_ExhaustiveWithParentSecondaryASC_ShouldIncludeOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_ExhaustiveWithParentSecondaryDESC_ShouldIncludeOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_ExhaustiveWithASCAndLimit_ShouldIncludeOrphansFirst", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_ExhaustiveWithDESCAndLimit_ShouldAppendOrphansLast", "index/query_with_relation_sub_order_test.go::TestQueryWithNestedOrderByRelationField_ExhaustiveWithPrimaryParentASC_ShouldIncludeOrphans", "index/query_with_relation_sub_order_test.go::TestQueryWithOrderByRelationField_WithSomeDocsWithoutRelation_ShouldIncludeAll", "index/query_with_relation_sub_order_test.go::TestQueryWithFilterOnNullRelation_SecondaryDocWithoutRelation_ShouldReturnOrphans", "index/query_with_relation_sub_order_complex_test.go::TestQueryWithOrderByRelationField_ExhaustiveASC_ManyBooksShowsFullPipeline", "index/query_with_relation_sub_order_complex_test.go::TestQueryWithOrderByRelationField_ExhaustiveASCWithLimit_ManyOrphansEarlyTermination", "index/query_with_relation_sub_order_complex_test.go::TestQueryWithOrderByRelationField_ExhaustiveDESCWithLimit_ManyOrphansSkipsOrphanPhase", "index/query_with_relation_filter_test.go::TestQueryWithIndex_ExhaustiveOrderByRelationWithScalarFilter_ShouldFilterOrphans", "index/query_with_relation_filter_test.go::TestQueryWithIndex_ExhaustiveOrderByRelationWithScalarFilterDescending_ShouldFilterOrphans"]

With `query @exhaustive`, ordering by a related document's field returns every document: orphans (null related value) come first for ASC and last for DESC, at the root or nested, from the primary or secondary side, with `limit` stopping early, and with same-object scalar filters still applied to orphans.

# `go.index.array`: Indexes on array fields

- **Parents:** ["go.index"]

Non-unique indexes on scalar array fields, indexed per element.

# `go.index.array.types`: Array element types

- **Parents:** ["go.index.array"]
- **Go-Tests:** ["index/array_test.go::TestArrayIndex_Bool_ShouldUseIndex", "index/array_test.go::TestArrayIndex_Float_ShouldUseIndex", "index/array_test.go::TestArrayIndex_OptionalBool_ShouldUseIndex", "index/array_test.go::TestArrayIndex_OptionalFloat_ShouldUseIndex", "index/array_test.go::TestArrayIndex_OptionalInt_ShouldUseIndex", "index/array_test.go::TestArrayIndex_OptionalString_ShouldUseIndex"]

An index on `[Int]`, `[Float]`, `[Boolean]`, `[String]` and their nillable-element variants serves `_any: {_eq: ...}` filters and returns the matching documents.

# `go.index.array.operators`: `_any`, `_all`, `_none` on indexed arrays

- **Parents:** ["go.index.array"]
- **Go-Tests:** ["index/array_test.go::TestArrayIndex_WithAllAndInOperator_Succeed", "index/array_test.go::TestArrayIndex_WithAllAndNinOperator_Succeed", "index/array_test.go::TestArrayIndex_WithAnyAndInOperator_Succeed", "index/array_test.go::TestArrayIndex_WithAnyAndNinOperator_Succeed", "index/array_test.go::TestArrayIndex_WithFilterOnIndexedArrayUsingAll_ShouldUseIndex", "index/array_test.go::TestArrayIndex_WithFilterOnIndexedArrayUsingAny_ShouldUseIndex", "index/array_test.go::TestArrayIndex_WithFilterOnIndexedArrayUsingNone_ShouldNotUseIndex", "index/array_test.go::TestArrayIndex_WithNoneAndInOperator_Succeed", "index/array_test.go::TestArrayIndex_WithNoneAndNinOperator_Succeed"]

`_any` and `_all` filters on an indexed array (with `_eq`, `_in`, `_nin`) use the index and return the matching documents; `_none` returns correct results without using the index.

# `go.index.array.nil-elements`: Null elements in indexed arrays

- **Parents:** ["go.index.array"]
- **Go-Tests:** ["index/array_test.go::TestArrayIndex_WithNilElementsAndAllOp_Succeed", "index/array_test.go::TestArrayIndex_WithNilElementsAndAnyOp_Succeed", "index/array_test.go::TestArrayIndex_WithNilElementsAndNoneOp_Succeed"]

Arrays containing null elements are indexed; `_any`, `_all` and `_none` filters with `_eq: null` or a value return the matching documents.

# `go.index.array.update`: Array index maintenance on update

- **Parents:** ["go.index.array"]
- **Go-Tests:** ["index/array_test.go::TestArrayIndexDelete_IfUpdateRemovesSoughtElement_ShouldNotFetch", "index/array_test.go::TestArrayIndexUpdate_IfUpdateAddsSoughtElement_ShouldFetch", "index/array_test.go::TestArrayIndexUpdate_IfUpdateRearrangesArrayElements_ShouldFetch", "index/array_test.go::TestArrayIndexUpdate_IfUpdateRemovesSoughtElement_ShouldNotFetch"]

After an update adds, removes or reorders array elements, `_any` filters return the document only if the sought element is present in the new array.

# `go.index.array.composite`: Composite indexes with array fields

- **Parents:** ["go.index.array"]
- **Go-Tests:** ["index/array_composite_test.go::TestArrayCompositeIndex_With2ConsecutiveArrayFields_Succeed", "index/array_composite_test.go::TestArrayCompositeIndex_With2SeparateArrayFields_Succeed", "index/array_composite_test.go::TestArrayCompositeIndex_WithAnyNoneAll_Succeed", "index/array_composite_test.go::TestArrayCompositeIndex_WithFilterOnIndexedArrayUsingAll_ShouldUseIndex", "index/array_composite_test.go::TestArrayCompositeIndex_WithFilterOnIndexedArrayUsingAny_ShouldUseIndex", "index/array_composite_test.go::TestArrayCompositeIndex_WithFilterOnIndexedArrayUsingNone_ShouldUseIndex", "index/array_composite_test.go::TestArrayCompositeIndexDelete_With2ConsecutiveArrayFields_Succeed", "index/array_composite_test.go::TestArrayCompositeIndexUpdate_With2ArrayFields_Succeed"]

Composite indexes with two array fields (adjacent or separated by a scalar) serve `_any`, `_all` and `_none` filters and stay correct after updates and deletes.

# `go.index.json`: Indexes on JSON fields

- **Parents:** ["go.index"]

`@index` on a `JSON` field indexes each leaf value by its path.

# `go.index.json.scalar-ops`: Comparison filters on JSON paths

- **Parents:** ["go.index.json"]
- **Go-Tests:** ["index/json_test.go::TestJSONIndex_UponUpdate_ShouldUseNewIndexValues", "index/json_test.go::TestJSONIndex_WithCompoundFilterCondition_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithEqFilterOnBoolField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithEqFilterOnStringField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithGeFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithGreaterThanFilterOnTopLevelJSONField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithGtFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithInFilter_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithInFilterOfDifferentTypes_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithLeFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithLikeFilterOnStringField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithLtFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNeFilterOnBoolField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNeFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNinFilter_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNLikeFilterOnStringField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNonNillableField_WithFilterOnNumberField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNotAndInFilter_ShouldNotUseIndex"]

Filters on a path inside an indexed JSON field (`custom: {height: {_eq: 168}}`) for numbers, strings and booleans with `_eq`, `_ne`, `_gt`, `_ge`, `_lt`, `_le`, `_in` (including mixed-type lists), `_nin`, `_like`, `_ilike`, `_nlike`, `_nilike`, `_and`, and top-level `_gt` return the matching documents through the index, also for `JSON!` and after updates; `_not` over `_in` does not use the index.

# `go.index.json.null`: Null semantics on indexed JSON

- **Parents:** ["go.index.json"]
- **Go-Tests:** ["index/json_test.go::TestJSONIndex_WithEqFilterAgainstExplicitNullField_ShouldFetchNullValues", "index/json_test.go::TestJSONIndex_WithEqFilterOnNullField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithGeqNullFilterOnNestedJSONPath_ShouldNotUseIndex", "index/json_test.go::TestJSONIndex_WithGeqNullFilterOnTopLevelJSONField_ShouldNotUseIndex", "index/json_test.go::TestJSONIndex_WithLeqNullFilterOnNestedJSONPath_ShouldNotUseIndex", "index/json_test.go::TestJSONIndex_WithLeqNullFilterOnTopLevelJSONField_ShouldUseIndex", "index/json_test.go::TestJSONIndex_WithNeFilterAgainstBoolField_ShouldFetchNullValues", "index/json_test.go::TestJSONIndex_WithNeFilterAgainstNumberField_ShouldFetchNullValues", "index/json_test.go::TestJSONIndex_WithNeFilterAgainstStringField_ShouldFetchNullValues", "index/json_test.go::TestJSONIndex_WithNeFilterOnNullNestedField_ShouldUseIndex"]

`_eq: null` on a JSON path or on the whole field returns documents where it is null; `_ne: <value>` also returns documents where the path is null or missing; `_ne: null` returns non-null ones. `_le: null` on the whole field uses the index; `_ge: null` (top-level or nested) and `_le: null` on a nested path do not, with correct results.

# `go.index.json.object-values`: Object and array values in JSON filters

- **Parents:** ["go.index.json"]
- **Go-Tests:** ["index/json_test.go::TestJSONIndex_WithEqFilterWithObjectValueOnNestedPath_ShouldFilter", "index/json_test.go::TestJSONIndex_WithInFilterWithObjectValueOnNestedPath_ShouldFilter", "index/json_test.go::TestJSONIndex_WithNeqFilterWithObjectValueOnNestedPath_ShouldFilter", "index/json_test.go::TestJSONIndex_WithNinFilterWithObjectValueOnNestedPath_ShouldFilter", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithEqFilterOnJSONFieldWithNestedArrayValue_ShouldNotUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithEqFilterOnJSONFieldWithObjectValue_ShouldNotUseIndex", "index/query_with_index_only_filter_test.go::TestQueryWithIndex_WithInFilterOnJSONFieldWithObjectValue_ShouldNotUseIndex"]

`_eq`, `_ne`, `_in` and `_nin` whose value is a JSON object or nested array (`nested: {_eq: {foo: "bar"}}`) return the matching documents by full-scan comparison without using the index (0 index fetches).

# `go.index.json.array`: JSON arrays in an index

- **Parents:** ["go.index.json"]
- **Go-Tests:** ["index/json_array_test.go::TestJSONArrayIndex_WithAllEqAndComparisonFilter_ShouldFetchCorrectlyUsingIndex", "index/json_array_test.go::TestJSONArrayIndex_WithAllEqFilter_ShouldFetchCorrectlyUsingIndex", "index/json_array_test.go::TestJSONArrayIndex_WithAnyAndComparisonFilter_ShouldNotConsiderThem", "index/json_array_test.go::TestJSONArrayIndex_WithAnyEqFilter_ShouldNotConsiderThem", "index/json_array_test.go::TestJSONArrayIndex_WithDifferentElementValuesAndTypes_ShouldFetchCorrectlyUsingIndex", "index/json_array_test.go::TestJSONArrayIndex_WithNoneEqAndComparisonFilter_ShouldFetchCorrectlyUsingIndex", "index/json_array_test.go::TestJSONArrayIndex_WithNoneEqFilter_ShouldFetchCorrectlyUsingIndex"]

Elements of JSON arrays are indexed: `_any: {_eq|_gt: ...}` matches top-level elements only (not elements of nested arrays or a scalar in place of the array); `_all` with `_eq`/`_gt` uses the index; `_none` returns correct results without the index; elements of mixed types are matched correctly.

# `go.index.json.composite`: Composite indexes with JSON fields

- **Parents:** ["go.index.json"]
- **Go-Tests:** ["index/json_composite_test.go::TestJSONArrayCompositeIndex_JSONArrayWithArrayField_ShouldFetchUsingIndex", "index/json_composite_test.go::TestJSONArrayCompositeIndex_JSONArrayWithScalar_ShouldFetchUsingIndex", "index/json_composite_test.go::TestJSONCompositeIndex_JSONWithScalarWithEqFilter_ShouldFetchUsingIndex", "index/json_composite_test.go::TestJSONCompositeIndex_JSONWithScalarWithOtherFilters_ShouldFetchUsingIndex2", "index/json_composite_test.go::TestJSONCompositeIndex_ScalarWithJSON_ShouldFetchUsingIndex"]

Composite indexes pairing a JSON field (scalar paths or arrays) with a scalar or array field, in either order, return the matching documents for filters on either or both fields, with the index fetch counts each case expects.

# `go.index.maintenance`: Index maintenance outside direct writes

- **Parents:** ["go.index"]

Index entries kept correct by deletes and P2P merges.

# `go.index.maintenance.delete-with-filter`: Filter deletes clean up index entries

- **Parents:** ["go.index.maintenance"]
- **Go-Tests:** ["index/delete_test.go::TestDocDeleteWithFilter_WithMultipleMatchingDocs_CleansUpAllIndexEntries", "index/delete_test.go::TestDocDeleteWithFilter_WithUniqueIndex_CleansUpIndexEntries"]

`delete_<Type>(filter: ...)` removes the index entries of every deleted document, so re-adding documents with the same unique or non-unique indexed values succeeds.

# `go.index.maintenance.p2p`: Replicated changes update the peer's index

- **Parents:** ["go.index.maintenance"]
- **Go-Tests:** ["index/index_p2p_test.go::TestIndexP2P_IfPeerAddedDoc_ListeningPeerShouldIndexIt", "index/index_p2p_test.go::TestIndexP2P_IfPeerDeleteDoc_ListeningPeerShouldDeleteIndex", "index/index_p2p_test.go::TestIndexP2P_IfPeerUpdateDoc_ListeningPeerShouldUpdateIndex", "index/index_p2p_test.go::TestIndexP2P_UniqueConflictOnReplicatorRetry_ShouldNotBlockOtherDocs"]

Documents created, updated or deleted on one peer and synced to another are added to, updated in or removed from the receiving peer's index, as filtered queries there show. A replicated document rejected by a unique index on the target stays in the retry queue without blocking other documents from arriving.

# `go.index.vector`: Vector indexes

- **Parents:** ["go.index"]

HNSW vector indexes on `[Float32!]` fields used by `SIMILARITY` queries.

# `go.index.vector.create`: Creating vector indexes

- **Parents:** ["go.index.vector"]
- **Go-Tests:** ["index/vector_params_test.go::TestVectorIndex_CreateAsUnique_IsRejected", "index/vector_params_test.go::TestVectorIndex_CreateViaIndexAPI_ThenQueryRoutes", "index/vector_params_test.go::TestVectorIndex_CreateWithDefaultParams_Works", "index/vector_params_test.go::TestVectorIndex_CreateWithDirection_IsRejected", "index/vector_params_test.go::TestVectorIndex_CreateWithOversizedM_IsRejected", "index/vector_metrics_test.go::TestVectorIndex_SecondIndexOnFieldWithDifferentMetric_IsRejected", "index/vector_metrics_test.go::TestVectorIndex_DropThenRecreateWithDifferentMetric_IsAllowed"]

A vector index can be created through the index API on a populated collection, with only an empty HNSW config filled with defaults, and similarity queries then route to it. Creation fails for an HNSW parameter above its cap (`vector index parameter is out of range`), as unique (`only an ordered index can be unique`), with a direction (`vector index cannot have a direction`), or as a second index on a field with another metric (`field already has a vector index with a different distance metric`); dropping and recreating with a new metric is allowed.

# `go.index.vector.scoring`: Similarity results follow the index metric

- **Parents:** ["go.index.vector"]
- **Go-Tests:** ["index/vector_metrics_test.go::TestVectorIndex_QueryOnAnyMetric_ShouldUseIndexAndScoreByItsMetric", "index/vector_metrics_test.go::TestVectorIndex_SameQueryUsingIndexAndFullScan_ReturnsSameResults", "index/vector_metrics_test.go::TestVectorIndex_EuclideanOnLargeVectors_OrdersByDistance", "index/vector_metrics_test.go::TestVectorIndex_DotProductOnLargeVectors_OrdersByDistance", "index/vector_metrics_test.go::TestVectorIndex_CosineOnLargeVectors_OrdersByDistance"]

`SIMILARITY(vector: {vector: [...]})` ordered DESC with a limit uses the index and scores by the index's metric (COSINE, EUCLIDEAN, DOT), returning the same results as a full scan. Vectors whose squared distance or dot product exceeds float32 range are scored in float64 and ordered correctly.

# `go.index.vector.backfill`: Vector index backfill

- **Parents:** ["go.index.vector"]
- **Go-Tests:** ["index/vector_backfill_test.go::TestVectorIndex_AsyncBackfillOverExistingDocs_BuildsUsableGraph", "index/vector_backfill_test.go::TestVectorIndex_QueryWhileBuilding_FullScansThenUsesIndex"]
- **Scope:** "client types: go only; database: badger in-memory"

A vector index created on a populated collection builds asynchronously; while building, similarity queries full-scan with correct results, and once ready they use the index (index fetches > 0).

# `go.index.vector.fallback-warning`: Warning when a similarity query cannot use the vector index

- **Parents:** ["go.index.vector"]
- **Go-Tests:** ["index/vector_fallback_warning_test.go::TestVectorIndexWarning_MultipleSimilarityFields_ReportsWarning", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_NoLimit_ReportsWarning", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_NoVectorIndexOnField_ReportsNoWarning", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_OrderedAscending_ReportsWarning", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_QueryUsesIndex_ReportsNoWarning", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_SelectiveFilter_StillReturnsMatchingDocs", "index/vector_fallback_warning_test.go::TestVectorIndexWarning_WithFilter_ReportsWarning"]

A similarity query on a vector-indexed field that full-scans returns correct results plus a GraphQL warning with code `VECTOR_INDEX_UNUSED` and detail `{field: "vector", reason}` where reason is `notOrderedBySimilarityDesc`, `noLimit`, `filter` or `multipleSimilarityFields`. A query that uses the index, or a field with no vector index, gets no warning; a selective filter still returns the matching documents.

# `go.index.vector.replication`: Replicated documents are searchable

- **Parents:** ["go.index.vector"]
- **Go-Tests:** ["index/vector_p2p_test.go::TestVectorIndexP2P_ReplicatedDoc_IsSearchableOnReplica"]
- **Scope:** "cross-version runs only from v1.1.0 (first release with vector indexes)"

A document written on one peer and synced to another is added to the replica's vector index, so a similarity query on the replica returns it.

# `go.txn`: Explicit transactions

- **Parents:** ["go"]

Client operations run inside an explicit transaction (`TransactionID`) and their visibility before and after commit.

# `go.txn.docs`: Document operations in transactions

- **Parents:** ["go.txn"]

Document writes and GraphQL requests inside a transaction.

# `go.txn.docs.commit-visible`: Committed document writes are visible

- **Parents:** ["go.txn.docs"]
- **Go-Tests:** ["txn/add_doc_test.go::TestTxn_AddDoc_WithCommit_Succeeds", "txn/add_doc_test.go::TestTxn_AddDoc_InsideTxnWithAddCollection_WithCommit_Succeeds", "txn/update_doc_test.go::TestTxn_UpdateDoc_WithCommit_Succeeds", "txn/update_doc_test.go::TestTxn_UpdateDocWithFilter_WithCommit_Succeeds", "txn/delete_doc_test.go::TestTxn_DeleteDoc_WithCommit_Succeeds", "txn/request_test.go::TestTxn_Request_WithCommit_Succeeds", "txn/truncate_test.go::TestCollectionTruncate_WithCommit_Succeeds"]

After `CommitTransaction`, documents added, updated (by ID or filter), deleted, created via a GraphQL mutation request, or removed by `Truncate` inside the transaction are reflected in queries outside it; a collection and its documents added in one transaction are both visible.

# `go.txn.docs.uncommitted-invisible`: Uncommitted document writes are invisible

- **Parents:** ["go.txn.docs"]
- **Go-Tests:** ["txn/add_doc_test.go::TestTxn_AddDoc_WithoutCommit_EmptyResults", "txn/update_doc_test.go::TestTxn_UpdateDoc_WithoutCommit_DoesNotUpdateDocument", "txn/update_doc_test.go::TestTxn_UpdateDocWithFilter_WithoutCommit_DoesNotUpdateDocument", "txn/delete_doc_test.go::TestTxn_DeleteDoc_WithoutCommit_DoesNotDelete", "txn/request_test.go::TestTxn_Request_WithoutCommit_EmptyResults"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442)"

Before commit, documents added, updated (by ID or filter), deleted or created by a mutation inside a transaction are not visible to queries outside it, which still return the previous state.

# `go.txn.docs.read-own-writes`: Transactions see their own document writes

- **Parents:** ["go.txn.docs"]
- **Go-Tests:** ["txn/update_doc_test.go::TestTxn_UpdateDoc_ExhibitsTransactionalIsolation_Succeeds", "txn/update_doc_test.go::TestTxn_UpdateWithFilter_ExhibitsTransactionalIsolation_Succeeds", "txn/delete_doc_test.go::TestTxn_DeleteDoc_ExhibitsTransactionalIsolation_Succeeds"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442), except TestTxn_UpdateWithFilter_ExhibitsTransactionalIsolation_Succeeds which runs on all"

Inside a transaction, `UpdateDoc`, update-with-filter and `DeleteDoc` operate on documents created earlier in the same transaction, and the result is visible after commit.

# `go.txn.definitions`: Schema, view and lens operations in transactions

- **Parents:** ["go.txn"]

Collection, view, lens and migration changes inside a transaction.

# `go.txn.definitions.commit-visible`: Committed definition changes take effect

- **Parents:** ["go.txn.definitions"]
- **Go-Tests:** ["txn/add_collection_test.go::TestTxn_AddCollection_WithCommit_Succeeds", "txn/patch_collection_test.go::TestTxn_PatchCollection_WithCommit_Succeeds", "txn/set_active_collection_version_test.go::TestTxn_SetActiveCollectionVersion_WithCommit_Succeeds", "txn/add_view_test.go::TestTxn_AddView_WithCommit_Succeeds", "txn/refresh_view_test.go::TestTxn_RefreshView_WithCommit_Succeeds", "txn/add_lens_test.go::TestTxn_AddLens_WithCommit_Succeeds"]
- **Scope:** "TestTxn_RefreshView_WithCommit_Succeeds: materialized views only"

After commit, a collection added, a collection patch, `SetActiveCollectionVersion`, a view added or refreshed, and a lens added inside the transaction are in effect for later requests.

# `go.txn.definitions.uncommitted-invisible`: Uncommitted definition changes do not take effect

- **Parents:** ["go.txn.definitions"]
- **Go-Tests:** ["txn/add_collection_test.go::TestTxn_AddCollection_WithoutCommit_EmptyResults", "txn/patch_collection_test.go::TestTxn_PatchCollection_WithoutCommit_PatchNotApplied", "txn/set_active_collection_version_test.go::TestTxn_SetActiveCollectionVersion_WithoutCommit_VersionNotChanged", "txn/add_view_test.go::TestTxn_AddView_WithoutCommit_Fails", "txn/add_lens_test.go::TestTxn_AddLens_WithoutCommit_Fails", "txn/set_migration_test.go::TestTxn_SetMigration_WithoutCommit_Succeeds"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442)"

Without commit, a collection added in a transaction is not listed outside it, a patched field fails with `Cannot query field "name" on type "Users".`, an unswitched version field fails with `Cannot query field "email" on type "Users".`, a view fails with `Cannot query field "UserView" on type "Query".`, a lens fails with `lens CID not found`, and a migration set in the transaction is not applied (field reads null).

# `go.txn.definitions.same-txn-visibility`: Definition reads see only their own transaction

- **Parents:** ["go.txn.definitions"]
- **Go-Tests:** ["txn/get_collections_test.go::TestTxn_GetCollections_InsideTxnWithAddSchema_Succeeds", "txn/get_collections_test.go::TestTxn_GetCollections_InsideTxnWithoutAddSchema_NoCollections", "txn/list_lenses_test.go::TestTxn_ListLenses_InsideTxnWithAddLens_Succeeds", "txn/list_lenses_test.go::TestTxn_ListLenses_InsideTxnWithoutAddLens_NoLenses"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442), except TestTxn_GetCollections_InsideTxnWithAddSchema_Succeeds which runs on all"

`GetCollections` and `ListLenses` inside the transaction that added a collection or lens return it; run in a different transaction they return nothing.

# `go.txn.indexes`: Index operations in transactions

- **Parents:** ["go.txn"]

Secondary and encrypted index changes inside a transaction.

# `go.txn.indexes.commit-visible`: Committed index changes take effect

- **Parents:** ["go.txn.indexes"]
- **Go-Tests:** ["txn/new_index_test.go::TestTxn_NewIndex_WithCommit_Succeeds", "txn/delete_index_test.go::TestTxn_DeleteIndex_WithCommit_Succeeds", "txn/new_encrypted_index_test.go::TestTxn_NewEncryptedIndex_WithCommit_Succeeds", "txn/delete_encrypted_index_test.go::TestTxn_DeleteEncryptedIndex_WithCommit_Succeeds"]

After commit, an index or encrypted index created inside the transaction is listed, and one deleted inside it is gone.

# `go.txn.indexes.uncommitted-invisible`: Uncommitted index changes are invisible

- **Parents:** ["go.txn.indexes"]
- **Go-Tests:** ["txn/new_index_test.go::TestTxn_NewIndex_WithoutCommit_NoIndexes", "txn/delete_index_test.go::TestTxn_DeleteIndex_WithoutCommit_DoesNotDelete", "txn/new_encrypted_index_test.go::TestTxn_NewEncryptedIndex_WithoutCommit_NoIndexes", "txn/delete_encrypted_index_test.go::TestTxn_DeleteEncryptedIndex_WithoutCommit_DoesNotDelete"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442)"

Before commit, an index or encrypted index created in a transaction is not listed outside it, and one deleted in a transaction is still listed outside it.

# `go.txn.indexes.read-own-writes`: Index operations see their own transaction

- **Parents:** ["go.txn.indexes"]
- **Go-Tests:** ["txn/new_index_test.go::TestTxn_NewIndex_ExhibitsTransactionalIsolation_Succeeds", "txn/delete_index_test.go::TestTxn_DeleteIndex_ExhibitsTransactionalIsolation_Succeeds", "txn/delete_encrypted_index_test.go::TestTxn_DeleteEncryptedIndex_ExhibitsTransactionalIsolation_Succeeds", "txn/list_encrypted_indexes_test.go::TestTxn_AddEncryptedIndex_ExhibitsTransactionalIsolation_Succeeds", "txn/list_indexes_test.go::TestTxn_ListIndexes_InsideTxn_Succeeds", "txn/new_encrypted_index_test.go::TestTxn_ListEncryptedIndexes_InsideTxn_Succeeds"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442)"

Inside one transaction, indexes (plain or encrypted) can be created on a collection added in it and over documents added in it, listed, and deleted; `ListIndexes` and `ListEncryptedIndexes` in that transaction return them.

# `go.txn.verify-signature`: Block signature verification in transactions

- **Parents:** ["go.txn"]
- **Go-Tests:** ["txn/verify_signature_test.go::TestTxn_VerifyBlockSignature_InsideTxn_Succeeds", "txn/verify_signature_test.go::TestTxn_VerifyBlockSignature_OutsideTxn_Fails"]
- **Scope:** "database types: badger file, badger in-memory, defra in-memory (not leveldb, Go issue #4442)"

`VerifyBlockSignature` inside the transaction that created a signed block succeeds; outside it, before commit, it fails with `ipld: could not find`.

# `go.txn.ttl`: HTTP transaction TTL

- **Parents:** ["go.txn"]
- **Go-Tests:** ["txn/ttl_test.go::TestTxnTTL_GivenIdleHTTPTransaction_DiscardsTransaction", "txn/ttl_test.go::TestTxnTTL_GivenHTTPTransactionActivity_ExtendsTTL", "txn/ttl_test.go::TestTxnTTL_GivenCommittedHTTPTransaction_RemovesCacheEntry", "txn/ttl_test.go::TestTxnTTL_GivenDiscardedHTTPTransaction_RemovesCacheEntry", "txn/ttl_test.go::TestTxnTTL_GivenExpiredHTTPTransaction_OnGetCollections_ReturnsNonGraphQLError"]
- **Scope:** "client types: http, cli"

An HTTP/CLI transaction idle longer than the configured TTL is discarded: later requests with its ID (GraphQL or `GetCollections`) fail with `transaction not found` (a plain, non-GraphQL error for collection calls) and its writes are lost; activity extends the TTL. Committing or discarding a transaction removes it at once, so its ID then returns `transaction not found`.

# `go.subscription`: GraphQL subscriptions

- **Parents:** ["go"]

`subscription { ... }` requests that stream document or commit events.

# `go.subscription.document-events`: Document change events

- **Parents:** ["go.subscription"]
- **Go-Tests:** ["subscription/subscription_test.go::TestSubscriptionWithAddMutations", "subscription/subscription_test.go::TestSubscriptionWithUpdateMutations", "subscription/subscription_test.go::TestSubscriptionWithUpdateAllMutations", "subscription/subscription_test.go::TestSubscription_WithCounterCRDT_ShouldSucceed", "subscription/subscription_test.go::TestSubscription_WithDeleteOperation_ShouldSucceed"]

`subscription { User { ... } }` emits one event per created, updated (including an update of all documents) or deleted document carrying its current state; with `showDeleted: true` the delete event has `_deleted: true`; counter CRDT fields report the accumulated value.

# `go.subscription.filtering`: Filtered subscriptions

- **Parents:** ["go.subscription"]
- **Go-Tests:** ["subscription/subscription_test.go::TestSubscriptionWithFilterAndOneAddMutation", "subscription/subscription_test.go::TestSubscriptionWithFilterAndOneAddMutationOutsideFilter", "subscription/subscription_test.go::TestSubscriptionWithFilterAndAddMutations", "subscription/subscription_test.go::TestSubscription_WithDocIDFilter_ShouldOnlyGetUpdatesForThatDocID"]

A subscription with `filter` emits only documents matching it (none when a mutation falls outside it), and one with `docID` emits only that document's changes.

# `go.subscription.close`: Closing with an open subscription

- **Parents:** ["go.subscription"]
- **Go-Tests:** ["subscription/subscription_test.go::TestSubscription_WithClose_WontBlock"]

Closing the node while a subscription is open does not block.

# `go.subscription.commits`: Commit subscriptions

- **Parents:** ["go.subscription"]
- **Go-Tests:** ["subscription/with_commit_test.go::TestCommitSubscription_WithAddMutations_ReturnCommits", "subscription/with_commit_test.go::TestCommitSubscription_WithCommitLinksAddMutations_ValidLinks", "subscription/with_commit_test.go::TestCommitSubscription_WithDocFilterAndMultipleMutations_FilteredDoc"]

`subscription { _commits { cid links { cid fieldName } heads { cid } } }` emits one event per mutation with the document's composite commit, its field links and heads; with `docID` it emits only that document's commits.

# `go.version`: Cross-version test binaries

- **Parents:** ["go"]

Harness package that resolves a DefraDB release to a downloaded executable for cross-version P2P tests; not node behaviour.

# `go.version.platform`: Release asset names per platform

- **Parents:** ["go.version"]
- **Go-Tests:** ["version/version_test.go::TestAssetName", "version/version_test.go::TestBinaryPath_UnsupportedPlatform_Skips"]

`assetName` maps a version and platform to `defradb_<version>_<os>_<arch>` (amd64 as `x86_64`, `.exe` on Windows, with or without a `v` prefix); unsupported platforms make `BinaryPath` skip without downloading.

# `go.version.cache`: Binary cache

- **Parents:** ["go.version"]
- **Go-Tests:** ["version/version_test.go::TestBinaryPath_CacheHit_SkipsDownload", "version/version_test.go::TestBinaryPath_CacheMiss_Downloads", "version/version_test.go::TestBinaryPath_DownloadError_IsHardError"]

`BinaryPath` returns a cached binary without downloading, downloads on a miss and marks the file executable, and returns a hard error (no skip) when the download fails.

# `go.version.download`: Release download

- **Parents:** ["go.version"]
- **Go-Tests:** ["version/httpdownload_test.go::TestHttpDownload_Success", "version/httpdownload_test.go::TestHttpDownload_NotFound_ReturnsError", "version/httpdownload_test.go::TestHttpDownload_Redirect_Follows", "version/httpdownload_test.go::TestHttpDownload_BodyCopyFailure_CleansUpTempFile"]

`httpDownload` fetches `/sourcenetwork/defradb/releases/download/<tag>/<asset>`, follows redirects, returns an error on 404 or a truncated body, and leaves no partial file behind.

# `go.issues`: Regression tests for specific Go issues

- **Parents:** ["go"]

Tests that document known Go behaviour tied to an issue number.

# `go.issues.pncounter-overflow-divergence`: PN counter float overflow diverges across peers (#2566)

- **Parents:** ["go.issues"]
- **Go-Tests:** ["issues/2566_test.go::TestP2PUpdate_WithPNCounterSimultaneousOverflowIncrement_DoesNotReachConsitency", "issues/2566_test.go::TestP2PUpdate_WithPNCounterSimultaneousOverflowDecrement_DoesNotReachConsitency"]
- **Scope:** "client types: go only"
- **Go-Bug-Suspect:** "Asserts non-convergence of a CRDT (Go issue #2566); Rust should converge rather than copy this."

Two peers apply `+MaxFloat64` and `-MaxFloat64` to a Float `pncounter` in different orders; after sync one reads `+Inf` and the other `1.7976931348623155e+307`, so the replicas do not converge.

# `go.issues.pncounter-overflow-serialization`: Infinite PN counter values break HTTP/CLI reads (#2569)

- **Parents:** ["go.issues"]
- **Go-Tests:** ["issues/2569_test.go::TestP2PUpdate_WithPNCounterFloatOverflowIncrement_PreventsQuerying", "issues/2569_test.go::TestP2PUpdate_WithPNCounterFloatOverflowDecrement_PreventsQuerying", "issues/2569_test.go::TestP2PUpdate_WithPNCounterFloatOverflow_PreventsCollectionGet"]
- **Scope:** "client types: http, cli; TestP2PUpdate_WithPNCounterFloatOverflow_PreventsCollectionGet: collection save and collection named mutation types"
- **Go-Bug-Suspect:** "Documents Go issue #2569: \u00b1Inf cannot be serialised to JSON so reads fail; the asserted errors are artifacts of that bug and of the test harness."

When a Float `pncounter` overflows to ±Inf, querying it over HTTP or CLI fails with `unexpected end of JSON input`, and a collection `Get` of the document returns an empty body (surfacing as `cannot parse JSON: cannot parse empty string`).

# `go.node`: Node-level behaviour

- **Parents:** ["go"]

Properties of a running node outside the data model.

# `go.node.identity`: Node identity

- **Parents:** ["go.node"]
- **Go-Tests:** ["node/identity_test.go::TestNodeIdentity_NodeIdentity_Succeed"]

`GetNodeIdentity` on each node returns the identity the node was configured with.
