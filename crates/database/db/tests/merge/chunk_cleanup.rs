//! Merges delete the chunks their text no longer uses, drop chunk creates
//! nothing here needs, and make the writer's chunks under its IDs.

use std::sync::Arc;

use defra_core::merge::MergeOutcome;
use document::{DocID, Document, NormalValue};
use query::mutator::{DerivedWrite, DocMutator};

use super::chunk_update_journal::{
    create_chunk, merge_batch_in, merge_outcome, merge_outcome_in, node_with_policy, Node, BODY,
};

const OTHER: &str = "Other text.";

fn created(derived: &[DerivedWrite]) -> Vec<&DerivedWrite> {
    derived
        .iter()
        .filter(|write| write.document.is_some())
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
fn a_merged_edit_deletes_the_peers_unused_chunks_and_a_revert_makes_the_writers_chunks() {
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
        // The revert is a commit of its own, so both nodes make the chunk under it.
        let again = created(&reverted.derived)[0].doc_id.clone();
        assert_ne!(again, first);
        assert_eq!(
            stored(&peer, &again).await.unwrap().get("sourceCID"),
            stored(&writer.node, &again).await.unwrap().get("sourceCID")
        );
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
        // A copy of a chunk the peer holds live, naming a commit the peer merged.
        let chunks = writer.node.chunks();
        let held = stored(&peer, &created(&made.derived)[0].doc_id)
            .await
            .unwrap();
        let mut copy = Document::new(chunks.schema_arc());
        for field in ["_sourceID", "text", "hash"] {
            copy.set(field, held.get(field).unwrap().clone()).unwrap();
        }
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

        let MergeOutcome::Skipped {
            reason,
            terminal: true,
        } = &outcome
        else {
            panic!("expected the copy to be dropped, got {outcome:?}");
        };
        assert!(reason.contains("already holds it"), "{reason}");
        assert!(stored(&peer, &DocID::from_string(&second.doc_id).unwrap())
            .await
            .is_none());
    })
    .unwrap()
}

/// The skip a dropped chunk create gets: terminal, so it is never retried.
fn dropped(outcome: &MergeOutcome) -> &str {
    match outcome {
        MergeOutcome::Skipped {
            reason,
            terminal: true,
        } => reason,
        other => panic!("expected the chunk to be dropped, got {other:?}"),
    }
}

#[test]
fn a_chunk_create_after_its_parents_delete_is_dropped() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;
        let (_, _, late) = create_chunk(&writer.node, &made.doc_id.to_string(), OTHER, None).await;
        let deleted = writer
            .mutator
            .delete("Article", &made.doc_id)
            .await
            .unwrap();
        writer
            .merge_parent(&peer, &deleted.commit_cid.unwrap(), &made.doc_id)
            .await;

        let outcome = merge_outcome(&writer.node, &peer, &late.cid, &late.doc_id)
            .await
            .unwrap();

        assert_eq!(dropped(&outcome), "the chunk's parent is deleted");
        assert!(stored(&peer, &DocID::from_string(&late.doc_id).unwrap())
            .await
            .is_none());
    })
    .unwrap()
}

#[test]
fn copies_of_one_chunk_that_arrive_before_the_parent_keep_the_smallest_doc_id() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        let parent = made.doc_id.to_string();
        // The writer's chunk, and a copy of it that names no commit.
        let own = created(&made.derived)[0];
        let (_, _, copy) = create_chunk(&writer.node, &parent, BODY, None).await;
        let own_id = own.doc_id.to_string();
        assert_ne!(copy.doc_id, own_id);
        for (cid, doc_id) in [(&own.commit_cid, &own_id), (&copy.cid, &copy.doc_id)] {
            let outcome = merge_outcome(&writer.node, &peer, cid, doc_id)
                .await
                .unwrap();
            assert!(outcome.is_merged(), "{outcome:?}");
        }

        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;

        let (kept, gone) = match own_id < copy.doc_id {
            true => (&own_id, &copy.doc_id),
            false => (&copy.doc_id, &own_id),
        };
        assert!(stored(&peer, &DocID::from_string(kept).unwrap())
            .await
            .is_some());
        assert!(stored(&peer, &DocID::from_string(gone).unwrap())
            .await
            .is_none());
    })
    .unwrap()
}

#[test]
fn a_dropped_chunk_in_a_batch_merge_leaves_the_rest_merged() {
    workers::block_on(async {
        let writer = Writer::new().await;
        let peer = node_with_policy(false).await;
        let made = writer.create(BODY).await;
        writer
            .merge_parent(&peer, &made.commit_cid.unwrap(), &made.doc_id)
            .await;
        let parent = made.doc_id.to_string();
        let (_, _, stale) = create_chunk(&writer.node, &parent, "Never used.", None).await;
        let edited = writer.update(&made.doc_id, OTHER).await;
        let articles = writer.node.db.get_collection("Article").unwrap().unwrap();
        let edit = edited.commit_cid.unwrap();
        let blocks = [
            (
                stale.cid,
                stale.doc_id.clone(),
                writer.node.chunks().collection_id().to_string(),
            ),
            (edit, parent.clone(), articles.collection_id().to_string()),
        ];

        let results = merge_batch_in(&writer.node, &peer, &blocks).await;

        let outcomes: Vec<MergeOutcome> = results.into_iter().map(Result::unwrap).collect();
        assert_eq!(dropped(&outcomes[0]), "the chunk's parent does not use it");
        assert!(outcomes[1].is_merged(), "{:?}", outcomes[1]);
        assert!(stored(&peer, &DocID::from_string(&stale.doc_id).unwrap())
            .await
            .is_none());
        let other = created(&edited.derived)[0].doc_id.clone();
        assert!(stored(&peer, &other).await.is_some());
    })
    .unwrap()
}
