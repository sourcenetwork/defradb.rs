//! Adapter to bridge database view operations to HTTP's ViewOperations trait.

use std::borrow::Cow;
use std::sync::Arc;

use async_trait::async_trait;
use defra_core::RequestContext;

use defra_http::router::ViewOperations;
use identity::Did;
use schema::CollectionVersion;
use storage::corekv::Store;

/// Adapter that implements ViewOperations using the database. A request in a
/// transaction runs in it, through `registry`.
pub struct ViewAdapter<S>
where
    S: Store + 'static,
{
    database: Arc<db::DB<S>>,
    registry: Arc<db::DbTransactionRegistry<S>>,
    query_limits: query::QueryLimits,
    document_acp: Option<Arc<dyn acp::DocumentACP>>,
    /// The request this adapter serves, for one bound to it.
    context: Option<RequestContext>,
}

impl<S: Store + 'static> ViewAdapter<S> {
    /// Create a new adapter wrapping the given database and query limits.
    pub fn new(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        query_limits: query::QueryLimits,
    ) -> Self {
        Self {
            database,
            registry,
            query_limits,
            document_acp: None,
            context: None,
        }
    }

    /// Create a new adapter with document ACP registration support.
    pub fn new_with_acp(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        query_limits: query::QueryLimits,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self {
            document_acp: Some(document_acp),
            ..Self::new(database, registry, query_limits)
        }
    }

    fn context(&self) -> Cow<'_, RequestContext> {
        RequestContext::bound_or_ambient(self.context.as_ref())
    }

    /// Create an Arc-wrapped adapter.
    pub fn new_arc(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        query_limits: query::QueryLimits,
    ) -> Arc<dyn ViewOperations> {
        Arc::new(Self::new(database, registry, query_limits))
    }

    /// Create an Arc-wrapped adapter with document ACP registration support.
    pub fn new_arc_with_acp(
        database: Arc<db::DB<S>>,
        registry: Arc<db::DbTransactionRegistry<S>>,
        query_limits: query::QueryLimits,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Arc<dyn ViewOperations> {
        Arc::new(Self::new_with_acp(
            database,
            registry,
            query_limits,
            document_acp,
        ))
    }
}

#[async_trait]
impl<S: Store + 'static> ViewOperations for ViewAdapter<S> {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn ViewOperations>> {
        Some(Arc::new(Self {
            database: Arc::clone(&self.database),
            registry: Arc::clone(&self.registry),
            query_limits: self.query_limits,
            document_acp: self.document_acp.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn add_view(
        &self,
        gql_query: &str,
        sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>, String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::ViewAdd)
            .await
            .map_err(|e| format!("{}", e))?;

        let mut known_types: rapidhash::RapidHashSet<String> = self
            .database
            .list_collections()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let txn = self.context().txn().map(|txn| txn.to_string());
        if let Some(txn) = &txn {
            let in_txn = self
                .registry
                .get_collections_in_txn_as(&self.context(), txn)
                .await
                .map_err(|e| format!("{}", e))?;
            known_types.extend(in_txn.into_iter().map(|version| version.name));
        }

        let collections = query::parse_sdl_with_known_types(sdl, known_types)
            .map_err(|e| format!("failed to parse view SDL: {}", e))?;

        let wrapped_query = format!("query {{ {} }}", gql_query);
        let selects = query::parse_query_with_limits(&wrapped_query, None, self.query_limits)
            .map_err(|e| format!("failed to parse view query: {}", e))?;
        if selects.is_empty() {
            return Err("invalid view query: no selections found".to_string());
        }
        let select_json = query::select_to_go_json(&selects[0]);

        if let Some(t) = transform {
            let lens_store = self.database.lens_store();
            for cid in t.split(',') {
                let cid = cid.trim();
                if !cid.is_empty() {
                    let tid = lens::TransformId::new(cid);
                    if !lens_store.has_transform(&tid) {
                        return Err("lens CID not found".to_string());
                    }
                }
            }
        }

        let mut query_source = schema::QuerySource::new(select_json);
        if let Some(t) = transform {
            query_source = query_source.with_transform(t);
        }

        let view_collections: Vec<_> = collections
            .into_iter()
            .map(|mut col_version| {
                if col_version.downsample_interval.is_some() {
                    col_version.downsample_source = Some(query_source.clone());
                } else {
                    col_version.query = Some(query_source.clone());
                }
                col_version
            })
            .collect();

        for collection in &view_collections {
            if collection.downsample_interval.is_some() {
                self.database
                    .validate_downsample_collection(collection)
                    .map_err(|e| format!("invalid downsample definition: {}", e))?;
            }
        }

        // Fail closed: a present-but-malformed acting identity must error, not
        // silently degrade a permissioned collection to public (unregistered).
        let context = self.context();
        let creator = match context.identity() {
            Some(raw) => {
                Some(Did::new(raw).map_err(|e| format!("malformed acting identity: {}", e))?)
            }
            None => None,
        };
        if let Some(txn) = &txn {
            let materialized = view_collections
                .iter()
                .filter(|col| db::is_refreshable_view(col))
                .map(|col| col.name.clone())
                .collect();
            let downsample = view_collections
                .iter()
                .filter(|col| col.downsample_interval.is_some())
                .map(|col| col.name.clone())
                .collect();
            return self
                .registry
                .add_view_in_txn(
                    &context,
                    txn,
                    view_collections,
                    self.document_acp.clone(),
                    creator,
                    materialized,
                    downsample,
                )
                .await
                .map_err(|e| format!("failed to create view collection: {}", e));
        }
        let created_versions = self
            .database
            .create_view_collections_as(
                &context,
                view_collections,
                self.document_acp.clone(),
                creator,
            )
            .await
            .map_err(|e| format!("failed to create view collection: {}", e))?;

        let materialized_names: Vec<String> = created_versions
            .iter()
            .filter(|col| db::is_refreshable_view(col))
            .map(|col| col.name.clone())
            .collect();
        let downsample_names: Vec<String> = created_versions
            .iter()
            .filter(|col| col.downsample_interval.is_some())
            .map(|col| col.name.clone())
            .collect();

        if !materialized_names.is_empty() {
            self.database
                .refresh_views(db::CollectionSelector::with_names(materialized_names))
                .await
                .map_err(|e| format!("failed to refresh materialized views: {}", e))?;
        }

        if !downsample_names.is_empty() {
            self.database
                .bootstrap_downsamples_as(&context, Some(&downsample_names))
                .await
                .map_err(|e| format!("failed to bootstrap downsample collections: {}", e))?;
        }

        Ok(created_versions)
    }

    async fn refresh_views(&self, options: db::CollectionSelector) -> Result<(), String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::ViewRefresh)
            .await
            .map_err(|e| format!("{}", e))?;

        let context = self.context();
        if let Some(txn) = context.txn() {
            return self
                .registry
                .refresh_views_in_txn(&context, &txn.to_string(), options)
                .await
                .map_err(|e| format!("failed to refresh views: {}", e));
        }
        self.database
            .refresh_views(options)
            .await
            .map_err(|e| format!("failed to refresh views: {}", e))
    }

    async fn gc_downsample_histories(&self, names: Option<Vec<String>>) -> Result<(), String> {
        self.database
            .check_node_access_as(&self.context(), acp::nac::NodePermission::ViewGc)
            .await
            .map_err(|e| format!("{}", e))?;

        let options = names.map(db::downsample::GcDownsampleHistoriesOptions::with_names);
        self.database
            .gc_downsample_histories_as(&self.context(), options)
            .await
            .map_err(|e| format!("failed to GC downsample histories: {}", e))
    }
}

#[cfg(test)]
#[path = "../tests/unit/view_adapter.rs"]
mod tests;
