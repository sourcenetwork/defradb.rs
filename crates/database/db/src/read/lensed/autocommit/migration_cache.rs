//! The lensed fetcher's cache of migration contexts.
//!
//! A context (whether a collection version has lens migrations, and its
//! targeted history) is computed from the committed migration graph, so it
//! holds for one generation of that graph: a [`GenerationCache`] keyed by the
//! graph's committed generation, read without a lock.

use lens::TargetedHistoryLink;
use rapidhash::RapidHashMap;

use super::super::generation_cache::GenerationCache;

/// Whether a collection version has lens migrations, and its targeted
/// history when it has.
pub(super) type MigrationContext = (bool, Option<RapidHashMap<String, TargetedHistoryLink>>);

/// Migration contexts keyed by `"{collection_id}:{version_id}"`, one snapshot
/// per committed migration-graph generation.
pub(super) type MigrationCache = GenerationCache<MigrationContext>;

#[cfg(test)]
type Snapshot = super::super::generation_cache::Snapshot<MigrationContext>;

#[cfg(test)]
#[path = "../../../../tests/read/lensed_migration_cache.rs"]
mod tests;
