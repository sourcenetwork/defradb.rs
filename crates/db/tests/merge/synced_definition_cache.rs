//! A collection definition learned over p2p is rebuilt from its delta and
//! added to the runtime collection cache. That cache is keyed by name, and the
//! insert is unconditional, so a definition that merely shares a name with a
//! collection this node already has takes its place — dropping everything the
//! delta cannot carry.

use std::sync::Arc;

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use defra_core::block::{
    Block, CollectionDefinitionDeltaPayload, CrdtDelta, DAGLink, FieldDefinitionDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use schema::{CType, CollectionVersion, FieldDescription, FieldKind, PolicyDescription};
use storage::RegolithStore;

/// Go's `IntToFieldKind` numeric kind for a nillable string.
const STRING_KIND: u8 = 11;

type Handler = DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>;

fn policy() -> PolicyDescription {
    PolicyDescription {
        id: "policy-agents".to_string(),
        resource_name: "agents".to_string(),
    }
}

/// A node holding `Agent` with a policy and an `@immutable` field.
async fn node() -> (
    Arc<DB<RegolithStore>>,
    Handler,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let mut did = FieldDescription::new("2", "did", FieldKind::string());
    did.immutable = true;
    db.create_collection(
        CollectionVersion::new(
            "Agent",
            "col-agent",
            "col-agent",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                did,
            ],
        )
        .with_policy(policy()),
    )
    .await
    .unwrap();
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    let handler = DbMergeHandler::new(db.clone(), blockstore.clone());
    (db, handler, blockstore)
}

/// A definition block naming `collection`, as a peer sends it.
fn definition(collection: &str, fields: &[&str]) -> (Cid, Vec<(Cid, Vec<u8>)>) {
    let mut blocks = Vec::new();
    let mut links = Vec::new();
    for field in fields {
        let block = Block::new(
            CrdtDelta::FieldDefinition(
                FieldDefinitionDeltaPayload::new(1)
                    .with_name(*field)
                    .with_scalar_kind(STRING_KIND)
                    .with_crdt(CType::LwwRegister.to_u8()),
            ),
            vec![],
            vec![],
        );
        let cid = block.generate_cid().unwrap();
        blocks.push((cid, block.to_dag_cbor().unwrap()));
        links.push(DAGLink::new(*field, cid));
    }
    let block = Block::new(
        CrdtDelta::CollectionDefinition(
            CollectionDefinitionDeltaPayload::new(1).with_name(collection),
        ),
        vec![],
        links,
    );
    let cid = block.generate_cid().unwrap();
    blocks.push((cid, block.to_dag_cbor().unwrap()));
    (cid, blocks)
}

async fn merge(
    handler: &Handler,
    blockstore: &DefraBlockstore<RegolithStore>,
    collection: &str,
) -> MergeOutcome {
    let (cid, blocks) = definition(collection, &["_docID", "did"]);
    for (block_cid, bytes) in &blocks {
        blockstore.put(block_cid, bytes).await.unwrap();
    }
    let bytes = &blocks.last().unwrap().1;
    handler
        .handle_block(
            &cid,
            bytes,
            BlockMetadata::normal("", "", "peer-did", Some("peer"), false),
        )
        .await
        .unwrap()
}

/// What the collection named `Agent` commits to, as this node reads it.
fn commitments(db: &DB<RegolithStore>) -> (Option<PolicyDescription>, bool, String) {
    let collection = db.get_collection("Agent").unwrap().unwrap();
    let schema = collection.schema();
    let immutable = schema
        .fields
        .iter()
        .any(|field| field.name == "did" && field.immutable);
    (
        schema.policy.clone(),
        immutable,
        schema.collection_id.clone(),
    )
}

#[tokio::test]
async fn a_synced_definition_does_not_displace_a_collection_of_the_same_name() {
    let (db, handler, blockstore) = node().await;
    assert_eq!(
        commitments(&db),
        (Some(policy()), true, "col-agent".to_string()),
        "the local definition holds its policy and its immutable field"
    );

    merge(&handler, &blockstore, "Agent").await;

    assert_eq!(
        commitments(&db),
        (Some(policy()), true, "col-agent".to_string()),
        "a definition delta carries no policy and no @immutable flag, so a \
         rebuilt record must not take the local one's place"
    );
}

/// The insert exists so a synced collection is visible to `get_collections`
/// with inactive included. A name this node does not have must still register.
#[tokio::test]
async fn a_synced_definition_for_a_free_name_still_registers() {
    let (db, handler, blockstore) = node().await;

    let outcome = merge(&handler, &blockstore, "Ledger").await;

    assert_eq!(outcome, MergeOutcome::Merged);
    let collection = db.get_collection("Ledger").unwrap().unwrap();
    assert_eq!(collection.schema().name, "Ledger");
    assert!(collection
        .schema()
        .fields
        .iter()
        .any(|field| field.name == "did"));
}
