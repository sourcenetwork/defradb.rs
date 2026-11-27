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
    /// Runs the operations a request makes inside its transaction.
    transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
}

impl DbSchemaOps {
    pub(crate) fn new(
        database: Arc<db::DB<storage::RegolithStore>>,
        query_limits: query::QueryLimits,
        transaction_registry: Arc<db::DbTransactionRegistry<storage::RegolithStore>>,
    ) -> Self {
        Self {
            database,
            query_limits,
            transaction_registry,
        }
    }

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
        Ok(self
            .transaction_registry
            .add_view(
                context,
                source_query,
                target_sdl,
                transform,
                self.query_limits,
            )
            .await?)
    }

    pub(crate) async fn refresh_views(
        &self,
        context: &RequestContext,
        selector: db::CollectionSelector,
    ) -> Result<()> {
        Ok(self
            .transaction_registry
            .refresh_views_as(context, selector)
            .await?)
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
