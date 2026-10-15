//! GraphQL introspection schema cache.
//!
//! A cache keyed on the node's schema epoch: a counter bumped every
//! time the node's collection set changes, starting from zero at startup.
//!
//! Transactions are not cached: they can see uncommitted schema changes that
//! no committed epoch describes.
//!
//! Identity is not part of the key because introspection takes no identity
//! and its output is not ACP-filtered. If that changes, identity must join
//! the key.

use async_graphql::dynamic::Schema;
use std::sync::Mutex;

/// One built schema, keyed by the epoch it was built for.
pub(crate) struct IntrospectionSchemaCache {
    enabled: bool,
    head: Mutex<Option<(u64, Schema)>>,
    #[cfg(test)]
    builds: std::sync::atomic::AtomicU64,
}

impl IntrospectionSchemaCache {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            head: Mutex::new(None),
            #[cfg(test)]
            builds: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn head_get(&self, epoch: u64) -> Option<Schema> {
        let head = self.head.lock().expect("introspection cache poisoned");
        match head.as_ref() {
            Some((e, schema)) if *e == epoch => Some(schema.clone()),
            _ => None,
        }
    }

    pub(crate) fn head_put(&self, epoch: u64, schema: Schema) {
        let mut head = self.head.lock().expect("introspection cache poisoned");
        *head = Some((epoch, schema));
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

    #[tokio::test]
    async fn builds_once_for_an_unchanged_view() {
        let provider = Arc::new(StaticCollectionProvider::new(vec![users_collection("v1")]));
        let runner = runner_with(true, provider);

        let first = runner.execute_introspection(QUERY).await.unwrap();
        let second = runner.execute_introspection(QUERY).await.unwrap();

        assert_eq!(first, second);
        assert_eq!(runner.introspection_build_count(), 1);
    }

    #[tokio::test]
    async fn disabled_builds_every_time() {
        let provider = Arc::new(StaticCollectionProvider::new(vec![users_collection("v1")]));
        let runner = runner_with(false, provider);

        runner.execute_introspection(QUERY).await.unwrap();
        runner.execute_introspection(QUERY).await.unwrap();

        assert_eq!(runner.introspection_build_count(), 2);
    }

    /// A provider that cannot prove an epoch, like a transaction's. Content
    /// is swappable to model an uncommitted schema change.
    struct EpochlessProvider {
        collections: Mutex<Vec<Arc<CollectionVersion>>>,
    }

    impl EpochlessProvider {
        fn new(collections: Vec<CollectionVersion>) -> Self {
            Self {
                collections: Mutex::new(collections.into_iter().map(Arc::new).collect()),
            }
        }

        fn set(&self, collections: Vec<CollectionVersion>) {
            *self.collections.lock().unwrap() = collections.into_iter().map(Arc::new).collect();
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl CollectionProvider for EpochlessProvider {
        async fn get_collection(&self, name: &str) -> Result<Option<Arc<CollectionVersion>>> {
            Ok(self
                .collections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.name == name)
                .cloned())
        }

        async fn list_collections(&self) -> Result<Vec<String>> {
            Ok(self
                .collections
                .lock()
                .unwrap()
                .iter()
                .map(|c| c.name.clone())
                .collect())
        }
    }

    /// The uncached-by-design case: no epoch, so every introspection builds,
    /// and a schema change mid-stream is always visible.
    #[tokio::test]
    async fn epochless_provider_rebuilds_and_sees_changes() {
        let provider = Arc::new(EpochlessProvider::new(vec![users_collection("v1")]));
        let runner = runner_with(true, provider.clone());

        runner.execute_introspection(QUERY).await.unwrap();
        runner.execute_introspection(QUERY).await.unwrap();
        assert_eq!(runner.introspection_build_count(), 2);

        provider.set(vec![users_collection("v2")]);
        runner.execute_introspection(QUERY).await.unwrap();
        assert_eq!(runner.introspection_build_count(), 3);
    }
}
