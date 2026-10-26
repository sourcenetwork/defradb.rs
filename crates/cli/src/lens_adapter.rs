//! Adapter to bridge database lens operations to HTTP's LensOperations trait.

use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::LensOperations;
use lens::LensConfig;
use storage::corekv::Store;

/// Adapter that implements LensOperations using the database's persistent lens store.
pub struct LensAdapter<S: Store> {
    database: Arc<db::DB<S>>,
}

impl<S: Store + 'static> LensAdapter<S> {
    /// Create an Arc-wrapped adapter backed by the database's lens store.
    pub fn new_arc(database: Arc<db::DB<S>>) -> Arc<dyn LensOperations> {
        Arc::new(Self { database })
    }
}

/// A [`LensAdapter`] serving one request.
struct RequestLensAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    context: RequestContext,
}

#[async_trait]
impl<S: Store + 'static> LensOperations for LensAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn LensOperations>> {
        Some(Arc::new(RequestLensAdapter {
            database: Arc::clone(&self.database),
            context: context.clone(),
        }))
    }

    async fn set_migration(&self, config: &str) -> Result<String, String> {
        set_migration(&self.database, &RequestContext::ambient(), config).await
    }

    async fn reload(&self) -> Result<(), String> {
        reload(&self.database).await
    }

    async fn add(&self, config: &str) -> Result<String, String> {
        add(&self.database, &RequestContext::ambient(), config).await
    }

    async fn list(&self) -> Result<serde_json::Value, String> {
        list(&self.database, &RequestContext::ambient()).await
    }
}

#[async_trait]
impl<S: Store + 'static> LensOperations for RequestLensAdapter<S> {
    async fn set_migration(&self, config: &str) -> Result<String, String> {
        set_migration(&self.database, &self.context, config).await
    }

    async fn reload(&self) -> Result<(), String> {
        reload(&self.database).await
    }

    async fn add(&self, config: &str) -> Result<String, String> {
        add(&self.database, &self.context, config).await
    }

    async fn list(&self) -> Result<serde_json::Value, String> {
        list(&self.database, &self.context).await
    }
}

async fn set_migration<S: Store + 'static>(
    database: &db::DB<S>,
    context: &RequestContext,
    config: &str,
) -> Result<String, String> {
    let lens_config: LensConfig =
        serde_json::from_str(config).map_err(|e| format!("failed to parse lens config: {}", e))?;

    let transform_id = database
        .set_migration_as(context, lens_config)
        .await
        .map_err(|e| format!("failed to set migration: {}", e))?;

    Ok(transform_id.to_string())
}

async fn reload<S: Store + 'static>(database: &db::DB<S>) -> Result<(), String> {
    database
        .reload_lens_configs()
        .await
        .map_err(|e| format!("failed to reload lens configs: {}", e))
}

async fn add<S: Store + 'static>(
    database: &db::DB<S>,
    context: &RequestContext,
    config: &str,
) -> Result<String, String> {
    let lens_config: LensConfig =
        serde_json::from_str(config).map_err(|e| format!("failed to parse lens config: {}", e))?;

    // If version IDs are present, delegate to set_migration (matches FFI behavior)
    if !lens_config.source_collection_version_id.is_empty()
        && !lens_config.destination_collection_version_id.is_empty()
    {
        let transform_id = database
            .set_migration_as(context, lens_config)
            .await
            .map_err(|e| format!("failed to set migration: {}", e))?;
        return Ok(transform_id.to_string());
    }

    database
        .check_node_access_as(context, acp::nac::NodePermission::LensCreate)
        .await
        .map_err(|e| format!("{}", e))?;

    // Build IPLD blocks, store in blockstore, register with real CID
    let transform_id = database
        .add_lens(lens_config)
        .await
        .map_err(|e| format!("failed to add lens: {}", e))?;

    Ok(transform_id.to_string())
}

async fn list<S: Store + 'static>(
    database: &db::DB<S>,
    context: &RequestContext,
) -> Result<serde_json::Value, String> {
    database
        .check_node_access_as(context, acp::nac::NodePermission::LensList)
        .await
        .map_err(|e| format!("{}", e))?;

    let modules = database
        .lens_store()
        .list()
        .await
        .map_err(|e| format!("failed to list lenses: {}", e))?;

    serde_json::to_value(&modules).map_err(|e| format!("failed to serialize lens modules: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<db::DB<storage::RegolithStore>> {
        let store = storage::RegolithStore::in_memory().unwrap();
        Arc::new(db::DB::new(store).unwrap())
    }

    #[tokio::test]
    async fn test_lens_adapter_invalid_config() {
        let adapter = LensAdapter {
            database: test_db(),
        };
        let result = adapter.set_migration("not valid json").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse lens config"));
    }

    #[tokio::test]
    async fn test_lens_adapter_reload() {
        let adapter = LensAdapter {
            database: test_db(),
        };
        let result = adapter.reload().await;
        assert!(result.is_ok());
    }
}
