//! Database migrations and lens operations for schema versioning.
//!
//! Physical store-format migrations run first during database open so that
//! older data is readable by the current storage layer. Lens migrations then
//! provide document transforms between schema versions.

pub(crate) mod committed_lenses;
mod counters;
pub mod document_parts;
pub mod format;
mod head_sets;
pub mod helpers;
mod lenses;
mod reindex;
mod set;
mod shortid;

use std::sync::Arc;

use lens::{TransformId, TransformStore};
use storage::corekv::Store;

use crate::error::Result;
use crate::DB;

pub use reindex::ReindexReport;
pub(crate) use reindex::{owe_reindex, reindex_owed};

impl<S: Store> DB<S> {
    /// Run the ordered database-open migration pipeline.
    ///
    /// Physical migrations must precede migrations that read current-format
    /// documents or persisted lens configurations.
    pub(crate) async fn initialize_migrations(&self) -> Result<()> {
        self.maybe_migrate_v015_document_storage().await?;
        self.maybe_migrate_head_sets().await?;
        self.maybe_backfill_commit_priority_index().await?;
        self.reload_lens_configs().await
    }

    /// Get a reference to the lens transform store.
    ///
    /// The lens store manages schema migration transforms that can be applied
    /// when documents are fetched from older schema versions.
    pub fn lens_store(&self) -> &Arc<dyn TransformStore> {
        &self.lens_store
    }

    /// Check if a migration exists between two schema versions.
    pub fn has_migration(&self, transform_id: &TransformId) -> bool {
        self.lens_store.has_transform(transform_id)
    }
}
