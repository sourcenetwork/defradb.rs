//! Adding, refreshing and collecting views: the one implementation HTTP, the
//! CLI, FFI and the embedded node call.

mod error;
mod manage;

pub use error::ViewError;

use defra_core::RequestContext;
use identity::Did;
use schema::{CollectionVersion, QuerySource};
use storage::corekv::Store;

use crate::DbTransactionRegistry;

type Result<T> = std::result::Result<T, ViewError>;

impl<S> DbTransactionRegistry<S>
where
    S: Store + 'static,
{
    /// Go's `AddView`: the views `target_sdl` defines over the one source
    /// `source_query` selects, read through lens `transform` when it names
    /// one. In the transaction `context` carries if it carries one. Once the
    /// views commit, the materialized ones are built and the downsamples
    /// bootstrapped, before this returns.
    pub async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
        transform: Option<&str>,
        limits: query::QueryLimits,
    ) -> Result<Vec<CollectionVersion>> {
        let db = self.db();
        db.check_node_access_as(context, acp::nac::NodePermission::ViewAdd)
            .await?;
        let txn = context.txn().map(|txn| txn.to_string());

        let mut known_types: rapidhash::RapidHashSet<String> = db
            .list_collections()
            .unwrap_or_default()
            .into_iter()
            .collect();
        if let Some(txn) = &txn {
            let in_txn = self.get_collections_in_txn_as(context, txn).await?;
            known_types.extend(in_txn.into_iter().map(|version| version.name));
        }
        let mut collections = query::parse_sdl_with_known_types(target_sdl, known_types)
            .map_err(ViewError::SchemaParse)?;

        let source = view_source(source_query, transform, limits)?;
        for collection in &mut collections {
            if collection.downsample_interval.is_some() {
                collection.downsample_source = Some(source.clone());
            } else {
                collection.query = Some(source.clone());
            }
        }
        schema::definition_validation::validate_no_chunk_in_view(&collections)
            .map_err(|e| ViewError::Create(crate::Error::Other(e)))?;
        for collection in collections
            .iter()
            .filter(|c| c.downsample_interval.is_some())
        {
            db.validate_downsample_collection(collection)
                .map_err(ViewError::InvalidDownsample)?;
        }
        // As for a collection, a node with no policy lookup checks no `@policy`.
        let policies = collections
            .iter()
            .filter_map(|c| c.policy.as_ref())
            .filter(|_| db.dac_policies().is_some());
        for policy in policies {
            db.validate_resource_interface(&policy.id, &policy.resource_name)
                .await
                .map_err(ViewError::policy)?;
        }
        let creator = context
            .identity()
            .map(Did::new)
            .transpose()
            .map_err(ViewError::InvalidIdentity)?;

        let materialized: Vec<String> = collections
            .iter()
            .filter(|c| crate::is_refreshable_view(c))
            .map(|c| c.name.clone())
            .collect();
        let downsample: Vec<String> = collections
            .iter()
            .filter(|c| c.downsample_interval.is_some())
            .map(|c| c.name.clone())
            .collect();
        let document_acp = db.document_acp().cloned();

        if let Some(txn) = &txn {
            return self
                .add_view_in_txn(
                    context,
                    txn,
                    collections,
                    document_acp,
                    creator,
                    materialized,
                    downsample,
                )
                .await
                .map_err(ViewError::create);
        }
        let created = db
            .create_view_collections_as(context, collections, document_acp, creator)
            .await
            .map_err(ViewError::create)?;
        if !materialized.is_empty() {
            db.refresh_views(crate::CollectionSelector::with_names(materialized))
                .await
                .map_err(ViewError::Refresh)?;
        }
        if !downsample.is_empty() {
            db.bootstrap_downsamples_as(context, Some(&downsample))
                .await
                .map_err(ViewError::DownsampleBootstrap)?;
        }
        Ok(created)
    }
}

/// The query source of a view: the one type `source_query` selects, in
/// Go's JSON, with lens `transform` when it names one.
fn view_source(
    source_query: &str,
    transform: Option<&str>,
    limits: query::QueryLimits,
) -> Result<QuerySource> {
    let wrapped = format!("query {{ {} }}", source_query.trim());
    let mut selects = query::parse_query_with_limits(&wrapped, None, limits)
        .map_err(ViewError::QueryParse)?
        .into_iter();
    let select = selects.next().ok_or(ViewError::QueryEmpty)?;
    if selects.next().is_some() {
        return Err(ViewError::QueryMultipleSources);
    }
    let source = QuerySource::new(query::select_to_go_json(&select));
    match transform {
        Some("") => Err(ViewError::TransformCidEmpty),
        // The create transaction checks the lens config is committed.
        Some(cid) => Ok(source.with_transform(cid)),
        None => Ok(source),
    }
}
