use std::sync::Arc;

use acp::nac::{NodeACP, NodePermission};
use document::{DocID, NormalValue};
use identity::Did;
use storage::namespace::Namespace;

use crate::{Node, QueryRequest};

const OWNER: &str = "did:key:owner";
const AGENT: &str = "did:key:agent";

struct Projector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for Projector {
    async fn reconcile(&self, _context: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let view = context.view;
        if collection != "ProjectedTool" || doc_id.is_empty() {
            return Ok(());
        }
        let system = view.sibling(Namespace::Systemstore);
        let collection =
            db::collection::loader::load_collection_from_systemstore(&system, collection)
                .await
                .map_err(|e| storage::corekv::Error::Other(e.to_string()))?
                .ok_or_else(|| storage::corekv::Error::Other("missing collection".into()))?;
        let id =
            DocID::from_string(doc_id).map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let doc = collection
            .get_by_doc_id(view, &system, &id)
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let enabled = doc.as_ref().is_some_and(|doc| {
            matches!(
                doc.get("enabled"),
                Some(NormalValue::Bool(true) | NormalValue::NillableBool(Some(true)))
            )
        });
        let source = format!("tool:{doc_id}");
        let nac = NodeACP::new(Arc::new(
            acp::zanzibar::store::TransactionZanzibarStore::new(view.sibling(Namespace::Acpstore)),
        ));
        nac.load()
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        let owner = Did::new(OWNER).unwrap();
        if enabled {
            nac.change_source_permission(
                &owner,
                NodePermission::CollectionPatch,
                Some(&source),
                "actor",
                AGENT,
                false,
            )
            .await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        }
        nac.change_source_permission(
            &owner,
            NodePermission::CollectionPatch,
            None,
            "source",
            &source,
            !enabled,
        )
        .await
        .map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        Ok(())
    }
}

fn request(text: String) -> QueryRequest {
    QueryRequest::new(text).with_identity(Some(Did::new(OWNER).unwrap()))
}

async fn allowed(node: &Node) -> bool {
    node.nac_manager()
        .check_permission(&Did::new(AGENT).unwrap(), NodePermission::CollectionPatch)
        .await
        .unwrap()
}

#[test]
fn raw_document_mutations_stage_sources_in_the_same_transaction() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_acp_owner(OWNER)
            .with_node_identity_did(OWNER)
            .with_document_projector(Arc::new(Projector))
            .build().await.unwrap();
        let owner = defra_core::RequestContext::default().with_identity(Some(OWNER.to_owned()));
        node.add_collection_as(&owner, "type ProjectedTool { enabled: Boolean, name: String }").await.unwrap();
        let created = node.execute_request(request("mutation { create_ProjectedTool(input:{enabled:true, name:\"one\"}) { _docID } }".into())).await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let id = created.data.unwrap()["add_ProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        let second = node.execute_request(request("mutation { create_ProjectedTool(input:{enabled:true, name:\"two\"}) { _docID } }".into())).await;
        assert!(!second.has_errors(), "{:?}", second.errors);
        let second_id = second.data.unwrap()["add_ProjectedTool"][0]["_docID"].as_str().unwrap().to_owned();
        assert!(allowed(&node).await);

        let tx = node.runner().begin_txn_as(&owner, false).await.unwrap();
        let disabled = node.execute_request_in_txn(request(format!("mutation {{ update_ProjectedTool(docID:\"{id}\", input:{{enabled:false}}) {{ _docID }} }}")), &tx).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await);
        node.runner().discard_txn_as(&owner, &tx).await.unwrap();
        assert!(allowed(&node).await);

        node.add_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        let disabled = node.execute_request(request(format!("mutation {{ update_ProjectedTool(docID:\"{id}\", input:{{enabled:false}}) {{ _docID }} }}"))).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await);
        node.delete_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        assert!(allowed(&node).await, "the other tool source remains");
        node.add_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        let disabled = node.execute_request(request(format!("mutation {{ update_ProjectedTool(docID:\"{second_id}\", input:{{enabled:false}}) {{ _docID }} }}"))).await;
        assert!(!disabled.has_errors(), "{:?}", disabled.errors);
        assert!(allowed(&node).await, "the direct grant remains");
        node.delete_nac_actor_relationship_as(&owner, NodePermission::CollectionPatch.as_str(), AGENT).await.unwrap();
        assert!(!allowed(&node).await);
        node.shutdown().await.unwrap();
    }).unwrap();
}

struct RefusingProjector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for RefusingProjector {
    async fn reconcile(&self, _context: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        let view = context.view;
        if collection == "ProjectedTool" && !doc_id.is_empty() {
            view.sibling(Namespace::Acpstore)
                .set(b"test-projection", b"staged")
                .await?;
            return Err(storage::corekv::Error::Other("projection rejected".into()));
        }
        Ok(())
    }
}

#[test]
fn refused_projection_rolls_back_document_and_staged_native_changes() {
    workers::block_on(async {
        let node = Node::builder()
            .with_node_identity_did(OWNER)
            .with_document_projector(Arc::new(RefusingProjector))
            .build()
            .await
            .unwrap();
        node.add_collection("type ProjectedTool { enabled: Boolean }")
            .await
            .unwrap();
        let result = node
            .execute("mutation { create_ProjectedTool(input:{enabled:true}) { _docID } }")
            .await;
        assert!(result.has_errors());
        let docs = node.execute("{ ProjectedTool { _docID } }").await;
        assert!(!docs.has_errors(), "{:?}", docs.errors);
        assert!(
            docs.data.unwrap()["ProjectedTool"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let txn = node.database().new_txn(true).await.unwrap();
        assert!(
            txn.acpstore()
                .unwrap()
                .get(b"test-projection")
                .await
                .unwrap()
                .is_none()
        );
        txn.discard().unwrap();
        node.shutdown().await.unwrap();
    })
    .unwrap();
}

struct ForeignOwnerProjector;

#[async_trait::async_trait]
impl db::DocumentMutationProjector for ForeignOwnerProjector {
    async fn reconcile(&self, _context: &db::DocumentMutationContext<'_>) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        context: &db::DocumentMutationContext<'_>,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
        if collection != "ForeignProjected" || doc_id.is_empty() {
            return Ok(());
        }
        let system = context.view.sibling(Namespace::Systemstore);
        let collection = db::collection::loader::load_collection_from_systemstore(&system, collection)
            .await.map_err(|e| storage::corekv::Error::Other(e.to_string()))?
            .ok_or_else(|| storage::corekv::Error::Other("missing projected schema".into()))?;
        let policy = collection.schema().policy.as_ref()
            .ok_or_else(|| storage::corekv::Error::Other("missing projected policy".into()))?;
        let acp_view = context.view.sibling(Namespace::Acpstore);
        let scoped = context.document_acp.bind_transaction(acp_view.clone())
            .ok_or_else(|| storage::corekv::Error::Other("configured ACP cannot bind".into()))?;
        let owner = scoped.get_doc_owner(&policy.id, &policy.resource_name, doc_id).await
            .map_err(|e| storage::corekv::Error::Other(e.to_string()))?
            .ok_or_else(|| storage::corekv::Error::Other("creator not registered".into()))?;
        if &owner == context.node_did {
            return Err(storage::corekv::Error::Other("expected foreign document owner".into()));
        }
        context.document_acp.change_node_admin_relationship(
            acp_view, context.node_did, acp::Subject::wildcard(), &policy.id,
            &policy.resource_name, doc_id, "reader", false,
        ).await.map_err(|e| storage::corekv::Error::Other(e.to_string()))?;
        Ok(())
    }
}

#[test]
fn local_signed_foreign_creator_is_registered_before_projector_admin_grant() {
    use crypto::{Key as _, PrivateKey as _};
    use identity::Identity as _;
    workers::block_on(async {
        let private = crypto::generate_ed25519().unwrap();
        let public = private.public_key();
        let signing = defra_core::signing::SigningConfig {
            key_type: defra_core::signing::SigningKeyType::Ed25519,
            private_key_bytes: private.raw_owned(),
            public_key_bytes: public.raw_owned(),
            public_key_hex: hex::encode(public.raw()),
            remote_signer: None,
            signing_authorization: None,
        };
        let user = identity::RawIdentity::from_private_key(private).unwrap();
        let user_did = user.did().unwrap();
        let node = Node::builder().enable_signing().with_node_acp_enabled()
            .with_document_projector(Arc::new(ForeignOwnerProjector)).build().await.unwrap();
        let node_did = node.node_identity_did().unwrap().to_owned();
        let owner = defra_core::RequestContext::default().with_identity(Some(node_did));
        for permission in [NodePermission::DocumentUpdate, NodePermission::DocumentRead, NodePermission::CollectionGet] {
            node.add_nac_actor_relationship_as(&owner, permission.as_str(), user_did.as_str()).await.unwrap();
        }
        let policy = node.add_dac_policy_as(&owner, "name: Projection fixture\nresources:\n  - name: records\n    relations:\n      - name: reader\n        types: [actor]\n    permissions:\n      - name: read\n        expr: reader\n      - name: update\n      - name: delete\n").await.unwrap();
        node.add_collection_as(&owner, &format!("type ForeignProjected @policy(id: \"{policy}\", resource: \"records\") {{ name: String }}")).await.unwrap();
        let response = node.execute_request_with_signer(
            QueryRequest::new("mutation { create_ForeignProjected(input:{name:\"one\"}) { _docID } }")
                .with_identity(Some(user_did.clone())),
            &crate::EmbeddedSigner::from_config(signing).unwrap(),
        ).await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let doc_id = response.data.unwrap()["add_ForeignProjected"][0]["_docID"].as_str().unwrap().to_owned();
        assert_eq!(node.document_acp().get_doc_owner(&policy, "records", &doc_id).await.unwrap(), Some(user_did));
        assert!(node.document_acp().check_named_permission(&Did::new("did:key:reader").unwrap(), "read", &policy, "records", &doc_id).await.unwrap());
        node.shutdown().await.unwrap();
    }).unwrap();
}
