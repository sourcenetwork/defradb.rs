//! GraphQL introspection schema cache.
//!
//! A cache keyed on the node's schema epoch: a counter bumped every
//! time the node's collection set changes, starting from zero at startup.
//! The newest build is published whole through an `Atom`, so a reader never
//! waits, and a build for an older epoch never replaces a newer one.
//!
//! Transactions are not cached: they can see uncommitted schema changes that
//! no committed epoch describes.
//!
//! Identity is not part of the key because introspection takes no identity
//! and its output is not ACP-filtered. If that changes, identity must join
//! the key.

use async_graphql::dynamic::Schema;
use kovan::Atom;

/// One built schema, keyed by the epoch it was built for.
pub(crate) struct IntrospectionSchemaCache {
    enabled: bool,
    head: Atom<Option<(u64, Schema)>>,
    #[cfg(test)]
    builds: std::sync::atomic::AtomicU64,
}

impl IntrospectionSchemaCache {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            head: Atom::new(None),
            #[cfg(test)]
            builds: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn head_get(&self, epoch: u64) -> Option<Schema> {
        self.head.peek(|head| match head {
            Some((built, schema)) if *built == epoch => Some(schema.clone()),
            _ => None,
        })
    }

    /// Publishes `schema`, built at `epoch`, unless a build for a newer epoch
    /// is published already: epochs only grow, so the older build could never
    /// be served again.
    pub(crate) fn head_put(&self, epoch: u64, schema: Schema) {
        self.head.rcu(|head| match head {
            Some((newer, _)) if *newer > epoch => head.clone(),
            _ => Some((epoch, schema.clone())),
        });
    }

    pub(crate) fn note_build(&self) {
        #[cfg(test)]
        self.builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// How many schema builds have run, for tests that assert the cache is
    /// serving rather than rebuilding.
    #[cfg(test)]
    pub(crate) fn build_count(&self) -> u64 {
        self.builds.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for IntrospectionSchemaCache {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::fetcher::CollectionProvider;
    use crate::test_utils::MockFetcher;
    use crate::{QueryRunner, StaticCollectionProvider};
    use async_graphql::dynamic::{Field, FieldFuture, Object, TypeRef};
    use async_trait::async_trait;
    use schema::CollectionVersion;
    use std::sync::Arc;

    fn users_collection(version: &str) -> CollectionVersion {
        CollectionVersion::new("Users", version, "users", vec![])
    }

    fn runner_with(
        enabled: bool,
        provider: Arc<dyn CollectionProvider>,
    ) -> QueryRunner<MockFetcher> {
        QueryRunner::with_provider(MockFetcher::new(), provider).with_introspection_cache(enabled)
    }

    const QUERY: &str = r#"{ __type(name: "Users") { name } }"#;

    /// A schema whose query type is named `name`, to tell builds apart.
    fn schema_named(name: &str) -> Schema {
        let query = Object::new(name).field(Field::new("ok", TypeRef::named("Boolean"), |_| {
            FieldFuture::new(async { Ok(Some(async_graphql::Value::Null)) })
        }));
        Schema::build(name, None, None)
            .register(query)
            .finish()
            .expect("a one-field schema builds")
    }

    /// The query type of the schema served at `epoch`.
    fn served(cache: &IntrospectionSchemaCache, epoch: u64) -> Option<&'static str> {
        cache.head_get(epoch).map(|schema| {
            if schema.sdl().contains("type Newer") {
                "Newer"
            } else {
                "Older"
            }
        })
    }

    #[test]
    fn serves_only_the_epoch_it_was_built_for() {
        let cache = IntrospectionSchemaCache::new(true);
        assert!(cache.head_get(0).is_none());

        cache.head_put(1, schema_named("Newer"));
        assert_eq!(served(&cache, 1), Some("Newer"));
        assert_eq!(served(&cache, 0), None);
        assert_eq!(served(&cache, 2), None);
    }

    /// A slow build for an older epoch that finishes after a newer one must
    /// not evict it, or the newer epoch pays for a rebuild.
    #[test]
    fn an_older_build_never_replaces_a_newer_one() {
        let cache = IntrospectionSchemaCache::new(true);
        cache.head_put(5, schema_named("Newer"));
        cache.head_put(3, schema_named("Older"));

        assert_eq!(served(&cache, 5), Some("Newer"));
        assert_eq!(served(&cache, 3), None);

        cache.head_put(6, schema_named("Older"));
        assert_eq!(served(&cache, 5), None);
        assert_eq!(served(&cache, 6), Some("Older"));
    }

    #[test]
    fn builds_once_for_an_unchanged_view() {
        workers::block_on(async {
            let provider = Arc::new(StaticCollectionProvider::new(vec![users_collection("v1")]));
            let runner = runner_with(true, provider);

            let first = runner.execute_introspection(QUERY).await.unwrap();
            let second = runner.execute_introspection(QUERY).await.unwrap();

            assert_eq!(first, second);
            assert_eq!(runner.introspection_build_count(), 1);
        })
        .unwrap()
    }

    #[test]
    fn disabled_builds_every_time() {
        workers::block_on(async {
            let provider = Arc::new(StaticCollectionProvider::new(vec![users_collection("v1")]));
            let runner = runner_with(false, provider);

            runner.execute_introspection(QUERY).await.unwrap();
            runner.execute_introspection(QUERY).await.unwrap();

            assert_eq!(runner.introspection_build_count(), 2);
        })
        .unwrap()
    }

    /// A provider that cannot prove an epoch, like a transaction's. Content
    /// is swappable to model an uncommitted schema change.
    struct EpochlessProvider {
        collections: Atom<Vec<Arc<CollectionVersion>>>,
    }

    impl EpochlessProvider {
        fn new(collections: Vec<CollectionVersion>) -> Self {
            Self {
                collections: Atom::new(collections.into_iter().map(Arc::new).collect()),
            }
        }

        fn set(&self, collections: Vec<CollectionVersion>) {
            self.collections
                .store(collections.into_iter().map(Arc::new).collect());
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl CollectionProvider for EpochlessProvider {
        async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
            Ok(self
                .collections
                .load()
                .iter()
                .find(|c| c.name == name)
                .cloned())
        }

        async fn list_collections(&self) -> Result<Vec<String>> {
            Ok(self
                .collections
                .load()
                .iter()
                .map(|c| c.name.clone())
                .collect())
        }
    }

    /// The uncached-by-design case: no epoch, so every introspection builds,
    /// and a schema change mid-stream is always visible.
    #[test]
    fn epochless_provider_rebuilds_and_sees_changes() {
        workers::block_on(async {
            let provider = Arc::new(EpochlessProvider::new(vec![users_collection("v1")]));
            let runner = runner_with(true, provider.clone());

            runner.execute_introspection(QUERY).await.unwrap();
            runner.execute_introspection(QUERY).await.unwrap();
            assert_eq!(runner.introspection_build_count(), 2);

            provider.set(vec![users_collection("v2")]);
            runner.execute_introspection(QUERY).await.unwrap();
            assert_eq!(runner.introspection_build_count(), 3);
        })
        .unwrap()
    }
}
