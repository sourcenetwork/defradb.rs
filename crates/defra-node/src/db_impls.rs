//! Trait implementations bridging DB to the node's type-erased traits.

use std::sync::Arc;

use defra_core::RequestContext;
use identity::Did;
use schema::CollectionVersion;

use crate::{BlockOps, Error, Result, SchemaOps};

pub(crate) struct DbBlockOps<S: storage::corekv::Store + 'static> {
    database: Arc<db::DB<S>>,
    document_acp: Arc<dyn acp::DocumentACP>,
    transaction_registry: Arc<db::DbTransactionRegistry<S>>,
    caller_identity: acp::Identity,
}

impl<S: storage::corekv::Store + 'static> DbBlockOps<S> {
    pub(crate) fn new(
        database: Arc<db::DB<S>>,
        document_acp: Arc<dyn acp::DocumentACP>,
        transaction_registry: Arc<db::DbTransactionRegistry<S>>,
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
}

#[async_trait::async_trait]
impl<S: storage::corekv::Store + 'static> BlockOps for DbBlockOps<S> {
    async fn signed_block_bytes(
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

    async fn verified_signer_did(&self, cid: &str) -> Result<String> {
        db::block::verify::verified_block_signer_did(
            &self.database,
            self.document_acp.as_ref(),
            cid,
            &self.caller_identity,
        )
        .await
        .map_err(Error::BlockAccess)
    }

    async fn verified_signer_did_in_txn(
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

pub(crate) struct DbSchemaOps<S: storage::corekv::Store + 'static> {
    database: Arc<db::DB<S>>,
    query_limits: query::QueryLimits,
    document_acp: Arc<dyn acp::DocumentACP>,
}

impl<S: storage::corekv::Store + 'static> DbSchemaOps<S> {
    pub(crate) fn new(
        database: Arc<db::DB<S>>,
        query_limits: query::QueryLimits,
        document_acp: Arc<dyn acp::DocumentACP>,
    ) -> Self {
        Self {
            database,
            query_limits,
            document_acp,
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

#[async_trait::async_trait]
impl<S: storage::corekv::Store + 'static> SchemaOps for DbSchemaOps<S> {
    async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> Result<Vec<CollectionVersion>> {
        Ok(self.database.add_collection(context, sdl).await?)
    }

    async fn ensure_ordinary_index(
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

    async fn add_view(
        &self,
        context: &RequestContext,
        source_query: &str,
        target_sdl: &str,
    ) -> Result<()> {
        self.database
            .check_node_access_as(context, acp::nac::NodePermission::ViewAdd)
            .await
            .map_err(Error::from)?;

        let known_types: rapidhash::RapidHashSet<String> = db::DB::list_collections(&self.database)
            .unwrap_or_default()
            .into_iter()
            .collect();

        let mut collections = query::parse_sdl_with_known_types(target_sdl, known_types)
            .map_err(Error::ViewSchemaParse)?;

        let wrapped_query = format!("query {{ {} }}", source_query.trim());
        let selects = query::parse_query_with_limits(&wrapped_query, None, self.query_limits)
            .map_err(Error::ViewQueryParse)?;
        let select = selects.into_iter().next().ok_or(Error::ViewQueryEmpty)?;
        let query_source = schema::QuerySource::new(query::select_to_go_json(&select));

        let mut materialized_names = Vec::new();
        let mut downsample_names = Vec::new();

        for collection in &mut collections {
            if collection.downsample_interval.is_some() {
                collection.downsample_source = Some(query_source.clone());
                self.database
                    .validate_downsample_collection(collection)
                    .map_err(Error::InvalidDownsample)?;
            } else {
                collection.query = Some(query_source.clone());
            }

            if collection.query.is_some()
                && collection.is_materialized
                && !collection.is_embedded_only
            {
                materialized_names.push(collection.name.clone());
            }

            if collection.downsample_interval.is_some() {
                downsample_names.push(collection.name.clone());
            }
        }

        self.validate_policies(&collections).await?;

        let creator = Self::creator(context)?;
        self.database
            .create_collections_atomic_with_acp_registration_as(
                context,
                collections,
                self.document_acp.clone(),
                creator,
            )
            .await
            .map_err(Error::ViewCollectionCreate)?;

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

        Ok(())
    }

    async fn materialize_collection(
        &self,
        context: &RequestContext,
        collection_name: &str,
    ) -> Result<usize> {
        db::DB::materialize_collection_as(&self.database, context, collection_name)
            .await
            .map_err(Error::from)
    }

    fn list_collections(&self) -> Result<Vec<String>> {
        db::DB::list_collections(&self.database).map_err(Error::from)
    }

    fn get_collection(&self, name: &str) -> Result<Option<CollectionVersion>> {
        Ok(db::DB::get_collection(&self.database, name)
            .map_err(Error::from)?
            .map(|collection| collection.schema().clone()))
    }

    async fn get_collection_by_version_id(
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

    async fn get_all_collection_versions(&self) -> Result<Vec<CollectionVersion>> {
        db::DB::get_all_collection_versions(&self.database)
            .await
            .map_err(Error::from)
    }
}
