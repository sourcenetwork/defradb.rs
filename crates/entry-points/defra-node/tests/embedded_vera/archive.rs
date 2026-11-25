//! A deleted document's object, archived on the node's own Vera deployment.

use super::*;

const WRITER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

fn delete_user(doc: &str, actor: &identity::Did) -> QueryRequest {
    QueryRequest::new("mutation($id:ID!) { delete_Users(docIDs:[$id]) {_docID} }")
        .with_variables(serde_json::json!({ "id": doc }))
        .with_identity(Some(actor.clone()))
}

/// A reader the policy does not let delete is refused before anything
/// commits. A writer the policy lets delete, who does not own the document,
/// deletes it: the document is denied at once to its owner, the writer and
/// the reader, and the archive is sent as the owner, whose key the node
/// holds. The deployment then names no owner and holds no relationship, and
/// a former reader's permission proof fails.
#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_writers_delete_archives_the_object_as_its_owner_and_a_reader_may_not_delete() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let writer = identity::Did::new(WRITER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        node.add_collection(&users(&policy))
            .await
            .expect("collection");
        let doc = created(node.execute_request(create_user("doomed", &owner)).await);
        for (relation, target) in [("reader", &reader), ("writer", &writer)] {
            node.add_dac_actor_relationship(
                owner.as_str(),
                "Users",
                &doc,
                relation,
                target.as_str(),
            )
            .await
            .expect("grant");
        }
        assert!(chain_reads(&node, &reader, &policy, &doc).await);

        let refused = node.execute_request(delete_user(&doc, &reader)).await;
        assert!(
            refused.has_errors(),
            "a reader may not delete: {:?}",
            refused.data
        );
        assert_eq!(
            names(node.execute_request(read_users(&reader)).await),
            ["doomed"]
        );
        assert!(chain_reads(&node, &reader, &policy, &doc).await);

        let deleted = node.execute_request(delete_user(&doc, &writer)).await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        for actor in [&owner, &writer, &reader] {
            assert!(
                names(node.execute_request(read_users(actor)).await).is_empty(),
                "{actor} still reads the deleted document"
            );
            assert!(!reads_doc(&node, actor, &policy, &doc).await);
        }
        let owed_by_writer = node
            .document_acp()
            .owed_entries(acp::OwedFilter::Actor(&writer))
            .await
            .expect("owed entries");
        assert!(
            owed_by_writer.is_empty(),
            "the archive is the owner's: {owed_by_writer:?}"
        );
        // The archive settles behind the commit; until it does, it denies.
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while vera
            .get_doc_owner(&policy, "users", &doc)
            .await
            .expect("owner proof")
            .is_some()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "the archive never settled"
            );
            edge::testing::edge()
                .sleep(Duration::from_millis(200))
                .await;
        }
        assert!(!chain_reads(&node, &reader, &policy, &doc).await);
        assert!(!reads_doc(&node, &reader, &policy, &doc).await);
        assert!(!vera
            .check_named_permission(&reader, "reader", &policy, "users", &doc)
            .await
            .expect("relation check"));
        assert!(names(node.execute_request(read_users(&reader)).await).is_empty());
        drop(vera);
        node.shutdown().await.expect("shutdown");
    })
    .unwrap();
}
