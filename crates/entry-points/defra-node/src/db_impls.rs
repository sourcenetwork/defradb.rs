//! The block and schema operations bridging DB to the node.

use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use schema::CollectionVersion;

use crate::{Error, Result};

/// Every field is a shared handle or an immutable value, so a clone shares all
/// state; a field with state of its own breaks that and belongs behind an `Arc`.
#[derive(Clone)]
pub(crate) struct DbBlockOps {
    database: Arc<db::DB<storage::RegolithStore>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    caller_identity: acp::Identity,
}

impl DbBlockOps {
    pub(crate) fn new(
        database: Arc<db::DB<storage::RegolithStore>>,
        document_acp: Arc<dyn acp::DocumentACP>,
        transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
        node_identity: Option<Did>,
    ) -> Self {
        let caller_identity = node_identity.into();
        Self {
            database,
            document_acp,
            transaction_registry,
            caller_identity,
        }
    }

    pub(crate) async fn signed_block_bytes(
        &self,
        cid: &str,
        caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let caller_did = caller_did
            .map(|did| Did::try_from(did.to_string()))
            .transpose()?;
        let caller_identity: acp::Identity = caller_did.into();
        db::block::verify::authorized_signed_block_bytes(
            &self.database,
            self.document_acp.as_ref(),
            cid,
            &caller_identity,
        )
        .await
        .map_err(Error::BlockAccess)
    }

    pub(crate) async fn verified_signer_did(&self, cid: &str) -> Result<String> {
        db::block::verify::verified_block_signer_did(
            &self.database,
            self.document_acp.as_ref(),
            cid,
            &self.caller_identity,
        )
        .await
        .map_err(Error::BlockAccess)
    }

    pub(crate) async fn verified_signer_did_in_txn(
        &self,
        context: &RequestContext,
        cid: &str,
        transaction: &query::TransactionHandle,
    ) -> Result<String> {
        self.transaction_registry
            .verified_block_signer_did_in_txn_as(
                context,
                transaction.as_str(),
                self.document_acp.clone(),
                cid,
                &self.caller_identity,
            )
            .await
            .map_err(Error::from)
    }
}

/// Every field is a shared handle or an immutable value, so a clone shares all
/// state; a field with state of its own breaks that and belongs behind an `Arc`.
#[derive(Clone)]
pub(crate) struct DbSchemaOps {
    database: Arc<db::DB<storage::RegolithStore>>,
    query_limits: query::QueryLimits,
    document_acp: Arc<dyn acp::DocumentACP>,
    /// Runs the operations a request makes inside its transaction.
    transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
}

impl DbSchemaOps {
    pub(crate) fn new(
        database: Arc<db::DB<storage::RegolithStore>>,
        query_limits: query::QueryLimits,
        document_acp: Arc<dyn acp::DocumentACP>,
        transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    ) -> Self {
        Self {
            database,
            query_limits,
            document_acp,
            transaction_registry,
        }
    }

    /// Reject any `@policy` directive that does not resolve to a usable resource
    /// in the policy store, so a permissioned collection is never created public.
    /// Collections without a policy never touch the store.
    async fn validate_policies(&self, collections: &[CollectionVersion]) -> Result<()> {
        for collection in collections {
            let Some(policy) = &collection.policy else {
                continue;
            };
            self.database
                .validate_resource_interface(&policy.id, &policy.resource_name)
                .await
                .map_err(|error| match error {
                    db::DacPolicyError::ResourceInterface(message) => {
                        Error::PolicyValidation(message)
                    }
                    other => Error::DacPolicy(other),
                })?;
        }
        Ok(())
    }

    /// Resolve the identity acting in `context` into a creator DID. Returns
    /// `Ok(None)` when no identity is present (collection stays public), but
    /// fails closed when an identity string is present yet malformed, so a
    /// permissioned collection is never silently created unregistered.
    fn creator(context: &RequestContext) -> Result<Option<Did>> {
        match context.identity() {
            Some(raw) => Ok(Some(Did::new(raw).map_err(Error::InvalidActingIdentity)?)),
            None => Ok(None),
        }
    }
}

impl DbSchemaOps {
    pub(crate) async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        Ok(self.database.add_collection(context, sdl).await?)
    }

    pub(crate) async fn ensure_ordinary_index(
        &self,
        context: &RequestContext,
        collection: &str,
        name: &str,
        fields: Vec<schema::IndexedFieldDescription>,
    ) -> Result<schema::IndexDescription> {
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::IndexCreate)
            .await
            .map_err(Error::from)?;
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::IndexList)
            .await
            .map_err(Error::from)?;
        self.database
            .ensure_ordinary_index(collection, name, fields)
            .await
            .map_err(Error::from)
    }

    pub(crate) async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>> {
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::ViewAdd)
            .await
            .map_err(Error::from)?;

        let mut known_collections = self.database.known_collections();
        // A type the transaction created is known to the view's SDL too.
        if let Some(txn) = context.txn() {
            let in_txn = self
                .transaction_registry
                .get_collections_in_txn_as(context, &txn.to_string())
                .await
                .map_err(Error::from)?;
            known_collections.extend(
                in_txn
                    .into_iter()
                    .map(|version| (version.name, version.collection_id)),
            );
        }

        let mut collections =
            query::parse_sdl_with_known_collections(target_sdl, known_collections)
                .map_err(Error::ViewSchemaParse)?;

        let wrapped_query = format!("query {{ {} }}", source_query.trim());
        let selects = query::parse_query_with_limits(&wrapped_query, None, self.query_limits)
            .map_err(Error::ViewQueryParse)?;
        let mut selects = selects.into_iter();
        let select = selects.next().ok_or(Error::ViewQueryEmpty)?;
        if selects.next().is_some() {
            return Err(Error::ViewQueryMultipleSources);
        }
        let mut query_source = schema::QuerySource::new(query::select_to_go_json(&select));
        if let Some(cid) = transform {
            if cid.is_empty() {
                return Err(Error::ViewTransformCidEmpty);
            }
            // The create transaction checks the lens config is committed.
            query_source = query_source.with_transform(cid);
        }

        let mut materialized_names = Vec::new();
        let mut downsample_names = Vec::new();

        for collection in &mut collections {
            if collection.downsample_interval.is_some() {
                collection.downsample_source = Some(query_source.clone());
            } else {
                collection.query = Some(query_source.clone());
            }
        }
        schema::definition_validation::validate_no_chunk_in_view(&collections)
            .map_err(|e| Error::ViewCollectionCreate(db::Error::Other(e)))?;

        for collection in &collections {
            if collection.downsample_interval.is_some() {
                self.database
                    .validate_downsample_collection(collection)
                    .map_err(Error::InvalidDownsample)?;
            }

            if db::is_refreshable_view(collection) {
                materialized_names.push(collection.name.clone());
            }

            if collection.downsample_interval.is_some() {
                downsample_names.push(collection.name.clone());
            }
        }

        self.validate_policies(&collections).await?;

        let creator = Self::creator(context)?;
        if let Some(txn) = context.txn() {
            return self
                .transaction_registry
                .add_view_in_txn(
                    context,
                    &txn.to_string(),
                    collections,
                    Some(self.document_acp.clone()),
                    creator,
                    materialized_names,
                    downsample_names,
                )
                .await
                .map_err(|error| match error {
                    db::Error::ViewTransformMissing(cid) => Error::ViewTransformMissing(cid),
                    error => Error::ViewCollectionCreate(error),
                });
        }
        let created = self
            .database
            .create_view_collections_as(
                context,
                collections,
                Some(self.document_acp.clone()),
                creator,
            )
            .await
            .map_err(|error| match error {
                db::Error::ViewTransformMissing(cid) => Error::ViewTransformMissing(cid),
                error => Error::ViewCollectionCreate(error),
            })?;

        if !materialized_names.is_empty() {
            self.database
                .refresh_views(db::CollectionSelector::with_names(materialized_names))
                .await
                .map_err(Error::MaterializedViewRefresh)?;
        }

        if !downsample_names.is_empty() {
            self.database
                .bootstrap_downsamples_as(context, Some(&downsample_names))
                .await
                .map_err(Error::DownsampleBootstrap)?;
        }

        Ok(created)
    }

    pub(crate) async fn refresh_views(
        &self,
        context: &RequestContext,
        selector: db::CollectionSelector,
    ) -> Result<()> {
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::ViewRefresh)
            .await
            .map_err(Error::from)?;
        if let Some(txn) = context.txn() {
            return self
                .transaction_registry
                .refresh_views_in_txn(context, &txn.to_string(), selector)
                .await
                .map_err(Error::MaterializedViewRefresh);
        }
        self.database
            .refresh_views(selector)
            .await
            .map_err(Error::MaterializedViewRefresh)
    }

    pub(crate) async fn materialize_collection(
        &self,
        context: &RequestContext,
        collection_name: &str,
    ) -> Result<usize> {
        db::DB::materialize_collection_as(&self.database, context, collection_name)
            .await
            .map_err(Error::from)
    }

    pub(crate) fn list_collections(&self) -> Result<Vec<String>> {
        db::DB::list_collections(&self.database).map_err(Error::from)
    }

    pub(crate) fn get_collection(&self, name: &str) -> Result<Option<CollectionVersion>> {
        Ok(db::DB::get_collection(&self.database, name)
            .map_err(Error::from)?
            .map(|collection| collection.schema().clone()))
    }

    pub(crate) async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<CollectionVersion>> {
        Ok(
            db::DB::get_collection_by_version_id_full(&self.database, version_id)
                .await
                .map_err(Error::from)?
                .map(|collection| collection.schema().clone()),
        )
    }

    pub(crate) async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        db::DB::get_all_collection_versions(&self.database)
            .await
            .map_err(Error::from)
    }
}
