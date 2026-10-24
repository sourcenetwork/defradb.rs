//! Adapter to bridge database schema operations to HTTP and PG wire protocol.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::{AcpOperations, CollectionOperations};
use identity::Did;
use schema::CollectionVersion;
use storage::corekv::Store;

/// Adapter that implements schema operations using the database.
pub struct SchemaAdapter<S: Store> {
    database: Arc<db::DB<S>>,
    /// Optional ACP handle for schema-time DRI validation (#746).
    ///
    /// When `Some`, `add_collection` validates `@policy(id:, resource:)`
    /// directives against the ACP store before creating collections:
    /// the policy must exist, the resource must exist on it, and the
    /// resource must declare the DPI-required `read`/`update`/`delete`
    /// permissions. When `None` (legacy / tests without ACP wired in),
    /// the validator is skipped.
    acp: Option<Arc<dyn AcpOperations>>,
    /// Optional document ACP handle for branchable collection object registration.
    document_acp: Option<Arc<dyn acp::DocumentACP>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> SchemaAdapter<S> {
    /// Create a new adapter wrapping the given database.
    pub fn new(database: Arc<db::DB<S>>) -> Self {
        Self {
            database,
            acp: None,
            document_acp: None,
            context: None,
        }
    }

    /// Create a new adapter with ACP wired in for schema-time DRI validation.
    pub fn new_with_acp(
        database: Arc<db::DB<S>>,
        acp: Arc<dyn AcpOperations>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self {
            database,
            acp: Some(acp),
            document_acp: Some(document_acp),
            context: None,
        }
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }

    /// Create an Arc-wrapped adapter for HTTP schema operations.
    pub fn new_arc(database: Arc<db::DB<S>>) -> Arc<dyn CollectionOperations> {
        Arc::new(Self::new(database))
    }

    /// Create an Arc-wrapped adapter for HTTP schema operations with ACP
    /// validation wired in.
    pub fn new_arc_with_acp(
        database: Arc<db::DB<S>>,
        acp: Arc<dyn AcpOperations>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn CollectionOperations> {
        Arc::new(Self::new_with_acp(database, acp, document_acp))
    }

    /// Create an Arc-wrapped adapter for PG wire protocol schema operations.
    ///
    /// Use `new_pg_arc_with_acp` instead when the caller has an
    /// `AcpOperations` handle — that variant enables schema-time DRI
    /// validation (#746). This bare constructor exists for tests and
    /// for embedded PG callers that don't have ACP configured.
    #[cfg(feature = "postgres")]
    pub fn new_pg_arc(database: Arc<db::DB<S>>) -> Arc<dyn pg_compat::SchemaManager> {
        Arc::new(Self::new(database))
    }

    /// Create an Arc-wrapped PG schema manager with ACP wired in for
    /// schema-time DRI validation (#746). Use this when the PG server
    /// has access to an `AcpOperations` handle.
    #[cfg(feature = "postgres")]
    pub fn new_pg_arc_with_acp(
        database: Arc<db::DB<S>>,
        acp: Arc<dyn AcpOperations>,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn pg_compat::SchemaManager> {
        Arc::new(Self::new_with_acp(database, acp, document_acp))
    }

    async fn add_collection_inner(&self, sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        // Fail closed: a present-but-malformed acting identity must error, not
        // silently degrade a permissioned collection to public (unregistered).
        let context = self.context();
        let creator = match context.identity() {
            Some(raw) => {
                Some(Did::new(raw).map_err(|e| format!("malformed acting identity: {}", e))?)
            }
            None => None,
        };
        let known_types: rapidhash::RapidHashSet<String> = self
            .database
            .list_collections()
            .unwrap_or_default()
            .into_iter()
            .collect();

        let collections = query::parse_sdl_with_known_types(sdl, known_types)
            .map_err(|e| format!("failed to parse SDL: {}", e))?;

        schema::definition_validation::validate_new_collections(&collections)
            .map_err(|e| format!("failed to validate schema: {}", e))?;

        // #746: validate every @policy directive against the ACP store
        // before creating any collection. If any collection's DRI is
        // invalid (policy missing, resource missing, DPI perm missing),
        // reject the whole schema-add so we don't leave a half-created
        // state.
        if let Some(acp) = &self.acp {
            for collection in &collections {
                if let Some(policy) = &collection.policy {
                    acp.validate_resource_interface(&policy.id, &policy.resource_name)
                        .await
                        .map_err(|e| format!("failed to add collection: {}", e))?;
                }
            }
        }

        let created = if let Some(document_acp) = &self.document_acp {
            self.database
                .create_collections_atomic_with_acp_registration_as(
                    &context,
                    collections,
                    document_acp.clone(),
                    creator,
                )
                .await
        } else {
            self.database
                .create_collections_atomic_as(&context, collections)
                .await
        }
        .map_err(|e| format!("failed to create collection: {}", e))?;

        Ok(created)
    }
}

#[async_trait]
impl<S: Store + 'static> CollectionOperations for SchemaAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn CollectionOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            acp: self.acp.clone(),
            document_acp: self.document_acp.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn add_collection(&self, sdl: &str) -> Result<Vec<CollectionVersion>, String> {
        self.add_collection_inner(sdl).await
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl<S: Store + 'static> pg_compat::SchemaManager for SchemaAdapter<S> {
    async fn add_collection(&self, sdl: &str) -> Result<(), String> {
        self.add_collection_inner(sdl).await?;
        Ok(())
    }
}
