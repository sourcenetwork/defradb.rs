//! A chunk's parent decides its reads: its own policy and app validator, and
//! each parent apart from the others.

use std::sync::Arc;

use async_trait::async_trait;
use query::access_hooks::{ReadRequest, ReadValidator};
use schema::CollectionVersion;

use super::chunk_surfaces::{
    add_article, add_article_with, arrivals, chunks, commits, doc_ids, readers,
};
use super::chunk_update_journal::{
    merge_chunk, node, node_with_policy, owner, stranger, BODY, CHUNKS,
};

/// An app read validator that governs only the parent collection and refuses every read.
struct RefuseArticles;

#[async_trait]
impl ReadValidator for RefuseArticles {
    fn governs(&self, collection: &CollectionVersion) -> bool {
        collection.name == "Article"
    }

    async fn may_read(&self, _request: &ReadRequest<'_>) -> Result<bool, String> {
        Ok(false)
    }
}

#[test]
fn the_parents_app_validator_hides_its_chunks() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (chunk_id, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;
        assert_eq!(
            chunks(&peer, Some(owner())).await,
            std::slice::from_ref(&chunk_id)
        );
        let refuse: Arc<dyn ReadValidator> = Arc::new(RefuseArticles);
        let run = |query: String| {
            let refuse = Arc::clone(&refuse);
            let peer = &peer;
            async move { peer.run_with(&query, Some(owner()), Some(refuse)).await }
        };

        let listed = run(format!("{{ {CHUNKS} {{ _docID }} }}")).await;
        assert!(doc_ids(&listed[CHUNKS]).is_empty());
        let history = run(format!(r#"{{ _commits(docID: "{chunk_id}") {{ cid }} }}"#)).await;
        assert!(history["_commits"].as_array().unwrap().is_empty());
    })
    .unwrap()
}

#[test]
fn a_chunk_of_a_parent_without_a_policy_shows_to_every_reader() {
    workers::block_on(async {
        let author = node_with_policy(false).await;
        let peer = node_with_policy(false).await;
        let parent = add_article(&peer).await;
        let (chunk_id, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;

        for reader in readers() {
            assert_eq!(
                chunks(&peer, reader.clone()).await,
                std::slice::from_ref(&chunk_id)
            );
            assert!(commits(&peer, &chunk_id, reader).await > 0);
        }
    })
    .unwrap()
}

const SHARED: &str = "A shared opening sentence that both of these articles begin with.";

#[test]
fn chunks_of_two_parents_are_decided_per_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let mine_body = format!("{SHARED} Mine goes on.");
        let theirs_body = format!("{SHARED} Theirs goes on.");
        let mine = add_article_with(&peer, owner(), &mine_body).await;
        let theirs = add_article_with(&peer, stranger(), &theirs_body).await;
        let (my_chunk, _) = merge_chunk(&author, &peer, &mine, &mine_body, None).await;
        let (their_chunk, _) = merge_chunk(&author, &peer, &theirs, &theirs_body, None).await;

        // The peer's own writes add the rest of each parent's chunks.
        for (reader, chunk, other) in [
            (owner(), my_chunk.clone(), their_chunk.clone()),
            (stranger(), their_chunk, my_chunk),
        ] {
            let shown = chunks(&peer, Some(reader.clone())).await;
            assert!(
                shown.contains(&chunk) && !shown.contains(&other),
                "{shown:?}"
            );
            let arrived = arrivals(&peer, Some(reader)).await;
            assert!(
                arrived.contains(&chunk) && !arrived.contains(&other),
                "{arrived:?}"
            );
        }
        // Both first chunks are the shared sentence, so only the parent tells them apart.
        let hash = |reader| {
            let peer = &peer;
            async move {
                peer.run(&format!("{{ {CHUNKS} {{ hash }} }}"), Some(reader))
                    .await[CHUNKS][0]["hash"]
                    .clone()
            }
        };
        assert_eq!(hash(owner()).await, hash(stranger()).await);
    })
    .unwrap()
}
