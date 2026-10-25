//! The merge path publishes heads with no guard key: a replay through a fresh
//! handler, as after a restart, leaves a superseded head superseded.

use std::sync::Arc;

use blockstore::DefraBlockstore;
use db::{merge::merge_handler::DbMergeHandler, DB};
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler};
use query::mutator::DocMutator;
use storage::corekv::IterOptions;
use storage::merge::head_set::Set;
use storage::RegolithStore;

/// Every live name of every document head set, and how many keys the
/// headstore holds under `/r/`.
async fn heads(db: &DB<RegolithStore>) -> (Vec<Vec<u8>>, usize) {
    let txn = db.new_txn(true).await.unwrap();
    let view = txn.headstore().unwrap();
    let mut names = Vec::new();
    let mut sets = view
        .iterator(IterOptions::new().with_prefix(b"/hd/".to_vec()))
        .await
        .unwrap();
    while let Some(pair) = sets.next().await.unwrap() {
        let set = Set::read(&pair.value).unwrap();
        names.extend(set.live().map(|(name, _)| name.to_vec()));
    }
    sets.close().await.unwrap();
    let mut guards = view
        .iterator(IterOptions::new().with_prefix(b"/r/".to_vec()))
        .await
        .unwrap();
    let mut stored = 0;
    while guards.next().await.unwrap().is_some() {
        stored += 1;
    }
    guards.close().await.unwrap();
    (names, stored)
}

async fn merge(
    db: &Arc<DB<RegolithStore>>,
    blocks: &Arc<DefraBlockstore<RegolithStore>>,
    block: &MergeBlock,
) {
    DbMergeHandler::new(db.clone(), blocks.clone())
        .handle_block(
            &block.cid,
            &block.block_data,
            BlockMetadata::normal(
                &block.doc_id,
                &block.collection_id,
                &block.creator,
                None,
                false,
            ),
        )
        .await
        .unwrap();
}

/// A replicated create, a local update of one of its fields, then the create
/// replayed by a handler that has not seen it: the heads are what the update
/// left, and no guard key exists.
#[test]
fn a_replayed_create_after_a_local_update_stays_superseded() {
    workers::block_on(async {
        let store = Arc::new(RegolithStore::in_memory().unwrap());
        let db = Arc::new(DB::from_arc(store.clone()).unwrap());
        for mut schema in query::parse_sdl("type Users { name: String age: Int }").unwrap() {
            schema.version_id = "v1".to_string();
            schema.collection_id = "col-users".to_string();
            db.create_collection(schema).await.unwrap();
        }
        let blocks = Arc::new(DefraBlockstore::new(store, false));
        let genesis = super::merge_handler_tests::build_merge_block(&blocks, "Alice", 30).await;
        merge(&db, &blocks, &genesis).await;
        let (created, guards) = heads(&db).await;
        assert_eq!(created.len(), 3, "the composite and both fields");
        assert_eq!(guards, 0);

        let mutator = db::AutoCommitMutator::new(db.clone());
        let doc_id = document::DocID::from_string(&genesis.doc_id).unwrap();
        let mut doc = mutator
            .get_for_update("Users", &doc_id)
            .await
            .unwrap()
            .unwrap();
        doc.set("name", document::NormalValue::String("Bob".to_string()));
        mutator
            .update(
                "Users",
                doc,
                rapidhash::RapidHashSet::from_iter(["name".to_string()]),
            )
            .await
            .unwrap();
        let (updated, guards) = heads(&db).await;
        assert_eq!(
            updated.len(),
            3,
            "the new composite, the new name, the old age"
        );
        assert_ne!(updated, created);
        assert_eq!(guards, 0);

        merge(&db, &blocks, &genesis).await;
        assert_eq!(heads(&db).await, (updated, 0));
    })
    .unwrap()
}
