//! Transform store trait and types.
//!
//! Matches Go's lens/host-go/store.Store interface.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::RapidHashMap;

use crate::{Error, LensConfig, LensDoc, Result};

/// Unique identifier for a registered transform.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransformId(pub String);

impl TransformId {
    /// Create a new transform ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for TransformId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for TransformId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for TransformId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Stream of lens documents.
pub type LensDocStream = Pin<Box<dyn Stream<Item = LensDoc> + Send>>;

/// Stream of lens document results.
pub type LensDocResultStream = Pin<Box<dyn Stream<Item = Result<LensDoc>> + Send>>;

/// Store for managing lens transforms.
///
/// Matches Go's lens/host-go/store.Store interface.
#[async_trait]
pub trait TransformStore: Send + Sync {
    /// Create an empty store for one transaction's registrations. It shares
    /// this store's runtime (for WASM, the engine and its epoch clock), so a
    /// transaction never builds a runtime or a clock thread of its own.
    fn new_isolated_store(&self) -> Arc<dyn TransformStore>;

    /// Register a new lens transform.
    ///
    /// Returns the unique ID for the registered transform.
    async fn add(&self, config: LensConfig) -> Result<TransformId>;

    /// Register a lens transform under a specific ID.
    ///
    /// Used for P2P sync where the transform ID is the Go-generated IPLD CID.
    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> Result<()>;

    /// List all registered transforms.
    ///
    /// Returns a map of transform IDs to their lenses' modules, in order.
    async fn list(&self) -> Result<RapidHashMap<String, Vec<crate::LensModule>>>;

    /// Transform documents using a registered lens.
    ///
    /// Applies the forward transformation (source -> destination schema version).
    fn transform(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream>;

    /// Inverse transform documents using a registered lens.
    ///
    /// Applies the reverse transformation (destination -> source schema version).
    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream>;

    /// Transform one document, forward or inverse, and return the first
    /// document the lens yields.
    ///
    /// The call has its whole input in hand and its caller waits only on it,
    /// so a store may make it wait for room among the calls it bounds (the
    /// WASM store does). [`transform`](Self::transform) never waits for that
    /// room: a streaming caller may hold a call while it reads the source the
    /// call consumes, and that read may run lens calls of its own.
    async fn transform_doc(
        &self,
        id: &TransformId,
        doc: LensDoc,
        inverse: bool,
    ) -> Result<LensDoc> {
        let input: LensDocStream = Box::pin(futures::stream::iter([doc]));
        let output = if inverse {
            self.inverse(id, input)?
        } else {
            self.transform(id, input)?
        };
        first_output(output).await
    }

    /// Check if a transform exists.
    fn has_transform(&self, id: &TransformId) -> bool;

    /// Remove a registered transform.
    async fn remove(&self, id: &TransformId) -> Result<()>;
}

/// The first document `output` yields: what a call over one document returns.
pub(crate) async fn first_output<S>(output: S) -> Result<LensDoc>
where
    S: Stream<Item = Result<LensDoc>>,
{
    let mut output = std::pin::pin!(output);
    output
        .next()
        .await
        .ok_or_else(|| Error::Pipeline("transform produced no output".to_string()))?
}

/// In-memory transform store for testing.
pub struct MemoryTransformStore {
    transforms: HopscotchMap<TransformId, LensConfig, RandomState>,
}

impl Default for MemoryTransformStore {
    fn default() -> Self {
        Self {
            transforms: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl MemoryTransformStore {
    /// Create a new in-memory transform store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TransformStore for MemoryTransformStore {
    fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
        Arc::new(Self::new())
    }

    async fn add(&self, config: LensConfig) -> Result<TransformId> {
        use sha2::{Digest, Sha256};

        // Compute content-based ID for deduplication (matches Go's IPLD CID approach)
        // Hash only the lens modules, not the version IDs, so identical lens content
        // produces the same ID regardless of which versions it's associated with.
        let lenses_json = serde_json::to_vec(&config.lenses)
            .map_err(|e| Error::Pipeline(format!("failed to serialize lenses: {}", e)))?;
        let mut hasher = Sha256::new();
        hasher.update(&lenses_json);
        let hash = hasher.finalize();
        // Use "baf" prefix to mimic CID format, then 16 bytes of hash for uniqueness
        let id = TransformId::new(format!("baf{}", hex::encode(&hash[..16])));

        // Deduplication: if this config already exists, return the existing ID
        self.transforms.insert_if_absent(id.clone(), config);

        Ok(id)
    }

    async fn add_with_id(&self, id: TransformId, config: LensConfig) -> Result<()> {
        self.transforms.insert(id, config);
        Ok(())
    }

    async fn list(&self) -> Result<RapidHashMap<String, Vec<crate::LensModule>>> {
        let result = self
            .transforms
            .iter()
            .map(|(id, config)| (id.to_string(), config.lenses.clone()))
            .collect();

        Ok(result)
    }

    fn transform(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream> {
        if !self.has_transform(id) {
            return Err(Error::TransformNotFound(id.to_string()));
        }

        // In-memory store just passes through documents unchanged (for testing)
        Ok(Box::pin(docs.map(Ok)))
    }

    fn inverse(&self, id: &TransformId, docs: LensDocStream) -> Result<LensDocResultStream> {
        if !self.has_transform(id) {
            return Err(Error::TransformNotFound(id.to_string()));
        }

        // In-memory store just passes through documents unchanged (for testing)
        Ok(Box::pin(docs.map(Ok)))
    }

    fn has_transform(&self, id: &TransformId) -> bool {
        self.transforms.contains_key(id)
    }

    async fn remove(&self, id: &TransformId) -> Result<()> {
        if self.transforms.remove(id).is_none() {
            return Err(Error::TransformNotFound(id.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LensModule;

    #[test]
    fn test_memory_store_add_transform() {
        workers::block_on(async {
            let store = MemoryTransformStore::new();
            let config =
                LensConfig::new("v1", "v2", LensModule::from_path("/path/to/transform.wasm"));

            let id = store.add(config).await.unwrap();
            assert!(store.has_transform(&id));
        })
        .unwrap()
    }

    #[test]
    fn test_memory_store_remove_transform() {
        workers::block_on(async {
            let store = MemoryTransformStore::new();
            let config =
                LensConfig::new("v1", "v2", LensModule::from_path("/path/to/transform.wasm"));

            let id = store.add(config).await.unwrap();
            assert!(store.has_transform(&id));

            store.remove(&id).await.unwrap();
            assert!(!store.has_transform(&id));
        })
        .unwrap()
    }

    #[test]
    fn isolated_memory_store_starts_empty_and_stays_apart() {
        workers::block_on(async {
            let store = MemoryTransformStore::new();
            let config =
                LensConfig::new("v1", "v2", LensModule::from_path("/path/to/transform.wasm"));
            let id = store.add(config.clone()).await.unwrap();

            let isolated = store.new_isolated_store();
            assert!(!isolated.has_transform(&id));
            let local = TransformId::new("local");
            isolated.add_with_id(local.clone(), config).await.unwrap();
            assert!(isolated.has_transform(&local));
            assert!(!store.has_transform(&local));
        })
        .unwrap()
    }

    #[test]
    fn test_memory_store_transform_not_found() {
        workers::block_on(async {
            let store = MemoryTransformStore::new();
            let id = TransformId::new("nonexistent");

            let result = store.transform(&id, Box::pin(futures::stream::empty()));
            assert!(matches!(result, Err(Error::TransformNotFound(_))));
        })
        .unwrap()
    }
}
