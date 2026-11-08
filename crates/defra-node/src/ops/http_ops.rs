//! The node's operations behind the HTTP server's router traits: views,
//! backups, document ACP relationships, encrypted indexes and block
//! signatures run the same core operations the node's own calls do.
//!
//! A request runs as the identity it carries, or as none: unlike the node's
//! in-process calls, it never acts as the node.

use std::sync::Arc;

use defra_core::RequestContext;
use defra_http::router::{
    BackupOperations, BlockOperations, DocumentAcpOperations, EncryptedIndexInfo,
    EncryptedIndexOperations, ImportResult, ViewOperations,
};
use schema::CollectionVersion;

use super::handle::NodeOps;
use super::EncryptedIndexListing;

/// [`NodeOps`] serving one request.
#[derive(Clone)]
pub(crate) struct HttpNodeOps {
    ops: NodeOps,
    /// The request these operations serve, once bound to it.
    context: Option<RequestContext>,
}

impl HttpNodeOps {
    pub(crate) fn new(ops: NodeOps) -> Arc<Self> {
        Arc::new(Self { ops, context: None })
    }

    fn bound(&self, context: &RequestContext) -> Arc<Self> {
        Arc::new(Self {
            ops: self.ops.clone(),
            context: Some(context.clone()),
        })
    }

    fn context(&self) -> RequestContext {
        RequestContext::bound_or_ambient(self.context.as_ref()).into_owned()
    }
}

fn text<E>(error: E) -> String
where
    E: std::fmt::Display,
{
    error.to_string()
}

fn info(collection: &str, listing: EncryptedIndexListing) -> EncryptedIndexInfo {
    EncryptedIndexInfo {
        collection: collection.to_owned(),
        field_name: listing.field_name,
        index_type: listing.index_type,
    }
}

#[async_trait::async_trait]
impl ViewOperations for HttpNodeOps {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn ViewOperations>> {
        Some(self.bound(context))
    }

    async fn add_view(
        &self,
        gql_query: &str,
        sdl: &str,
        transform: Option<&str>,
    ) -> Result<Vec<CollectionVersion>, String> {
        self.ops
            .add_view(&self.context(), gql_query, sdl, transform)
            .await
            .map_err(text)
    }

    async fn refresh_views(&self, options: db::CollectionSelector) -> Result<(), String> {
        self.ops
            .refresh_views(&self.context(), options)
            .await
            .map_err(text)
    }

    async fn gc_downsample_histories(&self, names: Option<Vec<String>>) -> Result<(), String> {
        let (database, context) = (self.ops.database.clone(), self.context());
        self.ops
            .on_pool(move || async move {
                database
                    .check_node_access_as(&context, acp::nac::NodePermission::ViewGc)
                    .await?;
                let options = names.map(db::downsample::GcDownsampleHistoriesOptions::with_names);
                database
                    .gc_downsample_histories_as(&context, options)
                    .await
                    .map_err(crate::Error::from)
            })
            .await
            .map(drop)
            .map_err(text)
    }
}

#[async_trait::async_trait]
impl BackupOperations for HttpNodeOps {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn BackupOperations>> {
        Some(self.bound(context))
    }

    async fn export(
        &self,
        collections: Option<Vec<String>>,
        pretty: bool,
    ) -> Result<String, String> {
        self.ops
            .export_backup(self.context(), collections.unwrap_or_default(), pretty)
            .await
            .map_err(text)
    }

    async fn import(&self, data: &str) -> Result<ImportResult, String> {
        let stats = self
            .ops
            .import_backup(self.context(), data.to_owned())
            .await
            .map_err(text)?;
        Ok(ImportResult {
            documents_imported: stats.documents_imported,
            documents_skipped: 0,
            collections_affected: stats.collections_affected,
            errors: Vec::new(),
        })
    }
}

#[async_trait::async_trait]
impl DocumentAcpOperations for HttpNodeOps {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn DocumentAcpOperations>> {
        Some(self.bound(context))
    }

    async fn check_doc_access(
        &self,
        actor: &identity::Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool, String> {
        let acp = self.ops.document_acp.clone();
        let actor = acp::Identity::Authenticated(actor.clone());
        let (policy, resource, doc) = (
            policy_id.to_owned(),
            resource_name.to_owned(),
            doc_id.to_owned(),
        );
        self.ops
            .on_acp(move || async move {
                acp.check_doc_access(&actor, permission, &policy, &resource, &doc)
                    .await
                    .map_err(crate::Error::from)
            })
            .await
            .map_err(text)
    }

    async fn add_doc_relationship(
        &self,
        requestor: &identity::Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        let acp = self.ops.acp_ops.clone();
        let args = Relationship::new(requestor, target_actor, collection, doc_id, relation);
        self.ops
            .on_acp(move || async move {
                let Relationship(identity, target, collection, doc_id, relation) = args;
                acp.add_dac_actor_relationship(&identity, &collection, &doc_id, &relation, &target)
                    .await
            })
            .await
            .map_err(text)
    }

    async fn delete_doc_relationship(
        &self,
        requestor: &identity::Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        let acp = self.ops.acp_ops.clone();
        let args = Relationship::new(requestor, target_actor, collection, doc_id, relation);
        self.ops
            .on_acp(move || async move {
                let Relationship(identity, target, collection, doc_id, relation) = args;
                acp.delete_dac_actor_relationship(
                    &identity,
                    &collection,
                    &doc_id,
                    &relation,
                    &target,
                )
                .await
            })
            .await
            .map_err(text)
    }
}

/// A relationship change's requestor, target, collection, document and
/// relation, owned so they move onto the pool.
struct Relationship(String, String, String, String, String);

impl Relationship {
    fn new(
        requestor: &identity::Did,
        target: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Self {
        Self(
            requestor.to_string(),
            target.to_owned(),
            collection.to_owned(),
            doc_id.to_owned(),
            relation.to_owned(),
        )
    }
}

#[async_trait::async_trait]
impl EncryptedIndexOperations for HttpNodeOps {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn EncryptedIndexOperations>> {
        Some(self.bound(context))
    }

    async fn add_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexInfo, String> {
        self.ops
            .add_encrypted_index(&self.context(), collection, field_name)
            .await
            .map(|listing| info(collection, listing))
            .map_err(text)
    }

    async fn list_encrypted_indexes(
        &self,
        collection: Option<&str>,
    ) -> Result<Vec<EncryptedIndexInfo>, String> {
        let context = self.context();
        match collection {
            Some(name) => self
                .ops
                .list_encrypted_indexes(&context, name)
                .await
                .map(|all| all.into_iter().map(|one| info(name, one)).collect())
                .map_err(text),
            None => self
                .ops
                .list_all_encrypted_indexes(&context)
                .await
                .map(|all| {
                    all.into_iter()
                        .flat_map(|(name, indexes)| {
                            indexes.into_iter().map(move |one| info(&name, one))
                        })
                        .collect()
                })
                .map_err(text),
        }
    }

    async fn delete_encrypted_index(
        &self,
        collection: &str,
        field_name: &str,
    ) -> Result<(), String> {
        self.ops
            .delete_encrypted_index(&self.context(), collection, field_name)
            .await
            .map_err(text)
    }
}

#[async_trait::async_trait]
impl BlockOperations for HttpNodeOps {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn BlockOperations>> {
        Some(self.bound(context))
    }

    async fn signed_block_bytes(
        &self,
        cid: &str,
        _caller_did: Option<&str>,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let context = self.context();
        let caller: acp::Identity = super::request::acting_identity(&context)
            .map_err(text)?
            .into();
        let (database, acp, cid) = (
            self.ops.database.clone(),
            self.ops.document_acp.clone(),
            cid.to_owned(),
        );
        self.ops
            .on_pool(move || async move {
                db::block::verify::authorized_signed_block_bytes_as(
                    &context,
                    &database,
                    acp.as_ref(),
                    &cid,
                    &caller,
                )
                .await
                .map_err(crate::Error::BlockAccess)
            })
            .await
            .map_err(text)
    }

    async fn verify_signature(
        &self,
        cid: &str,
        public_key: &str,
        key_type: Option<&str>,
        _caller_did: Option<&str>,
    ) -> Result<(), String> {
        let key_type = match key_type.unwrap_or("secp256k1") {
            "ed25519" => crypto::KeyType::Ed25519,
            "secp256k1" => crypto::KeyType::Secp256k1,
            "secp256r1" => crypto::KeyType::Secp256r1,
            other => return Err(format!("unsupported key type: {other}")),
        };
        self.ops
            .verify_block_signature(&self.context(), cid, public_key, key_type)
            .await
            .map_err(text)
    }
}
