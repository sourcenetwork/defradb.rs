//! Lens migration pipeline.
//!
//! Matches Go's internal/lens/lens.go Lens type and behavior. Documents are
//! migrated on the caller's task, one at a time, as `next` asks for them.

use std::collections::VecDeque;
use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashMap, RapidHashSet};

use tracing::{debug, warn};

use crate::store::TransformStore;
use crate::{Error, LensDoc, Result, TargetedHistoryLink, TransformId};

/// Input to the lens pipeline: a document with its schema version.
#[derive(Debug, Clone)]
pub struct LensInput {
    /// The schema version ID of the document.
    pub schema_version_id: String,
    /// The document to transform.
    pub doc: LensDoc,
}

impl LensInput {
    /// Create a new lens input.
    pub fn new(schema_version_id: impl Into<String>, doc: LensDoc) -> Self {
        Self {
            schema_version_id: schema_version_id.into(),
            doc,
        }
    }
}

/// Lens migrates documents to a target schema version.
///
/// Documents may be of various schema versions and may need migration across multiple
/// versions. The pipeline is constructed lazily as new source versions are discovered.
///
/// Matches Go's Lens interface.
pub struct Lens {
    store: Arc<dyn TransformStore>,
    target_version_id: String,
    collection_history: RapidHashMap<String, TargetedHistoryLink>,
    pending: VecDeque<LensInput>,
}

impl Lens {
    /// Create a new lens pipeline.
    ///
    /// # Arguments
    /// * `store` - The transform store for executing WASM transforms
    /// * `target_version_id` - The target schema version to migrate documents to
    /// * `collection_history` - The targeted history for version traversal
    pub fn new(
        store: Arc<dyn TransformStore>,
        target_version_id: impl Into<String>,
        collection_history: RapidHashMap<String, TargetedHistoryLink>,
    ) -> Self {
        Self {
            store,
            target_version_id: target_version_id.into(),
            collection_history,
            pending: VecDeque::new(),
        }
    }

    /// Put a document into the pipeline for transformation.
    pub async fn put(&mut self, schema_version_id: &str, doc: LensDoc) -> Result<()> {
        self.pending
            .push_back(LensInput::new(schema_version_id, doc));
        Ok(())
    }

    /// Get the next transformed document from the pipeline.
    pub async fn next(&mut self) -> Option<Result<LensDoc>> {
        let input = self.pending.pop_front()?;
        Some(
            transform_to_target(
                &self.store,
                &self.target_version_id,
                &self.collection_history,
                input,
            )
            .await,
        )
    }

    /// Check if a migration is needed for the given schema version.
    pub fn needs_migration(&self, schema_version_id: &str) -> bool {
        schema_version_id != self.target_version_id
            && self.collection_history.contains_key(schema_version_id)
    }

    /// Get the target schema version ID.
    pub fn target_version_id(&self) -> &str {
        &self.target_version_id
    }
}

/// Transform a document to the target schema version.
///
/// Runs on the caller task without spawning work on another executor.
/// Transaction attempts use this entry point so a local transform cannot
/// depend on the runtime thread currently driving their storage I/O queue.
pub async fn transform_to_target(
    store: &Arc<dyn TransformStore>,
    target_version_id: &str,
    collection_history: &RapidHashMap<String, TargetedHistoryLink>,
    input: LensInput,
) -> Result<LensDoc> {
    debug!(
        from_version = %input.schema_version_id,
        to_version = %target_version_id,
        history_size = collection_history.len(),
        "Starting transform_to_target"
    );

    if input.schema_version_id == target_version_id {
        debug!("Document already at target version, no transform needed");
        return Ok(input.doc);
    }

    let _history_link = collection_history
        .get(&input.schema_version_id)
        .ok_or_else(|| {
            warn!(
                version = %input.schema_version_id,
                "Schema version not found in history"
            );
            Error::SchemaVersionNotFound(input.schema_version_id.clone())
        })?;

    let mut current_doc = input.doc;
    let mut current_version = input.schema_version_id.clone();

    let mut visited = RapidHashSet::new();
    visited.insert(current_version.clone());

    let mut iteration = 0;
    loop {
        iteration += 1;
        debug!(
            iteration = iteration,
            current_version = %current_version,
            target_version = %target_version_id,
            "Migration loop iteration"
        );

        if current_version == target_version_id {
            debug!(
                iterations = iteration,
                "Migration complete, reached target version"
            );
            return Ok(current_doc);
        }

        let current_link = collection_history
            .get(&current_version)
            .ok_or_else(|| Error::SchemaVersionNotFound(current_version.clone()))?;

        debug!(
            current_version = %current_version,
            transform = ?current_link.transform,
            previous = ?current_link.previous,
            next = ?current_link.next,
            "Current link in history"
        );

        // Try forward (next) first, then backward (previous).
        // If the forward direction was already visited, fall through to backward.
        let can_go_next = current_link
            .next
            .as_ref()
            .is_some_and(|v| !visited.contains(v));
        let can_go_prev = current_link
            .previous
            .as_ref()
            .is_some_and(|v| !visited.contains(v));

        debug!(
            can_go_next = can_go_next,
            can_go_prev = can_go_prev,
            visited = ?visited,
            "Navigation options"
        );

        if can_go_next {
            let next_version = current_link.next.as_ref().unwrap();

            let next_link = collection_history
                .get(next_version)
                .ok_or_else(|| Error::SchemaVersionNotFound(next_version.clone()))?;

            debug!(
                direction = "forward",
                from = %current_version,
                to = %next_version,
                transform = ?next_link.transform,
                "Moving forward in version history"
            );

            if let Some(ref transform_id) = next_link.transform {
                debug!(
                    transform_id = %transform_id,
                    "Applying forward transform"
                );
                current_doc =
                    apply_transform(store, &TransformId::new(transform_id), current_doc, false)
                        .await?;
                debug!("Forward transform applied successfully");
            } else {
                debug!("No transform defined for this version transition");
            }

            current_version = next_version.clone();
            visited.insert(current_version.clone());
        } else if can_go_prev {
            let prev_version = current_link.previous.as_ref().unwrap();

            debug!(
                direction = "backward",
                from = %current_version,
                to = %prev_version,
                transform = ?current_link.transform,
                "Moving backward in version history"
            );

            if let Some(ref transform_id) = current_link.transform {
                debug!(
                    transform_id = %transform_id,
                    "Applying inverse transform"
                );
                current_doc =
                    apply_transform(store, &TransformId::new(transform_id), current_doc, true)
                        .await?;
                debug!("Inverse transform applied successfully");
            } else {
                debug!("No transform defined for this version transition");
            }

            current_version = prev_version.clone();
            visited.insert(current_version.clone());
        } else {
            warn!(
                from = %input.schema_version_id,
                to = %target_version_id,
                current = %current_version,
                "No migration path found"
            );
            return Err(Error::Pipeline(format!(
                "no migration path from {} to {}",
                input.schema_version_id, target_version_id
            )));
        }
    }
}

async fn apply_transform(
    store: &Arc<dyn TransformStore>,
    transform_id: &TransformId,
    doc: LensDoc,
    inverse: bool,
) -> Result<LensDoc> {
    debug!(transform_id = %transform_id, inverse, "Applying transform");
    store
        .transform_doc(transform_id, doc, inverse)
        .await
        .inspect_err(|error| debug!(transform_id = %transform_id, %error, "Transform failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryTransformStore;
    use crate::{LensConfig, LensModule};
    use rapidhash::HashMapExt;
    use serde_json::json;

    #[test]
    fn test_lens_passthrough_at_target() {
        workers::block_on(async {
            let store = Arc::new(MemoryTransformStore::new());
            let history = RapidHashMap::new();
            let mut lens = Lens::new(store, "v1", history);

            let mut doc = LensDoc::new();
            doc.insert("name".to_string(), json!("Alice"));

            lens.put("v1", doc.clone()).await.unwrap();

            let result = lens.next().await.unwrap().unwrap();
            assert_eq!(result.get("name").unwrap(), &json!("Alice"));
        })
        .unwrap()
    }

    #[test]
    fn test_lens_needs_migration() {
        workers::block_on(async {
            let store = Arc::new(MemoryTransformStore::new());
            let mut history = RapidHashMap::new();

            history.insert(
                "v1".to_string(),
                TargetedHistoryLink::new("v1", "col_1").with_next("v2"),
            );
            history.insert(
                "v2".to_string(),
                TargetedHistoryLink::new("v2", "col_1")
                    .with_transform(Some("transform_1".to_string()))
                    .with_previous("v1"),
            );

            let lens = Lens::new(store, "v2", history);

            assert!(lens.needs_migration("v1"));
            assert!(!lens.needs_migration("v2"));
            assert!(!lens.needs_migration("v_unknown"));
        })
        .unwrap()
    }

    #[test]
    fn test_lens_transform_with_registered_migration() {
        workers::block_on(async {
            let store = Arc::new(MemoryTransformStore::new());

            // Register a transform
            let config =
                LensConfig::new("v1", "v2", LensModule::from_path("/path/to/transform.wasm"));
            let transform_id = store.add(config).await.unwrap();

            let mut history = RapidHashMap::new();
            history.insert(
                "v1".to_string(),
                TargetedHistoryLink::new("v1", "col_1").with_next("v2"),
            );
            history.insert(
                "v2".to_string(),
                TargetedHistoryLink::new("v2", "col_1")
                    .with_transform(Some(transform_id.to_string()))
                    .with_previous("v1"),
            );

            let mut lens = Lens::new(store, "v2", history);

            let mut doc = LensDoc::new();
            doc.insert("name".to_string(), json!("Alice"));

            lens.put("v1", doc).await.unwrap();

            // MemoryTransformStore passes through unchanged
            let result = lens.next().await.unwrap().unwrap();
            assert_eq!(result.get("name").unwrap(), &json!("Alice"));
        })
        .unwrap()
    }
}
