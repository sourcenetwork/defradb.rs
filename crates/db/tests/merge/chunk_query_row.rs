//! A chunk query decides from the chunk's scan row and its parent, even when
//! the query selects none of the chunk's fields.

use super::chunk_update_journal::{merge_updated_chunk, node, owner, stranger, BODY, CHUNKS};

#[test]
fn a_chunk_query_shows_chunks_only_while_the_reader_may_read_the_live_parent() {
    workers::block_on(async {
        let author = node().await;
        let peer = node().await;
        let created = peer
            .run(
                &format!(r#"mutation {{ add_Article(input: {{body: "{BODY}"}}) {{ _docID }} }}"#),
                Some(owner()),
            )
            .await;
        let parent = created["add_Article"][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string();
        let chunk_id = merge_updated_chunk(&author, &peer, &parent).await;
        let query = format!("{{ {CHUNKS} {{ _docID }} }}");

        let shown = peer.run(&query, Some(owner())).await;
        assert_eq!(shown[CHUNKS][0]["_docID"], chunk_id.as_str());
        assert_eq!(shown[CHUNKS].as_array().unwrap().len(), 1);
        for reader in [None, Some(stranger())] {
            let hidden = peer.run(&query, reader).await;
            assert!(hidden[CHUNKS].as_array().unwrap().is_empty());
        }

        peer.run(
            &format!(r#"mutation {{ delete_Article(docID: "{parent}") {{ _docID }} }}"#),
            Some(owner()),
        )
        .await;
        let after_delete = peer.run(&query, Some(owner())).await;
        assert!(after_delete[CHUNKS].as_array().unwrap().is_empty());
    })
    .unwrap()
}
