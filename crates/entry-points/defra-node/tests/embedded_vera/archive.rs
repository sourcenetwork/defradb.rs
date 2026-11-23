//! A deleted document's object, archived on the node's own Vera deployment.

use super::*;

const WRITER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

fn delete_user(doc: &str, actor: &identity::Did) -> QueryRequest {
    QueryRequest::new("mutation($id:ID!) { delete_Users(docIDs:[$id]) {_docID} }")
        .with_variables(serde_json::json!({ "id": doc }))
        .with_identity(Some(actor.clone()))
}

/// A writer the policy lets delete may not archive, so its delete is
/// refused and nothing is deleted. The owner's delete archives the object
/// with it: the deployment then names no owner and holds no relationship,
/// and a former reader's permission proof fails.
#[test]
#[mantra_macros::req_test("defra.acp.vera_delete_archive")]
fn a_deleted_document_is_archived_on_the_deployment_and_only_its_owner_may_delete_it() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let node = node(directory.path()).await;
        let vera = node.vera_acp().expect("vera.rs document ACP").clone();
        let owner = identity::Did::new(vera.self_did().expect("actor DID")).unwrap();
        let reader = identity::Did::new(STRANGER).unwrap();
        let writer = identity::Did::new(WRITER).unwrap();
        let policy = vera
            .add_policy(owner.as_str(), POLICY)
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

        let refused = node.execute_request(delete_user(&doc, &writer)).await;
        assert!(refused.has_errors(), "a writer may delete but not archive");
        assert!(
            format!("{:?}", refused.errors).contains("not document owner"),
            "{:?}",
            refused.errors
        );
        assert_eq!(
            names(node.execute_request(read_users(&reader)).await),
            ["doomed"]
        );
        assert!(chain_reads(&node, &reader, &policy, &doc).await);

        let deleted = node.execute_request(delete_user(&doc, &owner)).await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
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
