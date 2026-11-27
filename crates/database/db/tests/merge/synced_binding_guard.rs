//! A peer's definition block can't bring in a `@policy(relation:)` binding
//! the add would refuse: one on a bound `_xID` field that is not `@immutable`.

use blockstore::Blockstore as _;
use cid::Cid;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::{Block, CrdtDelta, DAGLink};

use super::chunk_definition::{node, Node};

const SDL: &str = r#"
    type Book @governed(root: "root-a") @policy(id: "p1", resource: "book") {
        title: String
        author: Author @immutable @policy(relation: "author")
    }
    type Author @policy(id: "p1", resource: "author") {
        name: String
    }
"#;

async fn block(node: &Node, cid: &Cid) -> Block {
    let bytes = node.blockstore.get(cid).await.unwrap().expect("block");
    Block::from_dag_cbor(&bytes).unwrap()
}

async fn put(node: &Node, block: &Block) -> (Cid, Vec<u8>) {
    let bytes = block.to_dag_cbor().unwrap();
    let cid = block.generate_cid().unwrap();
    node.blockstore.put(&cid, &bytes).await.unwrap();
    (cid, bytes)
}

/// `Book`'s definition block as its author wrote it, but with the bound
/// field's definition saying it is not immutable, stored on `peer` with its
/// field blocks.
async fn mutable_binding(author: &Node, peer: &Node, version_id: &str) -> (Cid, Vec<u8>) {
    let definition = block(author, &Cid::try_from(version_id).unwrap()).await;
    let mut links = Vec::new();
    for link in definition.links.iter().flatten() {
        let mut field = block(author, &link.link).await;
        if let CrdtDelta::FieldDefinition(payload) = &mut field.delta {
            if payload.policy_relation.is_some() {
                payload.immutable = false;
            }
        }
        let (cid, _) = put(peer, &field).await;
        links.push(DAGLink {
            name: link.name.clone(),
            link: cid,
        });
    }
    let forged = Block::new(
        definition.delta.clone(),
        definition.heads.clone().unwrap_or_default(),
        links,
    );
    put(peer, &forged).await
}

#[test]
fn a_synced_binding_on_a_mutable_field_is_rejected_and_nothing_is_kept() {
    workers::block_on(async {
        let author = node();
        let created = author
            .db
            .add_collection(&defra_core::RequestContext::ambient(), SDL)
            .await
            .unwrap();
        let book = created
            .iter()
            .find(|collection| collection.name == "Book")
            .expect("Book");
        let peer = node();
        let (cid, bytes) = mutable_binding(&author, &peer, &book.version_id).await;
        let outcome = DbMergeHandler::new(peer.db.clone(), peer.blockstore.clone())
            .handle_block(
                &cid,
                &bytes,
                BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
            )
            .await
            .unwrap();
        match &outcome {
            MergeOutcome::Rejected { reason } => {
                assert!(reason.contains("the field must be @immutable"), "{reason}")
            }
            other => panic!("expected a rejection, got {other:?}"),
        }
        assert!(peer
            .db
            .get_collection_by_version_id_full(&cid.to_string())
            .await
            .unwrap()
            .is_none());
        assert!(peer
            .db
            .get_all_collection_versions()
            .await
            .unwrap()
            .is_empty());
    })
    .unwrap()
}
