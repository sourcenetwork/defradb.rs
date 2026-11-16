//! Every read surface shows a merged chunk only to a reader of its live parent
//! whose current text uses it.

use std::sync::Arc;

use async_trait::async_trait;
use crypto::{Key as _, PrivateKey as _};
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_core::RequestContext;
use document::DocID;
use identity::Did;
use query::access_hooks::{ReadRequest, ReadValidator};
use schema::CollectionVersion;
use serde_json::Value;

use super::chunk_update_journal::{
    merge_chunk, merge_updated_chunk, node, owner, stranger, Node, BODY, CHUNKS,
};

const NEW_BODY: &str = "A new short story.";

async fn add_article(node: &Node) -> String {
    let created = node
        .run(
            &format!(r#"mutation {{ add_Article(input: {{body: "{BODY}"}}) {{ _docID }} }}"#),
            Some(owner()),
        )
        .await;
    created["add_Article"][0]["_docID"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn delete_article(node: &Node, parent: &str) {
    node.run(
        &format!(r#"mutation {{ delete_Article(docID: "{parent}") {{ _docID }} }}"#),
        Some(owner()),
    )
    .await;
}

fn readers() -> [Option<Did>; 2] {
    [None, Some(stranger())]
}

fn signing() -> SigningConfig {
    let key = crypto::generate_ed25519().unwrap();
    let public = key.public_key();
    SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: key.raw().to_vec(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: None,
        signing_authorization: None,
    }
}

fn doc_ids(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["_docID"].as_str().unwrap().to_string())
        .collect()
}

async fn chunks(node: &Node, reader: Option<Did>) -> Vec<String> {
    let data = node
        .run(&format!("{{ {CHUNKS} {{ _docID }} }}"), reader)
        .await;
    doc_ids(&data[CHUNKS])
}

async fn commits(node: &Node, chunk_id: &str, reader: Option<Did>) -> usize {
    let data = node
        .run(
            &format!(r#"{{ _commits(docID: "{chunk_id}") {{ cid }} }}"#),
            reader,
        )
        .await;
    data["_commits"].as_array().unwrap().len()
}

async fn version(node: &Node, cid: &cid::Cid, reader: Option<Did>) -> Vec<String> {
    let data = node
        .run(
            &format!(r#"{{ {CHUNKS}(cid: "{cid}") {{ _docID }} }}"#),
            reader,
        )
        .await;
    doc_ids(&data[CHUNKS])
}

async fn arrivals(node: &Node, reader: Option<Did>) -> Vec<String> {
    let data = node
        .run(
            &format!(
                r#"{{ _documentArrivals(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#
            ),
            reader,
        )
        .await;
    data["_documentArrivals"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["docID"].as_str().unwrap().to_string())
        .collect()
}

async fn signed_bytes(node: &Node, cid: &cid::Cid, reader: Option<Did>) -> Result<(), String> {
    db::block::verify::authorized_signed_block_bytes_as(
        &RequestContext::default(),
        &node.db,
        node.acp.as_ref(),
        &cid.to_string(),
        &acp::Identity::from(reader),
    )
    .await
    .map(|_| ())
}

#[test]
fn a_chunk_block_is_served_signed_only_to_a_reader_of_its_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (_, cid) = merge_chunk(&author, &peer, &parent, BODY, Some(&signing())).await;

        signed_bytes(&peer, &cid, Some(owner())).await.unwrap();
        for reader in readers() {
            let denied = signed_bytes(&peer, &cid, reader).await.unwrap_err();
            assert!(denied.contains("missing permission"), "{denied}");
        }

        // Blocks are history, so a deleted parent's reader still gets them.
        delete_article(&peer, &parent).await;
        signed_bytes(&peer, &cid, Some(owner())).await.unwrap();
    })
    .unwrap()
}

#[test]
fn a_chunk_block_whose_parent_has_not_arrived_is_not_served() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let missing = DocID::new_v0_from_seed("parent not merged yet").to_string();
        let (_, cid) = merge_chunk(&author, &peer, &missing, BODY, Some(&signing())).await;

        let denied = signed_bytes(&peer, &cid, Some(owner())).await.unwrap_err();
        assert!(denied.contains("parent is not known here"), "{denied}");
    })
    .unwrap()
}

#[test]
fn a_chunks_commits_show_only_to_a_reader_of_its_live_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (chunk_id, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;

        assert!(commits(&peer, &chunk_id, Some(owner())).await > 0);
        for reader in readers() {
            assert_eq!(commits(&peer, &chunk_id, reader).await, 0);
        }
        delete_article(&peer, &parent).await;
        assert_eq!(commits(&peer, &chunk_id, Some(owner())).await, 0);
    })
    .unwrap()
}

#[test]
fn a_chunk_at_a_version_shows_only_to_a_reader_of_its_live_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (chunk_id, cid) = merge_chunk(&author, &peer, &parent, BODY, None).await;

        assert_eq!(version(&peer, &cid, Some(owner())).await, [chunk_id]);
        for reader in readers() {
            assert!(version(&peer, &cid, reader).await.is_empty());
        }
        delete_article(&peer, &parent).await;
        assert!(version(&peer, &cid, Some(owner())).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_chunks_arrival_shows_only_to_a_reader_of_its_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (chunk_id, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;

        assert_eq!(arrivals(&peer, Some(owner())).await, [chunk_id]);
        for reader in readers() {
            assert!(arrivals(&peer, reader).await.is_empty());
        }
    })
    .unwrap()
}

#[test]
fn a_parent_edit_hides_the_old_chunk_and_shows_the_new_one() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let (old, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;
        assert_eq!(chunks(&peer, Some(owner())).await, std::slice::from_ref(&old));

        peer.run(
            &format!(
                r#"mutation {{ update_Article(docID: "{parent}", input: {{body: "{NEW_BODY}"}}) {{ _docID }} }}"#
            ),
            Some(owner()),
        )
        .await;
        let (new, _) = merge_chunk(&author, &peer, &parent, NEW_BODY, None).await;

        assert_eq!(chunks(&peer, Some(owner())).await, std::slice::from_ref(&new));
        assert_eq!(commits(&peer, &old, Some(owner())).await, 0);
        assert!(commits(&peer, &new, Some(owner())).await > 0);
    })
    .unwrap()
}

/// An app read validator that governs only the chunk collection and refuses every read.
struct RefuseChunks;

#[async_trait]
impl ReadValidator for RefuseChunks {
    fn governs(&self, collection: &CollectionVersion) -> bool {
        collection.name == CHUNKS
    }

    async fn may_read(&self, _request: &ReadRequest<'_>) -> Result<bool, String> {
        Ok(false)
    }
}

#[test]
fn the_chunk_collections_own_app_validator_hides_chunks_from_a_parent_reader() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let parent = add_article(&peer).await;
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let refuse: Arc<dyn ReadValidator> = Arc::new(RefuseChunks);
        let run = |query: String| {
            let refuse = Arc::clone(&refuse);
            let peer = &peer;
            async move { peer.run_with(&query, Some(owner()), Some(refuse)).await }
        };

        let listed = run(format!("{{ {CHUNKS} {{ _docID }} }}")).await;
        assert!(doc_ids(&listed[CHUNKS]).is_empty());
        let history = run(format!(r#"{{ _commits(docID: "{chunk_id}") {{ cid }} }}"#)).await;
        assert!(history["_commits"].as_array().unwrap().is_empty());
        let updates = run(format!(
            r#"{{ _documentUpdates(collection: "{CHUNKS}") {{ entries {{ docID }} }} }}"#
        ))
        .await;
        assert!(updates["_documentUpdates"]["entries"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(peer.journal(Some(owner())).await, [chunk_id]);
    })
    .unwrap()
}
