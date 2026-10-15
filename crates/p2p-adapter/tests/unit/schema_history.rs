use std::sync::Arc;

use blockstore::{Blockstore, DefraBlockstore};
use db::{merge::DbMergeHandler, DB};
use defra_core::{
    block::{CollectionDefinitionDeltaPayload, FieldDefinitionDeltaPayload},
    Block, CrdtDelta, DAGLink,
};
use storage::{corekv::Key, keys::systemstore::CollectionKey, RegolithStore};

use super::merge_schema_history;

async fn definition(
    store: &impl Blockstore,
    name: Option<&str>,
    heads: Vec<cid::Cid>,
    field: &str,
) -> cid::Cid {
    let block = Block::new(
        CrdtDelta::FieldDefinition(
            FieldDefinitionDeltaPayload::new(1)
                .with_name(field)
                .with_scalar_kind(13),
        ),
        vec![],
        vec![],
    );
    let field_cid = block.generate_cid().unwrap();
    store
        .put(&field_cid, &block.to_dag_cbor().unwrap())
        .await
        .unwrap();
    let mut payload = CollectionDefinitionDeltaPayload::new(if heads.is_empty() { 1 } else { 2 });
    payload.name = name.map(str::to_owned);
    let block = Block::new(
        CrdtDelta::CollectionDefinition(payload),
        heads,
        vec![DAGLink::new(field, field_cid)],
    );
    let cid = block.generate_cid().unwrap();
    store
        .put(&cid, &block.to_dag_cbor().unwrap())
        .await
        .unwrap();
    cid
}

#[tokio::test]
async fn fetched_patch_registers_predecessors_on_a_fresh_receiver() {
    for placeholder in [false, true] {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let genesis = definition(blocks.as_ref(), Some("Users"), vec![], "name").await;
        let patch = definition(blocks.as_ref(), None, vec![genesis], "age").await;
        let tip = definition(blocks.as_ref(), None, vec![patch], "city").await;
        if placeholder {
            let mut schema = schema::CollectionVersion::new(
                "Users",
                genesis.to_string(),
                genesis.to_string(),
                vec![],
            );
            schema.is_placeholder = true;
            schema.is_active = false;
            schema.root_id = 1;
            let txn = db.new_txn(false).await.unwrap();
            txn.systemstore()
                .unwrap()
                .set(
                    &storage::keys::systemstore::CollectionID::new(genesis.to_string()).bytes(),
                    b"1",
                )
                .await
                .unwrap();
            txn.systemstore()
                .unwrap()
                .set(
                    &CollectionKey::new(genesis.to_string()).bytes(),
                    &serde_json::to_vec(&schema).unwrap(),
                )
                .await
                .unwrap();
            txn.commit().await.unwrap();
        }
        let handler = DbMergeHandler::new(db.clone(), blocks.clone());
        merge_schema_history(tip, blocks.as_ref(), &handler)
            .await
            .unwrap();
        let versions = db.get_all_collection_versions().await.unwrap();
        assert_eq!(versions.len(), 3);
        assert!(versions
            .iter()
            .all(|version| !version.is_placeholder && !version.is_active));
        let tip_schema = versions
            .iter()
            .find(|version| version.version_id == tip.to_string())
            .unwrap();
        for field in ["name", "age", "city"] {
            assert!(tip_schema
                .fields
                .iter()
                .any(|description| description.name == field));
        }
        let fresh_handler = DbMergeHandler::new(db.clone(), blocks.clone());
        merge_schema_history(tip, blocks.as_ref(), &fresh_handler)
            .await
            .unwrap();
        assert_eq!(db.get_all_collection_versions().await.unwrap(), versions);
    }
}

#[tokio::test]
async fn missing_predecessor_is_reported_and_can_be_retried_after_delivery() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let blocks = Arc::new(DefraBlockstore::new(store, false));
    let genesis = definition(blocks.as_ref(), Some("Users"), vec![], "name").await;
    let patch = definition(blocks.as_ref(), None, vec![genesis], "age").await;
    let bytes = blocks.get(&genesis).await.unwrap().unwrap();
    blocks.delete(&genesis).await.unwrap();
    let handler = DbMergeHandler::new(db.clone(), blocks.clone());
    assert!(merge_schema_history(patch, blocks.as_ref(), &handler)
        .await
        .is_err());
    assert!(db.get_all_collection_versions().await.unwrap().is_empty());
    blocks.put(&genesis, &bytes).await.unwrap();
    merge_schema_history(patch, blocks.as_ref(), &handler)
        .await
        .unwrap();
    assert_eq!(db.get_all_collection_versions().await.unwrap().len(), 2);
}

#[tokio::test]
async fn an_unapplied_definition_is_not_reported_as_success() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let blocks = Arc::new(DefraBlockstore::new(store, false));
    let root = definition(blocks.as_ref(), None, vec![], "name").await;
    let handler = DbMergeHandler::new(db.clone(), blocks.clone());
    let error = merge_schema_history(root, blocks.as_ref(), &handler)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("was not applied"), "{error}");
    assert!(db.get_all_collection_versions().await.unwrap().is_empty());
}
