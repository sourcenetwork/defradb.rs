//! The arrival journal passes its rows through the same gate: a deleted or
//! purged document of a policy-backed collection is shown to nobody.

use super::{owner, stranger, Node, Path, POLICY, RESOURCE};
use serde_json::Value;

const ARRIVALS: &str =
    r#"{ _documentArrivals(collection: "Users") { head next entries { cursor docID } } }"#;

async fn page(node: &Node, path: Path, identity: Option<identity::Did>) -> (Value, Vec<String>) {
    let data = node.run(path, ARRIVALS, identity).await;
    let page = &data["_documentArrivals"];
    (
        page["next"].clone(),
        page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["docID"].as_str().unwrap().to_string())
            .collect(),
    )
}

#[test]
#[mantra_macros::req_test("defra.events.arrivals.visibility")]
fn a_deleted_private_documents_arrival_is_shown_to_nobody() {
    workers::block_on(async {
        for path in [Path::Registry, Path::AutoCommit] {
            let node = Node::new(true).await;
            let id = node.updated_document("removed").await;
            let kept = node.updated_document("kept").await;
            assert_eq!(
                page(&node, path, Some(owner())).await,
                ("2".into(), vec![id.clone(), kept.clone()]),
                "{path:?}"
            );

            node.delete(&id).await;
            assert!(
                node.acp
                    .is_doc_registered(POLICY, RESOURCE, &id)
                    .await
                    .unwrap(),
                "the delete keeps the document's object, which still decides its history"
            );
            for reader in [None, Some(stranger())] {
                let (next, entries) = page(&node, path, reader.clone()).await;
                assert_eq!(next, "2", "{path:?}: a hidden row still advances the page");
                assert!(entries.is_empty(), "{path:?} as {reader:?}: {entries:?}");
            }
            assert_eq!(
                page(&node, path, Some(owner())).await,
                ("2".into(), vec![kept]),
                "{path:?}: only the deleted document goes"
            );
        }
    })
    .unwrap();
}

#[test]
#[mantra_macros::req_test("defra.events.arrivals.visibility")]
fn a_purged_private_documents_arrival_is_shown_to_nobody() {
    workers::block_on(async {
        for path in [Path::Registry, Path::AutoCommit] {
            let node = Node::new(true).await;
            let id = node.updated_document("removed").await;
            let kept = node.updated_document("kept").await;
            let purged = node
                .db
                .purge_by_doc_ids("Users", &[id.parse::<document::DocID>().unwrap()], None)
                .await
                .unwrap();
            assert_eq!(purged, 1);
            for reader in [None, Some(stranger()), Some(owner())] {
                let (next, entries) = page(&node, path, reader.clone()).await;
                assert_eq!(next, "2", "{path:?}");
                assert!(
                    !entries.contains(&id),
                    "{path:?} as {reader:?}: {entries:?}"
                );
            }
            assert_eq!(page(&node, path, Some(owner())).await.1, [kept]);
        }
    })
    .unwrap();
}
