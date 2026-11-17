//! Chunk queries of every shape go through the chunk gate: counts, explains,
//! `_version` selections and filters an index can serve.

use serde_json::json;

use super::chunk_surfaces::{add_article, doc_ids, readers};
use super::chunk_update_journal::{merge_chunk, node, owner, Node, BODY, CHUNKS};
use identity::Did;

/// A peer holding one merged chunk of an owner's article, and the chunk's ID.
async fn one_chunk() -> (Node, String, String) {
    let author = node().await;
    let peer = node().await;
    let parent = add_article(&peer).await;
    let (chunk_id, _) = merge_chunk(&author, &peer, &parent, BODY, None).await;
    (peer, parent, chunk_id)
}

#[test]
fn a_chunk_count_counts_only_chunks_the_reader_may_read() {
    workers::block_on(async {
        let (peer, _, _) = one_chunk().await;
        let count = |reader: Option<Did>| {
            let peer = &peer;
            async move {
                peer.run(&format!("{{ COUNT({CHUNKS}: {{}}) }}"), reader)
                    .await["COUNT"]
                    .clone()
            }
        };

        assert_eq!(count(Some(owner())).await, 1);
        for reader in readers() {
            assert_eq!(count(reader).await, 0);
        }
    })
    .unwrap()
}

#[test]
fn a_chunk_query_with_its_version_shows_only_to_a_reader_of_its_parent() {
    workers::block_on(async {
        let (peer, _, chunk_id) = one_chunk().await;
        let query = format!("{{ {CHUNKS} {{ _docID _version {{ cid }} }} }}");

        let shown = peer.run(&query, Some(owner())).await;
        assert_eq!(doc_ids(&shown[CHUNKS]), [chunk_id]);
        assert!(!shown[CHUNKS][0]["_version"].as_array().unwrap().is_empty());
        for reader in readers() {
            assert!(doc_ids(&peer.run(&query, reader).await[CHUNKS]).is_empty());
        }
    })
    .unwrap()
}

#[test]
fn a_chunk_filtered_on_its_indexed_fields_still_needs_its_parents_reader() {
    workers::block_on(async {
        let (peer, parent, chunk_id) = one_chunk().await;
        let all = peer
            .run(&format!("{{ {CHUNKS} {{ hash text }} }}"), Some(owner()))
            .await;
        let (hash, text) = (&all[CHUNKS][0]["hash"], &all[CHUNKS][0]["text"]);
        let query = format!(
            r#"{{ {CHUNKS}(filter: {{_sourceID: {{_eq: "{parent}"}}, hash: {{_eq: {hash}}}}}) {{ _docID _sourceID text hash }} }}"#
        );

        let shown = peer.run(&query, Some(owner())).await;
        assert_eq!(
            shown[CHUNKS],
            json!([{"_docID": chunk_id, "_sourceID": parent, "text": text, "hash": hash}])
        );
        for reader in readers() {
            assert!(doc_ids(&peer.run(&query, reader).await[CHUNKS]).is_empty());
        }
    })
    .unwrap()
}

#[test]
fn a_chunk_query_can_be_explained() {
    workers::block_on(async {
        let (peer, _, _) = one_chunk().await;
        for explain in ["@explain", "@explain(type: execute)"] {
            let plan = peer
                .run(
                    &format!("query {explain} {{ {CHUNKS} {{ _docID }} }}"),
                    Some(owner()),
                )
                .await;
            assert!(plan["explain"]["executionErrors"].is_null(), "{plan}");
            assert!(plan.to_string().contains("permissionFilterNode"), "{plan}");
        }
    })
    .unwrap()
}
