//! Writes and merges delete the chunks their text no longer uses, and make a
//! deleted chunk again under its next generation.

use std::sync::Arc;

use defra_core::merge::MergeOutcome;
use document::{DocID, Document, NormalValue};
use query::mutator::{DerivedWrite, DocMutator};

use super::chunk_update_journal::{
    create_chunk, merge_outcome, merge_outcome_in, node_with_policy, Node, BODY,
};

const OTHER: &str = "Other text.";

fn created(derived: &[DerivedWrite]) -> Vec<&DerivedWrite> {
    derived
        .iter()
        .filter(|write| write.document.is_some())
        .collect()
}

fn deleted(derived: &[DerivedWrite]) -> Vec<&DerivedWrite> {
    derived
        .iter()
        .filter(|write| write.document.is_none())
        .collect()
}

async fn stored(node: &Node, chunk_id: &DocID) -> Option<Document> {
    let txn = node.db.new_txn(true).await.unwrap();
    let found = node
        .chunks()
        .get_by_doc_id(
            &txn.datastore().unwrap(),
            &txn.systemstore().unwrap(),
            chunk_id,
        )
        .await
        .unwrap();
    txn.force_discard().unwrap();
    found
}

struct Writer {
    node: Node,
    mutator: db::AutoCommitMutator<storage::RegolithStore>,
}

impl Writer {
    async fn new() -> Self {
        let node = node_with_policy(false).await;
        let mutator = db::AutoCommitMutator::new(Arc::clone(&node.db));
        Self { node, mutator }
    }

    fn article(&self, body: &str) -> Document {
        let articles = self.node.db.get_collection("Article").unwrap().unwrap();
        let mut article = Document::new(articles.schema_arc());
        article
            .set("body", NormalValue::String(body.into()))
            .unwrap();
        article
    }

    async fn create(&self, body: &str) -> query::mutator::CreateResult {
        self.mutator
            .create("Article", self.article(body))
            .await
            .unwrap()
    }

    async fn update(&self, doc_id: &DocID, body: &str) -> query::mutator::UpdateResult {
        let mut article = self.article(body);
        article.set_id(doc_id.clone());
        self.mutator
            .update(
                "Article",
                article,
                ["body".to_string()].into_iter().collect(),
            )
            .await
            .unwrap()
    }

    /// Merges the parent's block `cid` into `peer`.
    async fn merge_parent(&self, peer: &Node, cid: &cid::Cid, doc_id: &DocID) {
        let articles = self.node.db.get_collection("Article").unwrap().unwrap();
        let outcome = merge_outcome_in(
            &self.node,
            peer,
            cid,
            &doc_id.to_string(),
            articles.collection_id(),
        )
        .await
        .unwrap();
        assert!(outcome.is_merged(), "{outcome:?}");
    }
}

#[test]
fn an_update_deletes_the_chunks_its_text_no_longer_uses() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let made = writer.create(BODY).await;
        let old = created(&made.derived)[0].doc_id.clone();

        let updated = writer.update(&made.doc_id, OTHER).await;

        let gone: Vec<_> = deleted(&updated.derived)
            .iter()
            .map(|w| w.doc_id.clone())
            .collect();
        assert_eq!(gone, std::slice::from_ref(&old));
        assert!(stored(&writer.node, &old).await.is_none());
        let new = created(&updated.derived)[0].doc_id.clone();
        assert!(stored(&writer.node, &new).await.is_some());
    })
    .unwrap()
}

#[test]
fn a_revert_makes_a_deleted_chunk_again_under_the_next_generation() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let made = writer.create(BODY).await;
        let first = created(&made.derived)[0].doc_id.clone();
        writer.update(&made.doc_id, OTHER).await;

        let reverted = writer.update(&made.doc_id, BODY).await;

        let again = created(&reverted.derived)[0];
        assert_ne!(again.doc_id, first);
        let chunk = stored(&writer.node, &again.doc_id).await.unwrap();
        assert_eq!(chunk.get("generation"), Some(&NormalValue::Int(1)));
        assert!(stored(&writer.node, &first).await.is_none());
    })
    .unwrap()
}

#[test]
fn a_merged_edit_deletes_the_peers_unused_chunks_and_a_revert_makes_the_writers_generation() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;
        let first = created(&made.derived)[0].doc_id.clone();
        assert!(stored(&peer, &first).await.is_some());

        let edited = writer.update(&made.doc_id, OTHER).await;
        writer
            .merge_parent(&peer, &edited.commit_cid.unwrap(), &made.doc_id)
            .await;
        assert!(stored(&peer, &first).await.is_none());
        let other = created(&edited.derived)[0].doc_id.clone();
        assert!(stored(&peer, &other).await.is_some());

        let reverted = writer.update(&made.doc_id, BODY).await;
        writer
            .merge_parent(&peer, &reverted.commit_cid.unwrap(), &made.doc_id)
            .await;
        // Both nodes deleted the first generation, so both pick the second.
        let again = created(&reverted.derived)[0].doc_id.clone();
        let built = stored(&peer, &again).await.unwrap();
        assert_eq!(built.get("generation"), Some(&NormalValue::Int(1)));
        assert!(stored(&peer, &other).await.is_none());

        // The writer's own copy arrives later and is the same document.
        let copy = merge_outcome(&writer.node, &peer, &created(&reverted.derived)[0].commit_cid, &again.to_string())
            .await
            .unwrap();
        assert!(
            matches!(&copy, MergeOutcome::Skipped { reason, .. } if MergeOutcome::skip_reason_is_applied(reason)),
            "{copy:?}"
        );
    })
    .unwrap()
}

#[test]
fn a_peer_drops_a_chunk_create_its_live_parent_does_not_use() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;
        let (_, _, stale) = create_chunk(&writer.node, &made.doc_id.to_string(), OTHER, None).await;

        let outcome = merge_outcome(&writer.node, &peer, &stale.cid, &stale.doc_id)
            .await
            .unwrap();

        let MergeOutcome::Skipped {
            reason,
            terminal: true,
        } = &outcome
        else {
            panic!("expected the stale chunk to be dropped, got {outcome:?}");
        };
        assert!(reason.contains("does not use it"), "{reason}");
        assert!(stored(&peer, &DocID::from_string(&stale.doc_id).unwrap())
            .await
            .is_none());
    })
    .unwrap()
}

#[test]
fn a_peer_drops_a_second_copy_of_a_chunk_it_holds() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;
        // A later generation of a chunk the peer already holds live.
        let chunks = writer.node.chunks();
        let held = stored(&peer, &created(&made.derived)[0].doc_id)
            .await
            .unwrap();
        let mut copy = Document::new(chunks.schema_arc());
        for field in ["_sourceID", "text", "hash"] {
            copy.set(field, held.get(field).unwrap().clone()).unwrap();
        }
        copy.set("generation", NormalValue::Int(1)).unwrap();
        let txn = writer.node.db.new_txn(false).await.unwrap();
        let short_id = writer.node.db.next_doc_short_id().await.unwrap();
        let second = db::block::builder::write_document_blocks(
            &txn.blockstore().unwrap(),
            &txn.headstore().unwrap(),
            &copy,
            chunks.schema(),
            db::block::builder::DocRef::new(chunks.resolved_root_id(), short_id),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        txn.force_commit().await.unwrap();

        let outcome = merge_outcome(&writer.node, &peer, &second.cid, &second.doc_id)
            .await
            .unwrap();

        let MergeOutcome::Skipped { reason, .. } = &outcome else {
            panic!("expected the copy to be dropped, got {outcome:?}");
        };
        assert!(reason.contains("already holds it"), "{reason}");
    })
    .unwrap()
}
