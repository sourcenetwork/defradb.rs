//! A peer's delete of a chunk merges only once the chunk's parent is gone here.

use defra_core::merge::MergeOutcome;
use document::DocID;

use super::chunk_update_journal::{
    merge_outcome, merge_updated_chunk, node, owner, Node, BODY, CHUNKS,
};

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
fn a_peer_cannot_delete_a_chunk_its_live_parent_lists() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert_waits(&outcome, "parent is live");
        assert_eq!(shown(&peer).await, [chunk_id]);
    })
    .unwrap()
}

#[test]
fn a_peer_cannot_delete_a_chunk_its_live_parent_does_not_list_now() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article_with(&peer, "Other text.").await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let delete = write_chunk_delete(&author, &chunk_id).await;

        let outcome = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert_waits(&outcome, "parent is live");
        assert!(chunk_is_stored(&peer, &chunk_id).await);
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

        peer.run(
            &format!(r#"mutation {{ delete_Article(docID: "{parent}") {{ _docID }} }}"#),
            Some(owner()),
        )
        .await;
        let retried = merge_outcome(&author, &peer, &delete, &chunk_id)
            .await
            .unwrap();

        assert!(retried.is_merged(), "{retried:?}");
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
