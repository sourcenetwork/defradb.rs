//! A peer's delete of a chunk merges once the chunk's parent here is gone or
//! no longer uses it.

use std::sync::Arc;

use defra_core::merge::MergeOutcome;
use document::{DocID, Document, NormalValue};
use query::mutator::DocMutator;

use super::chunk_update_journal::{
    create_chunk, merge_outcome, merge_outcome_in, merge_updated_chunk, node, node_with_policy,
    owner, Node, BODY, CHUNKS,
};
use db::merge::merge_handler::MergeError;

async fn add_article(node: &Node) -> String {
    add_article_with(node, BODY).await
}

async fn add_article_with(node: &Node, body: &str) -> String {
    let created = node
        .run(
            &format!(r#"mutation {{ add_Article(input: {{body: "{body}"}}) {{ _docID }} }}"#),
            Some(owner()),
        )
        .await;
    created["add_Article"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The CID of a delete of `chunk_id` written on `author`.
async fn write_chunk_delete(author: &Node, chunk_id: &str) -> cid::Cid {
    let chunks = author.chunks();
    let txn = author.db.new_txn(false).await.unwrap();
    let systemstore = txn.systemstore().unwrap();
    let doc_ref = db::docid::map::get_doc_ref(&systemstore, chunk_id)
        .await
        .unwrap()
        .unwrap();
    let deleted = db::block::builder::write_delete_block(
        &txn.blockstore().unwrap(),
        &txn.headstore().unwrap(),
        chunk_id,
        doc_ref.doc_short_id,
        chunks.version_id(),
        None,
    )
    .await
    .unwrap();
    db::docid::map::set_block_doc_id_mapping(&systemstore, &deleted.cid.to_string(), chunk_id)
        .await
        .unwrap();
    drop(systemstore);
    txn.force_commit().await.unwrap();
    deleted.cid
}

async fn shown(node: &Node) -> Vec<String> {
    let data = node
        .run(&format!("{{ {CHUNKS} {{ _docID }} }}"), Some(owner()))
        .await;
    data[CHUNKS]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["_docID"].as_str().unwrap().to_string())
        .collect()
}

async fn chunk_is_stored(node: &Node, chunk_id: &str) -> bool {
    let txn = node.db.new_txn(true).await.unwrap();
    let found = node
        .chunks()
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            &DocID::from_string(chunk_id).unwrap(),
        )
        .await
        .unwrap();
    txn.force_discard().unwrap();
    found.is_some()
}

fn assert_waits(outcome: &MergeOutcome, why: &str) {
    let MergeOutcome::Skipped {
        reason,
        terminal: false,
    } = outcome
    else {
        panic!("expected the delete to wait, got {outcome:?}");
    };
    assert!(reason.contains(why), "{reason}");
}

#[test]
fn a_peer_cannot_delete_a_chunk_its_live_parent_uses() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert_waits(&outcome, "parent is live and uses it");
        assert!(chunk_is_stored(&peer, &chunk_id).await);
    })
    .unwrap()
}

#[test]
fn a_peer_deletes_a_chunk_its_live_parent_does_not_use() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        // Stored on both nodes outside any write, under one ID, as an encrypted
        // chunk would be: no merge on the peer builds or drops it.
        let (_, _, created) = create_chunk(&author, &parent, "Other text.", None).await;
        let (_, _, copy) = create_chunk(&peer, &parent, "Other text.", None).await;
        assert_eq!(copy.doc_id, created.doc_id);
        let delete = write_chunk_delete(&author, &created.doc_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &created.doc_id)
            .await
            .unwrap();

        assert!(outcome.is_merged(), "{outcome:?}");
        assert!(!chunk_is_stored(&peer, &created.doc_id).await);
    })
    .unwrap()
}

#[test]
fn a_held_back_delete_merges_once_the_parent_stops_using_the_chunk() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;
        let early = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();
        assert_waits(&early, "parent is live and uses it");

        peer.run(
            &format!(
                r#"mutation {{ update_Article(docID: "{parent}", input: {{body: "Other text."}}) {{ _docID }} }}"#
            ),
            Some(owner()),
        )
        .await;
        assert!(!chunk_is_stored(&peer, &chunk_id).await);
        let retried = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        // The edit deleted the chunk already, so the delete has nothing left to wait for.
        let settled = match &retried {
            MergeOutcome::Skipped { reason, .. } => MergeOutcome::skip_reason_is_applied(reason),
            other => other.is_merged(),
        };
        assert!(settled, "{retried:?}");
        assert!(!chunk_is_stored(&peer, &chunk_id).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_delete_that_arrives_before_its_parents_merges_after_it() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;
        let early = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();
        assert_waits(&early, "parent is live");
        assert_eq!(shown(&peer).await, std::slice::from_ref(&chunk_id));

        peer.run(
            &format!(r#"mutation {{ delete_Article(docID: "{parent}") {{ _docID }} }}"#),
            Some(owner()),
        )
        .await;
        let retried = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        // The local delete already took the chunk with its parent.
        let MergeOutcome::Skipped { reason, .. } = &retried else {
            panic!("expected the chunk to be gone already, got {retried:?}");
        };
        assert!(MergeOutcome::skip_reason_is_applied(reason), "{reason}");
        assert!(!chunk_is_stored(&peer, &chunk_id).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_delete_waits_for_a_parent_that_has_not_arrived() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let missing = DocID::new_v0_from_seed("parent not merged yet").to_string();
        let chunk_id = merge_updated_chunk(&author, &peer, &missing).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert_waits(&outcome, "has not arrived");
        assert!(chunk_is_stored(&peer, &chunk_id).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_delete_cannot_merge_before_its_chunk() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        // A parent the peer lacks, so no write or merge of its own holds this chunk.
        let parent = DocID::new_v0_from_seed("parent not merged yet").to_string();
        let (_, _, created) = create_chunk(&author, &parent, BODY, None).await;
        let delete = write_chunk_delete(&author, &created.doc_id).await;

        let early = merge_outcome(&author, &peer, &delete, &created.doc_id).await;
        let Err(MergeError::MergeFailed(reason)) = early else {
            panic!("expected the delete to name no chunk yet, got {early:?}");
        };
        assert!(reason.contains("no reachable genesis"), "{reason}");

        let chunk = merge_outcome(&author, &peer, &created.cid, &created.doc_id)
            .await
            .unwrap();
        assert!(chunk.is_merged(), "{chunk:?}");
        assert!(chunk_is_stored(&peer, &created.doc_id).await);
        let retried = merge_outcome(&author, &peer, &delete, &created.doc_id)
            .await
            .unwrap();
        assert_waits(&retried, "has not arrived");
    })
    .unwrap()
}

#[test]
fn a_chunk_delete_waits_for_a_parent_merged_without_a_value() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let created = author
            .run(
                r#"mutation { add_Article(input: {}) { _docID _version { cid } } }"#,
                Some(owner()),
            )
            .await;
        let parent = created["add_Article"][0]["_docID"].as_str().unwrap();
        let genesis: cid::Cid = created["add_Article"][0]["_version"][0]["cid"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let articles = author.db.get_collection("Article").unwrap().unwrap();
        let merged = merge_outcome_in(&author, &peer, &genesis, parent, articles.collection_id())
            .await
            .unwrap();
        assert!(merged.is_merged(), "{merged:?}");
        let chunk_id = merge_updated_chunk(&author, &peer, parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert_waits(&outcome, "parent has not arrived");
        assert!(chunk_is_stored(&peer, &chunk_id).await);
    })
    .unwrap()
}

#[test]
fn a_parent_delete_and_its_chunk_deletes_merge_on_a_peer() {
    workers::block_on(async {
        let author = node_with_policy(false).await;
        let peer = node_with_policy(false).await;
        let articles = author.db.get_collection("Article").unwrap().unwrap();
        let mutator = db::AutoCommitMutator::new(Arc::clone(&author.db));
        let mut article = Document::new(articles.schema_arc());
        article
            .set("body", NormalValue::String(BODY.into()))
            .unwrap();
        let created = mutator.create("Article", article).await.unwrap();
        let parent = created.doc_id.to_string();
        let merged = merge_outcome_in(
            &author,
            &peer,
            &created.commit_cid.unwrap(),
            &parent,
            articles.collection_id(),
        )
        .await
        .unwrap();
        assert!(merged.is_merged(), "{merged:?}");
        assert!(!created.derived.is_empty());
        // The peer built the same chunks when it merged the parent.
        for chunk in &created.derived {
            let merged =
                merge_outcome(&author, &peer, &chunk.commit_cid, &chunk.doc_id.to_string())
                    .await
                    .unwrap();
            assert!(
                matches!(&merged, MergeOutcome::Skipped { reason, .. } if MergeOutcome::skip_reason_is_applied(reason)),
                "{merged:?}"
            );
        }
        assert_eq!(shown(&peer).await.len(), created.derived.len());

        let deleted = mutator.delete("Article", &created.doc_id).await.unwrap();
        assert_eq!(deleted.derived.len(), created.derived.len());
        for chunk in &deleted.derived {
            let early = merge_outcome(&author, &peer, &chunk.commit_cid, &chunk.doc_id.to_string())
                .await
                .unwrap();
            assert_waits(&early, "parent is live");
        }
        let parent_deleted = merge_outcome_in(
            &author,
            &peer,
            &deleted.commit_cid.unwrap(),
            &parent,
            articles.collection_id(),
        )
        .await
        .unwrap();
        assert!(parent_deleted.is_merged(), "{parent_deleted:?}");
        for chunk in &deleted.derived {
            let chunk_id = chunk.doc_id.to_string();
            let retried = merge_outcome(&author, &peer, &chunk.commit_cid, &chunk_id)
                .await
                .unwrap();
            assert!(retried.is_merged(), "{retried:?}");
            assert!(!chunk_is_stored(&peer, &chunk_id).await);
        }
    })
    .unwrap()
}
