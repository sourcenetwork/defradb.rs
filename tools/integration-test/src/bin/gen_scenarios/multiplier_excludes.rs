//! Go tests that do not run under a multiplier, by Go source file and test:
//! the multipliers each excludes and the comment its generated test carries.
//! Go's own `MultiplierExcludes` are not carried over yet:
//! https://github.com/sourcenetwork/defradb.rs/issues/3046

const SECONDARY_INDEX: &[&str] = &["secondary-index"];

const ENTRIES: &[(&str, &str, &[&str], &str)] = &[
    (
        "tests/integration/query/one_to_many_to_one/with_order_limit_test.go",
        "TestOneToManyToOneDeepOrderBySubTypeOfBothDescAndAsc",
        SECONDARY_INDEX,
        "The inverted join drops books without a publisher, as Go's: \
         https://github.com/sourcenetwork/defradb/issues/4551",
    ),
    (
        "tests/integration/query/many_to_many/simple_test.go",
        "TestManyToMany_QueryFromJoinCollection_ShouldSucceed",
        SECONDARY_INDEX,
        "TODO(https://github.com/sourcenetwork/defradb.rs/issues/3042): \
         the join drops a relation when both foreign keys are indexed.",
    ),
    (
        "tests/integration/collection_version/migrations/query/simple_test.go",
        "TestCollectionMigrationQuery_WithMigrationsAcrossMultipleVersions_AppliesAllMigrations",
        SECONDARY_INDEX,
        LENS_3044,
    ),
    (
        "tests/integration/collection_version/migrations/query/with_incremental_migrations_test.go",
        "TestCollectionMigrationQuery_ThreeIncrementalMigrations_ShouldMigrateAcrossAllThree",
        SECONDARY_INDEX,
        LENS_3044,
    ),
    (
        "tests/integration/collection_version/migrations/query/with_incremental_migrations_test.go",
        "TestCollectionMigrationQuery_IntermediateMigrationRegisteredLast_ShouldProduceFullyMigratedResults",
        SECONDARY_INDEX,
        LENS_3044,
    ),
    (
        "tests/integration/collection_version/migrations/query/with_incremental_migrations_test.go",
        "TestCollectionMigrationQuery_IntermediateStampSurvivesRestart_ShouldRemigrateAfterRestart",
        SECONDARY_INDEX,
        LENS_3044,
    ),
    (
        "tests/integration/collection_version/migrations/query/with_set_default_test.go",
        "TestCollectionMigrationQuery_WithSetDefaultToOriginalVersionThatDocWasAddedAt_ClearsMigrations",
        SECONDARY_INDEX,
        LENS_3044,
    ),
    (
        "tests/integration/collection_version/updates/remove/col_source_transform_test.go",
        "TestColVersionUpdateRemoveCollectionSourceTransform",
        SECONDARY_INDEX,
        "TODO(https://github.com/sourcenetwork/defradb.rs/issues/3052): removing the transform leaves the migrated value the reindex wrote.",
    ),
    (
        "tests/integration/query/one_to_many/with_sum_filter_order_test.go",
        "TestOneToManyLimitAscOrderSumOfSubTypeAndLimitAscOrderFieldsOfSubtype",
        SECONDARY_INDEX,
        CHILD_PAGE,
    ),
    (
        "tests/integration/query/one_to_many/with_sum_filter_order_test.go",
        "TestOneToManyLimitDescOrderSumOfSubTypeAndLimitAscOrderFieldsOfSubtype",
        SECONDARY_INDEX,
        CHILD_PAGE,
    ),
    (
        "tests/integration/query/one_to_many/with_sum_limit_offset_order_test.go",
        "TestQueryOneToManyWithSumWithLimitWithOffsetWithOrderDescAndRenderedChildrenOrderedAsc",
        SECONDARY_INDEX,
        CHILD_PAGE,
    ),
];

const CHILD_PAGE: &str =
    "TODO(https://github.com/sourcenetwork/defradb.rs/issues/3051): a nested limit bounds the whole child index scan, not each parent's children.";

const LENS_3044: &str = "TODO(https://github.com/sourcenetwork/defradb.rs/issues/3044): \
                         with indexes, a query after a migration returns unmigrated fields.";

/// The multipliers Go test `test` in `path` does not run under, and why.
pub fn lookup(path: &str, test: &str) -> Option<(&'static [&'static str], &'static str)> {
    ENTRIES
        .iter()
        .find(|(p, t, _, _)| *p == path && *t == test)
        .map(|(_, _, names, reason)| (*names, *reason))
}
