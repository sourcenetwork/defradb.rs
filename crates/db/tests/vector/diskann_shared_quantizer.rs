//! One quantizer per DiskANN index, shared by every write transaction on the
//! node under the training that drew its codebooks.

use crate::diskann_streaming::{fresh, trained_store, value};
use db::index::vector::engine::diskann::codec::{self, Trained};
use db::index::vector::engine::diskann::CodeCaches;
use db::index::vector::index::VectorIndex;
use db::index::vector::store::{KvNodeStore, VectorNodeStore};
use storage::corekv::Store;
use storage::index::CollectionIndex;
use storage::RegolithStore;

const COLLECTION: u32 = 31;
const INDEX_ID: u32 = 3;

async fn training(store: &RegolithStore) -> u128 {
    let mut read = store.new_txn(true).await.unwrap();
    let bytes = KvNodeStore::new(&mut read, COLLECTION, INDEX_ID, 0)
        .get_aux(codec::TRAINED, b"")
        .await
        .unwrap()
        .expect("trained");
    read.discard();
    codec::decode_trained(&bytes)
        .unwrap()
        .training
        .expect("a training draws its tag")
}

async fn insert(store: &RegolithStore, index: &VectorIndex, id: u64, seed: u64) {
    let mut write = store.new_txn(false).await.unwrap();
    index
        .save(&mut write, id, &value(&fresh(seed)))
        .await
        .unwrap();
    write.commit().await.unwrap();
}

#[test]
fn trained_record_reads_back_with_and_without_its_training() {
    for trained in [
        Trained {
            dimensions: 768,
            m: 96,
            training: Some(u128::MAX - 7),
        },
        Trained {
            dimensions: 768,
            m: 96,
            training: None,
        },
    ] {
        assert_eq!(
            codec::decode_trained(&codec::encode_trained(&trained)).unwrap(),
            trained
        );
    }
    let original = [0x01, 0, 3, 0, 0, 96, 0, 0, 0];
    assert_eq!(
        codec::decode_trained(&original).unwrap(),
        Trained {
            dimensions: 768,
            m: 96,
            training: None
        },
        "a record written before the training was recorded still loads"
    );
    let mut tagged = codec::encode_trained(&Trained {
        dimensions: 768,
        m: 96,
        training: Some(1),
    });
    tagged.pop();
    assert!(codec::decode_trained(&tagged).is_err());
}

#[test]
fn a_write_shares_the_quantizer_under_its_training() {
    workers::block_on(async {
        let (store, mut index) = trained_store().await;
        let caches = CodeCaches::default();
        index.share_quantizer(&caches);
        let training = training(&store).await;
        let cache = caches.get(COLLECTION, INDEX_ID);
        assert!(cache.quantizer(training).is_none());

        insert(&store, &index, 70_001, 0xA1).await;
        let shared = cache.quantizer(training).expect("the write shared it");

        insert(&store, &index, 70_002, 0xA2).await;
        assert!(
            std::sync::Arc::ptr_eq(&shared, &cache.quantizer(training).unwrap()),
            "the next write reuses it rather than building another"
        );
    })
    .unwrap()
}

#[test]
fn a_quantizer_under_another_training_is_not_used() {
    workers::block_on(async {
        let (store, mut index) = trained_store().await;
        let training = training(&store).await;
        let built = CodeCaches::default();
        index.share_quantizer(&built);
        insert(&store, &index, 71_001, 0xB1).await;
        let quantizer = built.get(COLLECTION, INDEX_ID).quantizer(training).unwrap();

        let caches = CodeCaches::default();
        let cache = caches.get(COLLECTION, INDEX_ID);
        cache.share_quantizer(training ^ 1, quantizer);
        index.share_quantizer(&caches);
        insert(&store, &index, 71_002, 0xB2).await;
        assert!(
            cache.quantizer(training).is_some(),
            "the write built its own training's quantizer and shared it"
        );
        assert!(cache.quantizer(training ^ 1).is_none());
    })
    .unwrap()
}
