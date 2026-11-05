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
    async fn reconcile(&self, _view: &db::DocumentMutationView) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        view: &db::DocumentMutationView,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
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
    async fn reconcile(&self, _view: &db::DocumentMutationView) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        view: &db::DocumentMutationView,
        collection: &str,
        doc_id: &str,
    ) -> storage::corekv::Result<()> {
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
