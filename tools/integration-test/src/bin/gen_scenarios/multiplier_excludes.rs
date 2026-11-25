//! Which multipliers each generated test does not run under. A test carries
//! Go's own `MultiplierExcludes`, read at the parity pin, unless defradb.rs
//! runs it anyway (`RUNS_HERE`); a defradb.rs reason (`ENTRIES`) replaces Go's.

use super::go_excludes::GoExclude;

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

/// The Go exclusions defradb.rs does not share, and why the test runs here.
const RUNS_HERE: &[(GoReason, &str)] = &[
    (GoReason::Cites(GO_4439), COUNTERS),
    (
        GoReason::Test(
            "tests/integration/net/simple/peer/crdt/pncounter_convergence_test.go",
            "TestP2PUpdate_WithPNCounterFiveNodesRepeatedUpdates_AllConverge",
        ),
        COUNTERS,
    ),
    (
        GoReason::Cites("https://github.com/sourcenetwork/defradb/issues/4722"),
        "defradb.rs patches a collection whose fields have dependent indexes.",
    ),
];

const GO_4439: &str = "https://github.com/sourcenetwork/defradb/issues/4439";

const COUNTERS: &str = "defradb.rs indexes counter fields.";

/// Which Go exclusions a `RUNS_HERE` entry covers.
enum GoReason {
    /// Every exclusion whose Go comment cites this link.
    Cites(&'static str),
    /// The exclusion of one Go test, by source file and test.
    Test(&'static str, &'static str),
}

/// The multipliers a generated test does not run under and the comment it
/// carries, given Go test `test` in `path` and the exclusions Go lists for it.
pub fn resolve(path: &str, test: &str, go: &[GoExclude]) -> Option<(Vec<&'static str>, String)> {
    if let Some((_, _, names, reason)) =
        ENTRIES.iter().find(|(p, t, _, _)| *p == path && *t == test)
    {
        return Some((names.to_vec(), (*reason).to_owned()));
    }
    let carried: Vec<&GoExclude> = go
        .iter()
        .filter(|exclude| runs_here(path, test, exclude).is_none())
        .collect();
    let names: Vec<&'static str> = carried.iter().map(|exclude| exclude.multiplier).collect();
    if names.is_empty() {
        return None;
    }
    let mut reasons: Vec<&str> = Vec::new();
    for exclude in &carried {
        if !exclude.reason.is_empty() && !reasons.contains(&exclude.reason.as_str()) {
            reasons.push(&exclude.reason);
        }
    }
    let reason = match reasons.is_empty() {
        true => "Excluded as in Go.".to_owned(),
        false => format!("Excluded as in Go: {}", reasons.join(" ")),
    };
    Some((names, reason))
}

/// Why defradb.rs runs a test Go excludes, if it does.
pub fn runs_here(path: &str, test: &str, exclude: &GoExclude) -> Option<&'static str> {
    RUNS_HERE
        .iter()
        .find(|(go, _)| match go {
            GoReason::Cites(link) => exclude.reason.contains(link),
            GoReason::Test(p, t) => *p == path && *t == test,
        })
        .map(|(_, why)| *why)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn excluded(reason: &str) -> Vec<GoExclude> {
        vec![GoExclude {
            multiplier: "secondary-index",
            reason: reason.to_owned(),
        }]
    }

    #[test]
    fn carries_a_go_exclusion_with_its_comment() {
        assert_eq!(
            resolve("a_test.go", "TestA", &excluded("The join is inverted.")),
            Some((
                vec!["secondary-index"],
                "Excluded as in Go: The join is inverted.".to_owned()
            ))
        );
    }

    #[test]
    fn runs_a_go_exclusion_defradb_rs_does_not_share() {
        let reason = format!("Counters cannot be indexed. {GO_4439}");
        assert_eq!(resolve("a_test.go", "TestA", &excluded(&reason)), None);
    }

    #[test]
    fn a_defradb_rs_reason_replaces_go_s() {
        let (path, test, _, reason) = ENTRIES[0];
        assert_eq!(
            resolve(path, test, &excluded("Go's reason.")),
            Some((vec!["secondary-index"], reason.to_owned()))
        );
        assert_eq!(
            resolve(path, test, &[]),
            Some((vec!["secondary-index"], reason.to_owned()))
        );
    }

    #[test]
    fn no_exclusion_without_one() {
        assert_eq!(resolve("a_test.go", "TestA", &[]), None);
    }
}
