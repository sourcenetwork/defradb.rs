//! Go Shinzo's `merge stats` and `purge stats` counts: a replicated merge
//! counts once it commits, as a create when the document had no heads and
//! an update after; retention refusals count by cause; a purge counts the
//! documents it removed once it commits. Each node keeps its own counts.

use blockstore::{Blockstore as _, DefraBlockstore};
use cid::Cid;
use db::stats::MergeReport;
use defra_core::block::Block;
use defra_core::merge::{BlockMetadata, MergeBlock, MergeHandler, MergeOutcome};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use storage::RegolithStore;

use super::merge_handler_tests::{make_handler_with_schema_and_bus, users_collection};

/// Copy `cid` and every block it links from `from` to `to`, as a peer's
/// blocks arrive before their merge.
async fn copy_blocks(
    from: &DefraBlockstore<RegolithStore>,
    to: &DefraBlockstore<RegolithStore>,
    cid: &Cid,
) {
    let data = from.get(cid).await.unwrap().unwrap();
    let block = Block::from_dag_cbor(&data).unwrap();
    to.put(cid, &data).await.unwrap();
    for link in block.links.into_iter().flatten() {
        let linked = from.get(&link.link).await.unwrap().unwrap();
        to.put(&link.link, &linked).await.unwrap();
    }
}

fn replicated(cid: Cid, block: bytes::Bytes, doc_id: &str) -> MergeBlock {
    MergeBlock {
        cid,
        block_data: block,
        doc_id: doc_id.to_string(),
        collection_id: "col-users".to_string(),
        creator: "did:key:z6MkrMergeStatsTest".to_string(),
        sender_peer: Some("peer1".to_string()),
        is_explicit_replicator: false,
        explicit_replay_authorization: None,
        verified_creator: None,
    }
}

#[test]
#[mantra_macros::req_test("go.shinzo.telemetry.merge.report")]
fn a_replicated_create_then_update_count_once_each_on_the_merging_node() {
    workers::block_on(async {
        let (source, source_blocks, _) = make_handler_with_schema_and_bus().await;
        let (target, target_blocks, _) = make_handler_with_schema_and_bus().await;

        let mutator = db::AutoCommitMutator::new(source.db().clone());
        let doc =
            Document::from_json_str(r#"{"name": "Ann", "age": 30}"#, users_collection()).unwrap();
        let created = mutator.create("Users", doc).await.unwrap();
        let mut update = Document::new(users_collection());
        update.set_id(created.doc_id.clone());
        update.set("age", NormalValue::Int(31)).unwrap();
        let updated = mutator
            .update(
                "Users",
                update,
                RapidHashSet::from_iter(["age".to_string()]),
            )
            .await
            .unwrap();
        assert_eq!(
            source.db().merge_stats().drain(),
            MergeReport::default(),
            "local writes are not merges"
        );

        let doc_id = created.doc_id.to_string();
        for (cid, block) in [
            (
                created.commit_cid.unwrap(),
                created.commit_block.clone().unwrap(),
            ),
            (updated.commit_cid.unwrap(), updated.commit_block.unwrap()),
        ] {
            copy_blocks(&source_blocks, &target_blocks, &cid).await;
            let outcome = target
                .handle_block(
                    &cid,
                    &block,
                    BlockMetadata::normal(&doc_id, "col-users", "did:key:z6Mkr", None, false),
                )
                .await
                .unwrap();
            assert_eq!(outcome, MergeOutcome::Merged);
        }

        let report = target.db().merge_stats().drain();
        assert_eq!((report.creates, report.updates), (1, 1), "{report:?}");
        assert_eq!(report.txn_conflicts, 0);
        assert_eq!(source.db().merge_stats().drain(), MergeReport::default());

        // The same create again is already merged: nothing new commits.
        let again = replicated(
            created.commit_cid.unwrap(),
            created.commit_block.clone().unwrap(),
            &doc_id,
        );
        target
            .handle_block(
                &again.cid,
                &again.block_data,
                BlockMetadata::normal(&doc_id, "col-users", &again.creator, None, false),
            )
            .await
            .unwrap();
        let report = target.db().merge_stats().drain();
        assert_eq!((report.creates, report.updates), (0, 0), "{report:?}");
    })
    .unwrap()
}
